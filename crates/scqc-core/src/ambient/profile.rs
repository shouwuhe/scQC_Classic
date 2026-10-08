//! 环境表达谱（SPEC §8.2.4 / §8.6【硬约束】：整个流程只计算一次，各模块共用）。
//!
//! 来源：raw 矩阵中 0 < 总 UMI ≤ `umi_upper` 的空液滴（默认与 EmptyDrops 的 `lower` 相同，
//! SoupX `estimateSoup` 的默认 soupRange 亦为 (0, 100]）。
//!
//! 两种归一化形式：
//! - `fraction`：计数和归一化（零计数基因为 0）—— SoupX 式估计器与校正使用；
//! - `good_turing`：Good–Turing 平滑（零计数基因非零）—— EmptyDrops 的 Dirichlet-multinomial 与
//!   DecontX 的背景分布使用（避免 log 0）。

use serde::{Deserialize, Serialize};

use crate::matrix::CountMatrix;
use crate::qc::goodturing::good_turing_proportions;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AmbientProfile {
    pub umi_upper: u64,
    pub n_droplets: usize,
    pub total_umi: u64,
    /// 各基因在空液滴池中的计数和
    pub counts: Vec<u64>,
    /// 计数和归一化
    pub fraction: Vec<f64>,
    /// Good–Turing 平滑后的概率
    pub good_turing: Vec<f64>,
}

impl AmbientProfile {
    /// 从 raw 矩阵构建。`totals` 为每列总 UMI（调用方通常已算过）。
    pub fn from_raw(matrix: &CountMatrix, totals: &[u64], umi_upper: u64) -> Self {
        let g = matrix.n_genes();
        let mut counts = vec![0u64; g];
        let mut n_droplets = 0usize;
        let mut total_umi = 0u64;
        for (j, &t) in totals.iter().enumerate().take(matrix.n_barcodes()) {
            if t == 0 || t > umi_upper {
                continue;
            }
            n_droplets += 1;
            total_umi += t;
            let (idx, val) = matrix.column(j);
            for (&gi, &v) in idx.iter().zip(val) {
                counts[gi as usize] += v as u64;
            }
        }
        let fraction: Vec<f64> = if total_umi > 0 {
            counts
                .iter()
                .map(|&c| c as f64 / total_umi as f64)
                .collect()
        } else {
            vec![0.0; g]
        };
        let good_turing = good_turing_proportions(&counts);
        Self {
            umi_upper,
            n_droplets,
            total_umi,
            counts,
            fraction,
            good_turing,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.n_droplets == 0 || self.total_umi == 0
    }
}
