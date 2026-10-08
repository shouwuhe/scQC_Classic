//! cxds —— 基于共表达的双细胞打分（SPEC §8.5.1）。
//!
//! 依据：Bais AS & Kostka D (2020) "scds: computational annotation of doublets in single-cell
//! RNA sequencing data", Bioinformatics 36(4):1150–1158；参考实现 scds（MIT）。
//!
//! 算法（与 scds `cxds` 一致）：
//! 1. 二值化 B = counts > binThresh(0)；p_g = 表达细胞比例；按 p(1−p) 取前 ntop(500) 个基因；
//! 2. 对每个基因对 (i, j)：obs_ij = 恰好表达其中一个的细胞数，prb_ij = p_i(1−p_j) + p_j(1−p_i)，
//!    S_ij = log P(X ≥ obs_ij)，X ~ Binomial(n, prb_ij)（互斥表达显著多于随机 → S 很负）；
//! 3. 细胞得分 = −Σ_{i,j 在该细胞中均表达} S_ij（同时表达"互斥标记对"的细胞得分高）。
//!
//! 无 PCA、无 kNN。每细胞得分独立计算，逐位可复现。

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use statrs::distribution::{Binomial, DiscreteCDF};

use crate::embed::normalize::chunk_bounds;
use crate::matrix::CountMatrix;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CxdsParams {
    pub ntop: usize,
    pub bin_thresh: u32,
}

impl Default for CxdsParams {
    fn default() -> Self {
        Self {
            ntop: 500,
            bin_thresh: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CxdsResult {
    pub score: Vec<f64>,
    pub genes_used: Vec<usize>,
}

pub fn cxds(cells: &CountMatrix, params: &CxdsParams) -> CxdsResult {
    let g = cells.n_genes();
    let n = cells.n_barcodes();
    if n == 0 {
        return CxdsResult {
            score: Vec::new(),
            genes_used: Vec::new(),
        };
    }
    // 1. 表达细胞数
    let mut expr = vec![0u32; g];
    for j in 0..n {
        let (idx, val) = cells.column(j);
        for (&gi, &v) in idx.iter().zip(val) {
            if v > params.bin_thresh {
                expr[gi as usize] += 1;
            }
        }
    }
    let nf = n as f64;
    let mut order: Vec<usize> = (0..g).collect();
    let var_of = |gi: usize| {
        let p = expr[gi] as f64 / nf;
        p * (1.0 - p)
    };
    order.sort_by(|&a, &b| {
        var_of(b)
            .partial_cmp(&var_of(a))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    let top: Vec<usize> = order.into_iter().take(params.ntop).collect();
    let m = top.len();
    let mut pos = vec![u32::MAX; g];
    for (k, &gi) in top.iter().enumerate() {
        pos[gi] = k as u32;
    }

    // 每细胞在 top 基因中的表达列表
    let expressed: Vec<Vec<u32>> = (0..n)
        .map(|j| {
            let (idx, val) = cells.column(j);
            let mut e: Vec<u32> = idx
                .iter()
                .zip(val)
                .filter(|(&gi, &v)| v > params.bin_thresh && pos[gi as usize] != u32::MAX)
                .map(|(&gi, _)| pos[gi as usize])
                .collect();
            e.sort_unstable();
            e
        })
        .collect();

    // 2. 共表达计数 c_ij（固定分块并行 + 顺序合并）
    let partials: Vec<Vec<u32>> = chunk_bounds(n)
        .par_iter()
        .map(|&(a, b)| {
            let mut c = vec![0u32; m * m];
            for e in expressed.iter().take(b).skip(a) {
                for (x, &i) in e.iter().enumerate() {
                    for &j in &e[x + 1..] {
                        c[i as usize * m + j as usize] += 1;
                    }
                }
            }
            c
        })
        .collect();
    let mut co = vec![0u32; m * m];
    for p in &partials {
        for (o, v) in co.iter_mut().zip(p) {
            *o += v;
        }
    }
    let e_top: Vec<f64> = top.iter().map(|&gi| expr[gi] as f64).collect();
    let mut s = vec![0.0f64; m * m];
    for i in 0..m {
        for j in i + 1..m {
            let cij = co[i * m + j] as f64;
            let obs = e_top[i] + e_top[j] - 2.0 * cij;
            let pi = e_top[i] / nf;
            let pj = e_top[j] / nf;
            let prb = (pi * (1.0 - pj) + pj * (1.0 - pi)).clamp(1e-12, 1.0 - 1e-12);
            // log P(X ≥ obs) = log sf(obs − 1)
            let v = match Binomial::new(prb, n as u64) {
                Ok(d) => {
                    let sf = if obs <= 0.0 {
                        1.0
                    } else {
                        d.sf(obs as u64 - 1)
                    };
                    sf.max(f64::MIN_POSITIVE).ln()
                }
                Err(_) => 0.0,
            };
            s[i * m + j] = v;
            s[j * m + i] = v;
        }
    }

    // 3. 细胞得分
    let score: Vec<f64> = expressed
        .par_iter()
        .map(|e| {
            let mut acc = 0.0f64;
            for (x, &i) in e.iter().enumerate() {
                for &j in &e[x + 1..] {
                    acc += s[i as usize * m + j as usize];
                }
            }
            -2.0 * acc
        })
        .collect();
    CxdsResult {
        score,
        genes_used: top,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::matrix::Triplet;

    /// 两个细胞类型各自表达互斥基因；人工"双细胞"同时表达两组 → 得分应最高。
    #[test]
    fn doublets_score_highest() {
        let g = 40;
        let mut trip = Vec::new();
        let mut col = 0u32;
        let push_cell = |genes: &[usize], col: u32, trip: &mut Vec<Triplet>| {
            for &gi in genes {
                trip.push(Triplet {
                    row: gi as u32,
                    col,
                    value: 3,
                });
            }
        };
        let a: Vec<usize> = (0..15).collect();
        let b: Vec<usize> = (20..35).collect();
        for _ in 0..40 {
            push_cell(&a, col, &mut trip);
            col += 1;
        }
        for _ in 0..40 {
            push_cell(&b, col, &mut trip);
            col += 1;
        }
        let first_dbl = col;
        for _ in 0..5 {
            let mut both = a.clone();
            both.extend(&b);
            push_cell(&both, col, &mut trip);
            col += 1;
        }
        let m = CountMatrix::from_triplets(g, col as usize, trip).unwrap();
        let r = cxds(
            &m,
            &CxdsParams {
                ntop: 30,
                bin_thresh: 0,
            },
        );
        let max_single = r.score[..first_dbl as usize]
            .iter()
            .cloned()
            .fold(f64::MIN, f64::max);
        for j in first_dbl as usize..col as usize {
            assert!(
                r.score[j] > max_single,
                "doublet {} score {} <= {}",
                j,
                r.score[j],
                max_single
            );
        }
    }
}
