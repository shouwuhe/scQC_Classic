//! 高变基因（HVG）选择。
//!
//! 依据：Satija et al. 2015（Seurat "mean.var.plot" / Scanpy `flavor="seurat"`）的分箱离散度法：
//! 在 CP10k 值上求每基因均值 μ 与方差 σ²，离散度 d = σ²/μ；取 log(d) 与 log1p(μ)，
//! 按 log1p(μ) 分 20 箱，在箱内对 log(d) 做 z-score，取归一化离散度最高的 n 个基因。
//! 只考虑 `candidates` 中的基因（Gene Expression 类型）。

use super::normalize::GeneStats;

pub fn select_hvg(
    stats: &GeneStats,
    candidates: &[usize],
    n_top: usize,
    n_bins: usize,
) -> Vec<usize> {
    // 只保留均值 > 0 且离散度有限的基因
    let mut genes: Vec<(usize, f64, f64)> = candidates
        .iter()
        .filter_map(|&g| {
            let mu = stats.mean_cp10k[g];
            if mu <= 0.0 {
                return None;
            }
            let d = stats.var_cp10k[g] / mu;
            if d <= 0.0 || !d.is_finite() {
                return None;
            }
            Some((g, mu.ln_1p(), d.ln()))
        })
        .collect();
    if genes.is_empty() {
        return Vec::new();
    }
    let n_bins = n_bins.max(1);
    let lo = genes.iter().map(|x| x.1).fold(f64::INFINITY, f64::min);
    let hi = genes.iter().map(|x| x.1).fold(f64::NEG_INFINITY, f64::max);
    let width = ((hi - lo) / n_bins as f64).max(1e-12);
    let bin_of = |m: f64| (((m - lo) / width) as usize).min(n_bins - 1);

    // 箱内均值与标准差
    let mut sum = vec![0.0f64; n_bins];
    let mut sumsq = vec![0.0f64; n_bins];
    let mut cnt = vec![0usize; n_bins];
    for &(_, m, ld) in &genes {
        let b = bin_of(m);
        sum[b] += ld;
        sumsq[b] += ld * ld;
        cnt[b] += 1;
    }
    let mut norm: Vec<(usize, f64)> = genes
        .iter()
        .map(|&(g, m, ld)| {
            let b = bin_of(m);
            let c = cnt[b] as f64;
            let mean = sum[b] / c;
            let var = if cnt[b] > 1 {
                (sumsq[b] - sum[b] * sum[b] / c) / (c - 1.0)
            } else {
                0.0
            };
            let sd = var.max(0.0).sqrt();
            // 单基因箱：Scanpy 约定归一化离散度记 1
            let z = if cnt[b] <= 1 || sd == 0.0 {
                1.0
            } else {
                (ld - mean) / sd
            };
            (g, z)
        })
        .collect();
    // 按归一化离散度降序，并列按基因索引升序（确定性）
    norm.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    let mut out: Vec<usize> = norm.iter().take(n_top).map(|x| x.0).collect();
    out.sort_unstable();
    genes.clear();
    out
}
