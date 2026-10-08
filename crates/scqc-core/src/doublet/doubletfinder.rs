//! DoubletFinder —— 人工双细胞近邻法（SPEC §8.5.2）。
//!
//! 依据：McGinnis CS, Murrow LM, Gartner ZJ (2019) "DoubletFinder: Doublet Detection in
//! Single-Cell RNA Sequencing Data Using Artificial Nearest Neighbors", Cell Systems 8(4):329–337；
//! 参考实现 DoubletFinder（CC0）。
//!
//! 算法：
//! 1. 随机配对真实细胞生成人工双细胞（计数相加；CP10k 归一化后与"取平均"等价），人工占比 pN；
//! 2. 真实 + 人工 → 归一化 → HVG → 缩放 → PCA（与 `embed` 同一套代码，只算一次）；
//! 3. 每个真实细胞的 pANN = 其 k 近邻中人工双细胞的比例，k = round(pK · N_merged)；
//! 4. 参数扫描（paramSweep）：pN ∈ {0.05,…,0.30}、pK ∈ {0.0005, 0.001, 0.005, 0.01,…,0.30}，
//!    每个 (pN, pK) 计算 pANN 分布的双峰系数 BC；BCmvn(pK) = mean_pN(BC)/var_pN(BC)，取最大者为 pK*；
//!    与原实现一致，细胞数 > 10000 时扫描在 10000 个随机细胞的子样本上进行；
//! 5. 用 pN = 0.25、pK* 在全部细胞上计算 pANN，按预期双细胞率取 pANN 最高的 nExp 个为双细胞。
//!
//! 性能（SPEC §8.5.2 要求）：每个 pN 只做一次 PCA、一次 kNN（一次取 k_max 个近邻，各 pK 取前缀），
//! 扫描与 kNN 用 rayon 并行；kNN 为精确搜索，逐位可复现（DESIGN §12）。

use rand::seq::SliceRandom;
use rand::Rng;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::embed::knn::sq_dist;
use crate::embed::{hvg, normalize, pca};
use crate::error::Result;
use crate::matrix::{CountMatrix, Triplet};
use crate::pipeline::cancel::CancelToken;
use crate::rng::derive_rng;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DoubletFinderParams {
    pub p_n: f64,
    pub sweep_p_n: Vec<f64>,
    pub sweep_p_k: Vec<f64>,
    pub sweep_max_cells: usize,
    pub n_hvg: usize,
    pub n_pcs: usize,
    pub seed: u64,
    /// 指定 pK 时跳过参数扫描（对照验证 / 用户手动指定）
    pub fixed_p_k: Option<f64>,
}

impl Default for DoubletFinderParams {
    fn default() -> Self {
        let mut pk = vec![0.0005, 0.001, 0.005];
        for i in 1..=30 {
            pk.push(i as f64 / 100.0);
        }
        Self {
            p_n: 0.25,
            sweep_p_n: vec![0.05, 0.10, 0.15, 0.20, 0.25, 0.30],
            sweep_p_k: pk,
            sweep_max_cells: 10_000,
            n_hvg: 2000,
            n_pcs: 50,
            seed: crate::rng::DEFAULT_SEED,
            fixed_p_k: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SweepRow {
    pub p_k: f64,
    pub mean_bc: f64,
    pub var_bc: f64,
    pub bcmvn: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DoubletFinderResult {
    pub p_ann: Vec<f64>,
    pub call: Vec<bool>,
    pub p_k: f64,
    pub p_n: f64,
    pub n_expected: usize,
    pub n_artificial: usize,
    pub sweep: Vec<SweepRow>,
    pub sweep_n_cells: usize,
}

/// 生成人工双细胞并拼接：返回 (合并矩阵, 人工列起始索引)。
fn merged_with_doublets(
    cells: &CountMatrix,
    sub: &[usize],
    p_n: f64,
    seed: u64,
    tag: u64,
) -> Result<(CountMatrix, usize)> {
    let n_real = sub.len();
    let n_art = ((p_n * n_real as f64) / (1.0 - p_n)).round() as usize;
    let mut rng = derive_rng(seed, "doubletfinder", tag);
    let mut trip: Vec<Triplet> = Vec::new();
    for (new_j, &j) in sub.iter().enumerate() {
        let (idx, val) = cells.column(j);
        for (&g, &v) in idx.iter().zip(val) {
            trip.push(Triplet {
                row: g,
                col: new_j as u32,
                value: v,
            });
        }
    }
    for a in 0..n_art {
        let i = rng.gen_range(0..n_real);
        let mut j = rng.gen_range(0..n_real);
        while j == i && n_real > 1 {
            j = rng.gen_range(0..n_real);
        }
        let col = (n_real + a) as u32;
        for &src in &[sub[i], sub[j]] {
            let (idx, val) = cells.column(src);
            for (&g, &v) in idx.iter().zip(val) {
                trip.push(Triplet {
                    row: g,
                    col,
                    value: v,
                });
            }
        }
    }
    let m = CountMatrix::from_triplets(cells.n_genes(), n_real + n_art, trip)?; // 重复坐标自动相加
    Ok((m, n_real))
}

/// 合并矩阵 → PCA 得分（n_merged × k）。
fn pca_scores(
    merged: &CountMatrix,
    gene_candidates: &[usize],
    params: &DoubletFinderParams,
) -> (Vec<f32>, usize) {
    let totals = merged.column_sums();
    let stats = normalize::gene_stats(merged, &totals);
    let hv = hvg::select_hvg(&stats, gene_candidates, params.n_hvg, 20);
    let dense = normalize::dense_scaled(merged, &totals, &hv, &stats, 10.0);
    let p = pca::randomized_pca(
        &dense,
        merged.n_barcodes(),
        hv.len(),
        &pca::PcaParams {
            n_components: params.n_pcs,
            seed: params.seed,
            ..Default::default()
        },
    );
    (p.scores, p.k)
}

/// 对每个真实细胞（0..n_real）计算在各 k 值下的人工近邻数。返回 n_real × ks.len()。
fn artificial_neighbor_counts(
    scores: &[f32],
    dim: usize,
    n_real: usize,
    n_merged: usize,
    ks: &[usize],
) -> Vec<Vec<u32>> {
    let k_max = ks
        .iter()
        .copied()
        .max()
        .unwrap_or(1)
        .min(n_merged - 1)
        .max(1);
    (0..n_real)
        .into_par_iter()
        .map(|i| {
            let xi = &scores[i * dim..(i + 1) * dim];
            let mut d: Vec<(f32, u32)> = Vec::with_capacity(n_merged - 1);
            for j in 0..n_merged {
                if j == i {
                    continue;
                }
                d.push((sq_dist(xi, &scores[j * dim..(j + 1) * dim]), j as u32));
            }
            if d.len() > k_max {
                d.select_nth_unstable_by(k_max - 1, |a, b| {
                    a.0.partial_cmp(&b.0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(a.1.cmp(&b.1))
                });
                d.truncate(k_max);
            }
            d.sort_by(|a, b| {
                a.0.partial_cmp(&b.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.1.cmp(&b.1))
            });
            // 前缀累计人工数
            let mut cum = vec![0u32; d.len() + 1];
            for (t, &(_, j)) in d.iter().enumerate() {
                cum[t + 1] = cum[t] + if (j as usize) >= n_real { 1 } else { 0 };
            }
            ks.iter().map(|&k| cum[k.min(d.len())]).collect()
        })
        .collect()
}

/// Sarle 双峰系数（样本修正版，与 `modes::bimodality_coefficient` 一致）。
pub fn bimodality_coefficient(x: &[f64]) -> f64 {
    let n = x.len() as f64;
    if x.len() < 4 {
        return f64::NAN;
    }
    let mean = x.iter().sum::<f64>() / n;
    let m2 = x.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    if m2 <= 0.0 {
        return f64::NAN;
    }
    let m3 = x.iter().map(|v| (v - mean).powi(3)).sum::<f64>() / n;
    let m4 = x.iter().map(|v| (v - mean).powi(4)).sum::<f64>() / n;
    let g1 = m3 / m2.powf(1.5);
    let g2 = m4 / (m2 * m2) - 3.0;
    let skew = g1 * (n * (n - 1.0)).sqrt() / (n - 2.0);
    let kurt = ((n + 1.0) * g2 + 6.0) * (n - 1.0) / ((n - 2.0) * (n - 3.0));
    (skew * skew + 1.0) / (kurt + 3.0 * (n - 1.0).powi(2) / ((n - 2.0) * (n - 3.0)))
}

/// 运行 DoubletFinder。`expected_rate` 为预期双细胞率（由 Profile 计算）。
pub fn doublet_finder(
    cells: &CountMatrix,
    gene_candidates: &[usize],
    expected_rate: f64,
    params: &DoubletFinderParams,
    cancel: &CancelToken,
    on_progress: &dyn Fn(u8, &str),
) -> Result<DoubletFinderResult> {
    let n = cells.n_barcodes();
    if n < 20 {
        return Ok(DoubletFinderResult {
            p_ann: vec![0.0; n],
            call: vec![false; n],
            p_k: f64::NAN,
            p_n: params.p_n,
            n_expected: 0,
            n_artificial: 0,
            sweep: Vec::new(),
            sweep_n_cells: n,
        });
    }
    // ---- 4. 参数扫描（子样本）----
    let mut sub: Vec<usize> = (0..n).collect();
    if n > params.sweep_max_cells {
        let mut rng = derive_rng(params.seed, "doubletfinder-subsample", 0);
        sub.shuffle(&mut rng);
        sub.truncate(params.sweep_max_cells);
        sub.sort_unstable();
    }
    let n_pk = params.sweep_p_k.len();
    let mut bc_table: Vec<Vec<f64>> = vec![Vec::new(); n_pk]; // 每个 pK 的 BC（按 pN）
    let sweep_p_n: &[f64] = if params.fixed_p_k.is_some() {
        &[]
    } else {
        &params.sweep_p_n
    };
    for (pi, &p_n) in sweep_p_n.iter().enumerate() {
        cancel.checkpoint()?;
        on_progress(
            (5 + pi * 60 / sweep_p_n.len().max(1)) as u8,
            &format!("DoubletFinder 参数扫描 pN={p_n:.2}"),
        );
        let (merged, n_real) =
            merged_with_doublets(cells, &sub, p_n, params.seed, 1000 + pi as u64)?;
        let n_merged = merged.n_barcodes();
        let (scores, dim) = pca_scores(&merged, gene_candidates, params);
        let ks: Vec<usize> = params
            .sweep_p_k
            .iter()
            .map(|&pk| ((pk * n_merged as f64).round() as usize).max(1))
            .collect();
        let counts = artificial_neighbor_counts(&scores, dim, n_real, n_merged, &ks);
        for (qi, &k) in ks.iter().enumerate() {
            let pann: Vec<f64> = counts.iter().map(|c| c[qi] as f64 / k as f64).collect();
            bc_table[qi].push(bimodality_coefficient(&pann));
        }
    }
    let sweep: Vec<SweepRow> = params
        .sweep_p_k
        .iter()
        .zip(&bc_table)
        .map(|(&pk, bcs)| {
            let valid: Vec<f64> = bcs.iter().copied().filter(|v| v.is_finite()).collect();
            let m = valid.len() as f64;
            let mean = if m > 0.0 {
                valid.iter().sum::<f64>() / m
            } else {
                f64::NAN
            };
            let var = if m > 1.0 {
                valid.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (m - 1.0)
            } else {
                f64::NAN
            };
            let bcmvn = if var > 0.0 { mean / var } else { f64::NAN };
            SweepRow {
                p_k: pk,
                mean_bc: mean,
                var_bc: var,
                bcmvn,
            }
        })
        .collect();
    let best = sweep
        .iter()
        .filter(|r| r.bcmvn.is_finite())
        .max_by(|a, b| {
            a.bcmvn
                .partial_cmp(&b.bcmvn)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|r| r.p_k)
        .unwrap_or(0.09);

    // ---- 5. 全量运行 ----
    cancel.checkpoint()?;
    on_progress(70, &format!("DoubletFinder 全量计算（pK={best}）"));
    let all: Vec<usize> = (0..n).collect();
    let (merged, n_real) = merged_with_doublets(cells, &all, params.p_n, params.seed, 2000)?;
    let n_merged = merged.n_barcodes();
    let (scores, dim) = pca_scores(&merged, gene_candidates, params);
    let k = ((best * n_merged as f64).round() as usize).max(1);
    let counts = artificial_neighbor_counts(&scores, dim, n_real, n_merged, &[k]);
    let p_ann: Vec<f64> = counts.iter().map(|c| c[0] as f64 / k as f64).collect();
    let n_expected = ((expected_rate * n as f64).round() as usize).min(n);
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        p_ann[b]
            .partial_cmp(&p_ann[a])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    let mut call = vec![false; n];
    for &j in order.iter().take(n_expected) {
        call[j] = true;
    }
    on_progress(100, "DoubletFinder 完成");
    Ok(DoubletFinderResult {
        p_ann,
        call,
        p_k: best,
        p_n: params.p_n,
        n_expected,
        n_artificial: n_merged - n_real,
        sweep,
        sweep_n_cells: sub.len(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn bc_of_bimodal_is_higher_than_unimodal() {
        let uni: Vec<f64> = (0..200)
            .map(|i| (i as f64 - 100.0) / 30.0)
            .map(|z| z.exp())
            .collect();
        let bi: Vec<f64> = (0..200)
            .map(|i| if i % 2 == 0 { 0.0 } else { 1.0 })
            .collect();
        assert!(bimodality_coefficient(&bi) > bimodality_coefficient(&uni));
    }
}
