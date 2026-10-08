//! 共享近邻（SNN）图。
//!
//! 依据：Xu & Su (2015) SNN-Cliq；Seurat `FindNeighbors` 的定义：
//! 节点 i 的邻居集 N(i) 含自身与 k 个近邻；边权 w_ij = |N(i)∩N(j)| / |N(i)∪N(j)|（Jaccard），
//! 只对 j ∈ N(i) 或 i ∈ N(j) 的对计算；低于 `prune`（默认 1/15）的边删除。
//! 输出为对称 CSR 加权图，供 Leiden 使用。

use std::collections::HashMap;

use super::knn::KnnGraph;

/// 对称加权图（CSR，每条无向边存两次）。
#[derive(Debug, Clone)]
pub struct WeightedGraph {
    pub n: usize,
    pub ptr: Vec<usize>,
    pub adj: Vec<u32>,
    pub w: Vec<f32>,
}

impl WeightedGraph {
    pub fn neighbors(&self, i: usize) -> (&[u32], &[f32]) {
        (
            &self.adj[self.ptr[i]..self.ptr[i + 1]],
            &self.w[self.ptr[i]..self.ptr[i + 1]],
        )
    }

    pub fn n_edges(&self) -> usize {
        self.adj.len() / 2
    }

    /// 由 (u, v, w) 列表构建（自动对称化、合并重复边取最大权）。
    pub fn from_edges(n: usize, edges: &[(u32, u32, f32)]) -> Self {
        let mut map: HashMap<(u32, u32), f32> = HashMap::with_capacity(edges.len() * 2);
        for &(u, v, w) in edges {
            if u == v {
                continue;
            }
            let e = map.entry((u, v)).or_insert(0.0);
            *e = e.max(w);
            let e = map.entry((v, u)).or_insert(0.0);
            *e = e.max(w);
        }
        let mut list: Vec<((u32, u32), f32)> = map.into_iter().collect();
        list.sort_by_key(|x| x.0);
        let mut ptr = vec![0usize; n + 1];
        for &((u, _), _) in &list {
            ptr[u as usize + 1] += 1;
        }
        for i in 0..n {
            ptr[i + 1] += ptr[i];
        }
        let adj: Vec<u32> = list.iter().map(|x| x.0 .1).collect();
        let w: Vec<f32> = list.iter().map(|x| x.1).collect();
        Self { n, ptr, adj, w }
    }
}

pub fn snn_graph(knn: &KnnGraph, prune: f32) -> WeightedGraph {
    let n = knn.n;
    let k = knn.k;
    // N(i) = {i} ∪ 邻居，排序便于交集计算
    let sets: Vec<Vec<u32>> = (0..n)
        .map(|i| {
            let mut s: Vec<u32> = knn
                .neighbors(i)
                .0
                .iter()
                .copied()
                .filter(|&x| x != u32::MAX)
                .collect();
            s.push(i as u32);
            s.sort_unstable();
            s.dedup();
            s
        })
        .collect();
    let jaccard = |a: &[u32], b: &[u32]| -> f32 {
        let (mut i, mut j, mut inter) = (0, 0, 0usize);
        while i < a.len() && j < b.len() {
            match a[i].cmp(&b[j]) {
                std::cmp::Ordering::Less => i += 1,
                std::cmp::Ordering::Greater => j += 1,
                std::cmp::Ordering::Equal => {
                    inter += 1;
                    i += 1;
                    j += 1;
                }
            }
        }
        let union = a.len() + b.len() - inter;
        if union == 0 {
            0.0
        } else {
            inter as f32 / union as f32
        }
    };
    let mut edges: Vec<(u32, u32, f32)> = Vec::with_capacity(n * k);
    for i in 0..n {
        for &j in knn.neighbors(i).0 {
            if j == u32::MAX {
                continue;
            }
            let w = jaccard(&sets[i], &sets[j as usize]);
            if w >= prune {
                edges.push((i as u32, j, w));
            }
        }
    }
    WeightedGraph::from_edges(n, &edges)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_cliques_have_no_cross_edges() {
        // 两组各 5 点，kNN 只在组内
        let n = 10;
        let k = 4;
        let mut idx = Vec::new();
        for i in 0..n {
            let base = if i < 5 { 0 } else { 5 };
            for j in 0..5 {
                if base + j != i {
                    idx.push((base + j) as u32);
                }
            }
        }
        let g = snn_graph(
            &KnnGraph {
                n,
                k,
                idx,
                dist: vec![1.0; n * k],
            },
            1.0 / 15.0,
        );
        for i in 0..n {
            for &j in g.neighbors(i).0 {
                assert_eq!((i < 5), ((j as usize) < 5));
            }
            assert_eq!(g.neighbors(i).0.len(), 4);
            assert!(g.neighbors(i).1.iter().all(|&w| (w - 1.0).abs() < 1e-6));
        }
    }
}
