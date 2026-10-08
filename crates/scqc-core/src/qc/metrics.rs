//! 逐 barcode 基础指标：总 UMI、基因数、线粒体比例、核糖体比例。
//!
//! 基因集合由 `GeneSets` 给出（来自物种规则，见 `platform::species`）。
//! 核糖体比例总是计算；是否使用由 `PlatformProfile` 的字段决定，
//! 本函数不问平台（SPEC §16）。

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::matrix::CountMatrix;

/// 基因索引集合。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GeneSets {
    pub mito: Vec<usize>,
    pub ribo: Vec<usize>,
}

/// 每个 barcode 一行。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BasicMetrics {
    pub total_umi: Vec<u64>,
    pub n_genes: Vec<u32>,
    /// 0–1 的比例；总 UMI 为 0 时为 0
    pub pct_mito: Vec<f64>,
    pub pct_ribo: Vec<f64>,
}

pub fn compute_basic_metrics(matrix: &CountMatrix, sets: &GeneSets) -> BasicMetrics {
    let n_genes_total = matrix.n_genes();
    let mut is_mito = vec![false; n_genes_total];
    let mut is_ribo = vec![false; n_genes_total];
    for &g in &sets.mito {
        if g < n_genes_total {
            is_mito[g] = true;
        }
    }
    for &g in &sets.ribo {
        if g < n_genes_total {
            is_ribo[g] = true;
        }
    }

    // 逐列并行；每列结果独立，按索引写回 → 与线程数无关（确定性）。
    let rows: Vec<(u64, u32, f64, f64)> = (0..matrix.n_barcodes())
        .into_par_iter()
        .map(|j| {
            let (idx, val) = matrix.column(j);
            let mut total = 0u64;
            let mut mito = 0u64;
            let mut ribo = 0u64;
            for (&g, &v) in idx.iter().zip(val) {
                total += v as u64;
                if is_mito[g as usize] {
                    mito += v as u64;
                }
                if is_ribo[g as usize] {
                    ribo += v as u64;
                }
            }
            let frac = |x: u64| {
                if total == 0 {
                    0.0
                } else {
                    x as f64 / total as f64
                }
            };
            (total, idx.len() as u32, frac(mito), frac(ribo))
        })
        .collect();

    let mut m = BasicMetrics {
        total_umi: Vec::with_capacity(rows.len()),
        n_genes: Vec::with_capacity(rows.len()),
        pct_mito: Vec::with_capacity(rows.len()),
        pct_ribo: Vec::with_capacity(rows.len()),
    };
    for (t, n, pm, pr) in rows {
        m.total_umi.push(t);
        m.n_genes.push(n);
        m.pct_mito.push(pm);
        m.pct_ribo.push(pr);
    }
    m
}

impl BasicMetrics {
    pub fn len(&self) -> usize {
        self.total_umi.len()
    }

    pub fn is_empty(&self) -> bool {
        self.total_umi.is_empty()
    }

    /// 中位数（用于 summary 与识别证据）。
    pub fn median_total_umi(&self) -> f64 {
        median_u64(&self.total_umi)
    }

    pub fn median_pct_mito(&self) -> f64 {
        median_f64(&self.pct_mito)
    }
}

pub(crate) fn median_u64(v: &[u64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_unstable();
    let n = s.len();
    if n % 2 == 1 {
        s[n / 2] as f64
    } else {
        (s[n / 2 - 1] as f64 + s[n / 2] as f64) / 2.0
    }
}

pub(crate) fn median_f64(v: &[f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = s.len();
    if n % 2 == 1 {
        s[n / 2]
    } else {
        (s[n / 2 - 1] + s[n / 2]) / 2.0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::matrix::Triplet;

    #[test]
    fn metrics_on_tiny_matrix() {
        // 基因 0 = MT-, 基因 1 = RPS, 基因 2 = 普通
        let t = vec![
            Triplet {
                row: 0,
                col: 0,
                value: 2,
            },
            Triplet {
                row: 1,
                col: 0,
                value: 3,
            },
            Triplet {
                row: 2,
                col: 0,
                value: 5,
            },
            Triplet {
                row: 2,
                col: 1,
                value: 1,
            },
        ];
        let m = CountMatrix::from_triplets(3, 3, t).unwrap();
        let sets = GeneSets {
            mito: vec![0],
            ribo: vec![1],
        };
        let bm = compute_basic_metrics(&m, &sets);
        assert_eq!(bm.total_umi, vec![10, 1, 0]);
        assert_eq!(bm.n_genes, vec![3, 1, 0]);
        assert!((bm.pct_mito[0] - 0.2).abs() < 1e-12);
        assert!((bm.pct_ribo[0] - 0.3).abs() < 1e-12);
        assert_eq!(bm.pct_mito[2], 0.0);
        assert_eq!(bm.median_total_umi(), 1.0);
    }
}
