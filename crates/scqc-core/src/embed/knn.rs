//! kNN 图（SPEC §8.6，k = 20）。
//!
//! 默认 **精确暴力搜索**（`KnnMethod::Exact`）：对每个点计算到全部点的欧氏距离，
//! 取最近的 k 个。每行独立计算、无跨线程归约，因此**结果与线程数无关且逐位可复现**
//! （DESIGN §12）。5 万细胞 × 50 维约 1.25×10¹¹ 次乘加，内层循环按 8 路累加以便自动向量化。
//!
//! `KnnMethod::Hnsw` 使用 `hnsw_rs`（Malkov & Yashunin 2016），速度更快，但该库的层数
//! 随机化未暴露种子，**同一输入两次运行结果不同**（2026-09-11 在 PBMC 3k 上实测），
//! 违反 SPEC §14.5 的可复现性硬约束，因此不作为默认。

use hnsw_rs::prelude::*;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnnMethod {
    Exact,
    Hnsw,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnnParams {
    pub k: usize,
    pub method: KnnMethod,
    pub max_nb_connection: usize,
    pub ef_construction: usize,
    pub ef_search: usize,
}

impl Default for KnnParams {
    fn default() -> Self {
        Self {
            k: 20,
            method: KnnMethod::Exact,
            max_nb_connection: 16,
            ef_construction: 200,
            ef_search: 100,
        }
    }
}

/// n 个点的 kNN 图（每点 k 个邻居，扁平存储，按距离升序）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnnGraph {
    pub n: usize,
    pub k: usize,
    /// n × k，邻居索引（不含自身）；不足 k 时用 u32::MAX 填充
    pub idx: Vec<u32>,
    /// n × k，对应欧氏距离
    pub dist: Vec<f32>,
}

impl KnnGraph {
    pub fn neighbors(&self, i: usize) -> (&[u32], &[f32]) {
        (
            &self.idx[i * self.k..(i + 1) * self.k],
            &self.dist[i * self.k..(i + 1) * self.k],
        )
    }
}

/// 8 路累加的平方欧氏距离（便于自动向量化，且求和顺序固定）。
#[inline]
pub fn sq_dist(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = [0.0f32; 8];
    let ca = a.chunks_exact(8);
    let cb = b.chunks_exact(8);
    let ra = ca.remainder();
    let rb = cb.remainder();
    for (x, y) in ca.zip(cb) {
        for l in 0..8 {
            let d = x[l] - y[l];
            acc[l] += d * d;
        }
    }
    let mut s = ((acc[0] + acc[1]) + (acc[2] + acc[3])) + ((acc[4] + acc[5]) + (acc[6] + acc[7]));
    for (x, y) in ra.iter().zip(rb) {
        let d = x - y;
        s += d * d;
    }
    s
}

/// `data`：n × dim 行主序。
pub fn knn_graph(data: &[f32], n: usize, dim: usize, params: &KnnParams) -> KnnGraph {
    let k = params.k.min(n.saturating_sub(1)).max(1);
    if n < 2 {
        return KnnGraph {
            n,
            k,
            idx: vec![u32::MAX; n * k],
            dist: vec![f32::INFINITY; n * k],
        };
    }
    let rows: Vec<(Vec<u32>, Vec<f32>)> = match params.method {
        KnnMethod::Exact => (0..n)
            .into_par_iter()
            .map(|i| exact_row(data, n, dim, i, k))
            .collect(),
        KnnMethod::Hnsw => hnsw_rows(data, n, dim, k, params),
    };
    let mut idx = Vec::with_capacity(n * k);
    let mut dist = Vec::with_capacity(n * k);
    for (a, b) in rows {
        idx.extend(a);
        dist.extend(b);
    }
    KnnGraph { n, k, idx, dist }
}

fn cmp_pair(a: &(f32, u32), b: &(f32, u32)) -> std::cmp::Ordering {
    a.0.partial_cmp(&b.0)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then(a.1.cmp(&b.1))
}

fn exact_row(data: &[f32], n: usize, dim: usize, i: usize, k: usize) -> (Vec<u32>, Vec<f32>) {
    let xi = &data[i * dim..(i + 1) * dim];
    let mut d: Vec<(f32, u32)> = Vec::with_capacity(n - 1);
    for j in 0..n {
        if j == i {
            continue;
        }
        let xj = &data[j * dim..(j + 1) * dim];
        d.push((sq_dist(xi, xj), j as u32));
    }
    if d.len() > k {
        d.select_nth_unstable_by(k - 1, cmp_pair);
        d.truncate(k);
    }
    d.sort_by(cmp_pair);
    let idx: Vec<u32> = d.iter().map(|p| p.1).collect();
    let dist: Vec<f32> = d.iter().map(|p| p.0.max(0.0).sqrt()).collect();
    (idx, dist)
}

fn hnsw_rows(
    data: &[f32],
    n: usize,
    dim: usize,
    k: usize,
    params: &KnnParams,
) -> Vec<(Vec<u32>, Vec<f32>)> {
    let hnsw = Hnsw::<f32, DistL2>::new(
        params.max_nb_connection,
        n,
        16,
        params.ef_construction,
        DistL2 {},
    );
    for i in 0..n {
        hnsw.insert((&data[i * dim..(i + 1) * dim], i));
    }
    let ef = params.ef_search.max(k + 1);
    (0..n)
        .into_par_iter()
        .map(|i| {
            let q = &data[i * dim..(i + 1) * dim];
            let mut res: Vec<(f32, u32)> = hnsw
                .search(q, k + 1, ef)
                .into_iter()
                .filter(|nb| nb.d_id != i)
                .map(|nb| (nb.distance, nb.d_id as u32))
                .collect();
            res.sort_by(cmp_pair);
            res.truncate(k);
            let mut idx: Vec<u32> = res.iter().map(|r| r.1).collect();
            let mut dist: Vec<f32> = res.iter().map(|r| r.0).collect();
            idx.resize(k, u32::MAX);
            dist.resize(k, f32::INFINITY);
            (idx, dist)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_true_neighbors_on_grid() {
        // 1D 等距点：每点最近的两个邻居必然是左右相邻点
        let n = 50;
        let data: Vec<f32> = (0..n).flat_map(|i| [i as f32 * 10.0, 0.0]).collect();
        let g = knn_graph(
            &data,
            n,
            2,
            &KnnParams {
                k: 2,
                ..Default::default()
            },
        );
        for i in 1..n - 1 {
            let (nb, d) = g.neighbors(i);
            let mut v = nb.to_vec();
            v.sort_unstable();
            assert_eq!(
                v,
                vec![(i - 1) as u32, (i + 1) as u32],
                "i={i} nb={nb:?} d={d:?}"
            );
            assert!((d[0] - 10.0).abs() < 1e-5);
        }
    }

    #[test]
    fn sq_dist_matches_naive() {
        let a: Vec<f32> = (0..50).map(|i| i as f32 * 0.5).collect();
        let b: Vec<f32> = (0..50).map(|i| (50 - i) as f32 * 0.25).collect();
        let naive: f32 = a.iter().zip(&b).map(|(x, y)| (x - y) * (x - y)).sum();
        assert!((sq_dist(&a, &b) - naive).abs() < 1e-3 * naive);
    }
}
