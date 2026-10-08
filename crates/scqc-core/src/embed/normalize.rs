//! 归一化与逐基因统计（SPEC §8.6）。
//!
//! log1p(CP10k) 只用于内部计算，**不改变输出的原始计数**。
//! 所有并行归约都先按固定分块产生部分和，再顺序求和（DESIGN §12：与线程数无关）。

use rayon::prelude::*;

use crate::matrix::CountMatrix;

/// 归一化目标总数（CP10k）。
pub const SCALE: f64 = 1e4;

/// 逐基因的均值与方差（分别在 CP10k 值和 log1p(CP10k) 值上）。
#[derive(Debug, Clone)]
pub struct GeneStats {
    pub mean_cp10k: Vec<f64>,
    pub var_cp10k: Vec<f64>,
    pub mean_log: Vec<f64>,
    pub var_log: Vec<f64>,
    pub n_cells: usize,
}

/// 固定分块数，保证并行部分和的合并顺序固定。
pub(crate) const N_CHUNKS: usize = 64;

pub(crate) fn chunk_bounds(n: usize) -> Vec<(usize, usize)> {
    let chunks = N_CHUNKS.min(n.max(1));
    (0..chunks)
        .map(|c| (n * c / chunks, n * (c + 1) / chunks))
        .collect()
}

/// 计算每个基因在所有细胞上的 CP10k / log 均值与方差（总体方差以 n-1 为分母）。
pub fn gene_stats(matrix: &CountMatrix, totals: &[u64]) -> GeneStats {
    let g = matrix.n_genes();
    let n = matrix.n_barcodes();
    // 每块：sum, sumsq（cp10k 与 log 各一组）
    let partials: Vec<[Vec<f64>; 4]> = chunk_bounds(n)
        .par_iter()
        .map(|&(a, b)| {
            let mut s = [vec![0.0f64; g], vec![0.0; g], vec![0.0; g], vec![0.0; g]];
            for (j, &t) in totals.iter().enumerate().take(b).skip(a) {
                if t == 0 {
                    continue;
                }
                let (idx, val) = matrix.column(j);
                for (&gi, &v) in idx.iter().zip(val) {
                    let x = v as f64 / t as f64 * SCALE;
                    let l = x.ln_1p();
                    let gi = gi as usize;
                    s[0][gi] += x;
                    s[1][gi] += x * x;
                    s[2][gi] += l;
                    s[3][gi] += l * l;
                }
            }
            s
        })
        .collect();
    let mut sum = [vec![0.0f64; g], vec![0.0; g], vec![0.0; g], vec![0.0; g]];
    for p in &partials {
        for k in 0..4 {
            for (acc, v) in sum[k].iter_mut().zip(&p[k]) {
                *acc += v;
            }
        }
    }
    let nf = n as f64;
    let denom = (nf - 1.0).max(1.0);
    let var = |s1: &[f64], s2: &[f64]| -> Vec<f64> {
        s1.iter()
            .zip(s2)
            .map(|(&a, &b)| ((b - a * a / nf) / denom).max(0.0))
            .collect()
    };
    GeneStats {
        var_cp10k: var(&sum[0], &sum[1]),
        var_log: var(&sum[2], &sum[3]),
        mean_cp10k: sum[0].iter().map(|v| v / nf).collect(),
        mean_log: sum[2].iter().map(|v| v / nf).collect(),
        n_cells: n,
    }
}

/// 构建细胞 × HVG 的稠密 z-score 矩阵（行主序，f32），值裁剪到 [-clip, clip]。
///
/// z = (log1p(cp10k) − mean_log) / sd_log；sd 为 0 的基因整列为 0。
pub fn dense_scaled(
    matrix: &CountMatrix,
    totals: &[u64],
    hvg: &[usize],
    stats: &GeneStats,
    clip: f32,
) -> Vec<f32> {
    let n = matrix.n_barcodes();
    let h = hvg.len();
    // 基因 → HVG 列号
    let mut col_of = vec![u32::MAX; matrix.n_genes()];
    for (k, &g) in hvg.iter().enumerate() {
        col_of[g] = k as u32;
    }
    let inv_sd: Vec<f32> = hvg
        .iter()
        .map(|&g| {
            let sd = stats.var_log[g].sqrt();
            if sd > 0.0 {
                (1.0 / sd) as f32
            } else {
                0.0
            }
        })
        .collect();
    let mean: Vec<f32> = hvg.iter().map(|&g| stats.mean_log[g] as f32).collect();

    let mut out = vec![0.0f32; n * h];
    out.par_chunks_mut(h).enumerate().for_each(|(j, row)| {
        // 先填 "零计数" 的 z 值：(0 − mean)/sd
        for k in 0..h {
            row[k] = (-mean[k] * inv_sd[k]).clamp(-clip, clip);
        }
        let t = totals[j];
        if t == 0 {
            return;
        }
        let (idx, val) = matrix.column(j);
        for (&g, &v) in idx.iter().zip(val) {
            let k = col_of[g as usize];
            if k != u32::MAX {
                let l = (v as f64 / t as f64 * SCALE).ln_1p() as f32;
                let k = k as usize;
                row[k] = ((l - mean[k]) * inv_sd[k]).clamp(-clip, clip);
            }
        }
    });
    out
}
