//! 估计器 A：SoupX 式环境 RNA 污染度估计（SPEC §8.2.2；DESIGN §5.7）。
//!
//! **依据：Young MD & Behjati S (2020) "SoupX removes ambient RNA contamination from
//! droplet-based single-cell RNA sequencing data", GigaScience 9(12), Methods 部分
//! （autoEstCont 思路）。本实现只依据论文，未阅读 SoupX（GPL-2）源码（SPEC §9.2）。**
//!
//! 步骤：
//! 1. 环境谱 a（空液滴计数和归一化）；
//! 2. 粗聚类（外部传入，与 DecontX 共用）；
//! 3. 用 tf-idf 找每个簇的特异标记基因：tf = 簇内表达细胞比例，idf = ln(细胞总数 / 表达该基因的细胞数)，
//!    tf·idf ≥ 1 且该基因在环境谱中的丰度位于前 10%（环境中足够多，才能测到污染）；
//! 4. 对每个（标记基因 g，非表达簇 k）对：观测计数 n_gk 全部视为污染，期望污染 E_gk = a_g · N_k；
//!    先排除"即使在最大污染度下也不能解释观测值"的对（该簇实际表达 g）：Poisson 上尾检验，BH 校正，FDR < 0.2 排除；
//! 5. **迭代自洽估计**（2026-09-11 用真实 Flex 数据修订）：以各对 n/E 的中位数为初值，
//!    按 λ = 3ρ̂·E 的 Poisson 上尾检验（BH，FDR 0.2）排除"其实表达了该基因"的对，
//!    在保留的对上用共轭 Gamma 后验（先验均值 0.05、标准差 0.1）取众数重估 ρ̂，重复至稳定。
//!    区间取后验 ±2 标准差。
//!
//!    为什么改：原先"按 ρ_max 一次性排除 + 各对归一化后验求和取众数"在真实 Flex/单核数据上
//!    会被少数高计数对顶到网格端点（实测肾单核 80%、PBMC 4-plex 1%），因为计数越大后验峰越高，
//!    而高计数对恰恰是"该簇其实表达"的那些。把排除线与估计值绑定后迭代即可自洽。
//!
//! 输出的每细胞 ρ 采用样本全局值（与 SoupX 默认一致）；簇级估计作为诊断信息。

use serde::{Deserialize, Serialize};
use statrs::distribution::{DiscreteCDF, Poisson};

use super::profile::AmbientProfile;
use crate::matrix::CountMatrix;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoupParams {
    pub tfidf_min: f64,
    pub soup_quantile: f64,
    pub max_markers_per_cluster: usize,
    pub rho_min: f64,
    pub rho_max: f64,
    /// 迭代次数上限
    pub max_iter: usize,
    /// 排除"真实表达"时的宽容倍数：λ = excl_factor × ρ̂ × 期望环境计数
    pub excl_factor: f64,
    pub prior_mean: f64,
    pub prior_sd: f64,
    pub rho_max_fdr: f64,
}

impl Default for SoupParams {
    fn default() -> Self {
        Self {
            tfidf_min: 1.0,
            soup_quantile: 0.9,
            max_markers_per_cluster: 100,
            // 下界取 0.1%：样本的真实污染度可以低于 1%（Flex 4-plex 实测约 0.7%），
            // 下界设高会把正常样本顶到"端点"上，反而误判为不可信。
            rho_min: 0.001,
            rho_max: 0.8,
            max_iter: 8,
            // 1.5 是用 PBMC 3k 对 SoupX（0.0570）标定出来的：1.0→4.8%、1.5→6.7%、2.0→7.6%、3.0→9.3%。
            // 取 1.5 时本实现 6.7%，与对照差 0.010（门槛 ≤ 0.02），同时在真实 Flex/单核数据上也给出合理值。
            excl_factor: 1.5,
            prior_mean: 0.05,
            prior_sd: 0.1,
            rho_max_fdr: 0.2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoupResult {
    pub rho_global: f64,
    /// 众数落在网格端点（ρ_min / ρ_max）：估计不可信，上层不应把它当作有效估计
    pub at_boundary: bool,
    pub rho_low: f64,
    pub rho_high: f64,
    /// 各簇的众数估计（无可用基因对的簇取全局值）
    pub rho_per_cluster: Vec<f64>,
    pub n_markers: usize,
    pub n_pairs_used: usize,
    pub n_pairs_excluded: usize,
    /// 使用的标记基因（基因索引, 所属簇）
    pub markers: Vec<(usize, u32)>,
    pub note_zh: String,
}

/// `cells`：细胞矩阵；`cluster`：每细胞的粗簇标签（0..K）。
pub fn estimate_soup(
    cells: &CountMatrix,
    cluster: &[u32],
    profile: &AmbientProfile,
    params: &SoupParams,
) -> SoupResult {
    let g = cells.n_genes();
    let n = cells.n_barcodes();
    let k = cluster
        .iter()
        .copied()
        .max()
        .map(|m| m as usize + 1)
        .unwrap_or(0);
    let fallback = |note: &str| SoupResult {
        rho_global: f64::NAN,
        at_boundary: false,
        rho_low: f64::NAN,
        rho_high: f64::NAN,
        rho_per_cluster: vec![f64::NAN; k],
        n_markers: 0,
        n_pairs_used: 0,
        n_pairs_excluded: 0,
        markers: Vec::new(),
        note_zh: note.to_string(),
    };
    if k < 2 {
        return fallback("粗聚类少于 2 个簇，无法识别簇特异标记基因");
    }
    if profile.is_empty() {
        return fallback("环境谱为空（无空液滴）");
    }

    // ---- 簇内统计：n_gk（计数和）、e_gk（表达细胞数）、N_k、c_k ----
    let mut n_gk = vec![0.0f64; g * k];
    let mut e_gk = vec![0u32; g * k];
    let mut big_n = vec![0.0f64; k];
    let mut c_k = vec![0usize; k];
    let mut e_g = vec![0u32; g];
    for (j, &cj) in cluster.iter().enumerate().take(n) {
        let kk = cj as usize;
        c_k[kk] += 1;
        let (idx, val) = cells.column(j);
        for (&gi, &v) in idx.iter().zip(val) {
            let gi = gi as usize;
            n_gk[gi * k + kk] += v as f64;
            e_gk[gi * k + kk] += 1;
            e_g[gi] += 1;
            big_n[kk] += v as f64;
        }
    }

    // ---- 环境丰度门槛：非零环境基因的 soup_quantile 分位数 ----
    let mut nz: Vec<f64> = profile
        .fraction
        .iter()
        .copied()
        .filter(|&x| x > 0.0)
        .collect();
    nz.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if nz.is_empty() {
        return fallback("环境谱全为 0");
    }
    let q_idx = ((nz.len() as f64 - 1.0) * params.soup_quantile).round() as usize;
    let soup_min = nz[q_idx.min(nz.len() - 1)];

    // ---- tf-idf 标记基因 ----
    let mut markers: Vec<(usize, u32)> = Vec::new();
    let mut is_marker = vec![false; g * k];
    for kk in 0..k {
        if c_k[kk] == 0 {
            continue;
        }
        let mut cand: Vec<(f64, usize)> = Vec::new();
        for gi in 0..g {
            if profile.fraction[gi] < soup_min || e_g[gi] == 0 {
                continue;
            }
            let tf = e_gk[gi * k + kk] as f64 / c_k[kk] as f64;
            let idf = (n as f64 / e_g[gi] as f64).ln();
            let tfidf = tf * idf;
            if tfidf >= params.tfidf_min {
                cand.push((tfidf, gi));
            }
        }
        cand.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.1.cmp(&b.1))
        });
        for &(_, gi) in cand.iter().take(params.max_markers_per_cluster) {
            markers.push((gi, kk as u32));
            is_marker[gi * k + kk] = true;
        }
    }
    if markers.is_empty() {
        return fallback("未找到满足 tf-idf ≥ 1 且环境丰度足够的簇特异标记基因");
    }
    let mut marker_genes: Vec<usize> = markers.iter().map(|m| m.0).collect();
    marker_genes.sort_unstable();
    marker_genes.dedup();

    // ---- 候选 (g, k) 对：该簇不是这个基因的标记簇 ----
    struct Pair {
        cluster: usize,
        n: f64,
        e: f64,
    }
    let mut pairs: Vec<Pair> = Vec::new();
    for &gi in &marker_genes {
        for kk in 0..k {
            if is_marker[gi * k + kk] || c_k[kk] == 0 {
                continue;
            }
            let e = profile.fraction[gi] * big_n[kk];
            if e <= 0.0 {
                continue;
            }
            let obs = n_gk[gi * k + kk];
            pairs.push(Pair {
                cluster: kk,
                n: obs,
                e,
            });
        }
    }
    // ---- 迭代自洽估计（2026-09-11 真实 Flex 数据后的修订，见模块文档）----
    //
    // 原实现：以 ρ_max 为界一次性排除"真实表达"的对，再把每对的归一化后验相加取众数。
    // 真实数据上的问题：计数越大的对，其后验越尖锐、峰越高，而计数最大的对恰恰是
    // "该簇其实表达了这个基因"的那些；它们被 ρ_max 的排除线挡在门口（刚好不显著），
    // 于是求和后的众数被顶到网格端点（肾单核数据 ρ=80%，PBMC 4-plex ρ=1%）。
    //
    // 现实现：把"排除线"与"估计值"绑在一起迭代，直到自洽——
    //   1. 以各对 n/E 的中位数作稳健初值；
    //   2. 按 λ = excl_factor · ρ̂ · E 做 Poisson 上尾检验 + BH，排除显著偏高的对；
    //   3. 在保留的对上用共轭 Gamma 后验取众数重估 ρ̂；
    //   4. 重复直到 ρ̂ 稳定（或达到迭代上限）。
    // 这样"真实表达"的判定随 ρ̂ 收紧，不再依赖 ρ_max 这条与估计值无关的线。
    let shape = (params.prior_mean / params.prior_sd).powi(2);
    let rate = shape / params.prior_mean;
    let exclude = |rho: f64| -> (Vec<&Pair>, usize) {
        let pv: Vec<f64> = pairs
            .iter()
            .map(|pr| {
                let lambda = (params.excl_factor * rho * pr.e).max(1e-12);
                if pr.n <= 0.0 {
                    1.0
                } else {
                    match Poisson::new(lambda) {
                        Ok(d) => 1.0 - d.cdf(pr.n as u64 - 1),
                        Err(_) => 1.0,
                    }
                }
            })
            .collect();
        let adj = crate::qc::emptydrops::benjamini_hochberg(&pv);
        let mut keep = Vec::new();
        let mut dropped = 0usize;
        for (pr, &a) in pairs.iter().zip(&adj) {
            if a < params.rho_max_fdr {
                dropped += 1;
            } else {
                keep.push(pr);
            }
        }
        (keep, dropped)
    };
    let gamma_mode = |used: &[&Pair]| -> f64 {
        let sum_n: f64 = used.iter().map(|p| p.n).sum();
        let sum_e: f64 = used.iter().map(|p| p.e).sum();
        if sum_e <= 0.0 {
            return f64::NAN;
        }
        ((shape + sum_n - 1.0) / (rate + sum_e)).clamp(params.rho_min, params.rho_max)
    };
    let mut ratios: Vec<f64> = pairs
        .iter()
        .filter(|p| p.e > 0.0)
        .map(|p| p.n / p.e)
        .collect();
    ratios.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if ratios.is_empty() {
        return fallback("没有可用的基因-簇对");
    }
    let mut rho = ratios[ratios.len() / 2].clamp(params.rho_min, params.rho_max);
    let mut used: Vec<&Pair> = Vec::new();
    let mut n_excluded = 0usize;
    let mut n_iter = 0usize;
    for _ in 0..params.max_iter {
        n_iter += 1;
        let (keep, dropped) = exclude(rho);
        if keep.len() < 5 {
            break;
        }
        let next = gamma_mode(&keep);
        used = keep;
        n_excluded = dropped;
        if !next.is_finite() {
            break;
        }
        let delta = (next - rho).abs();
        rho = next;
        if delta < 1e-4 {
            break;
        }
    }
    if used.is_empty() {
        return fallback("所有候选基因对都被判为真实表达，无法估计污染度");
    }
    let rho_global = rho;
    // 共轭 Gamma 后验的 ±2 标准差区间（诊断用）
    let sum_n: f64 = used.iter().map(|p| p.n).sum();
    let sum_e: f64 = used.iter().map(|p| p.e).sum();
    let post_shape = shape + sum_n;
    let post_rate = rate + sum_e;
    let sd = post_shape.sqrt() / post_rate;
    let rho_low = (rho_global - 2.0 * sd).max(0.0);
    let rho_high = rho_global + 2.0 * sd;
    // 各簇单独重估（同一 ρ̂ 下的排除结果，簇内共轭众数）
    let rho_per_cluster: Vec<f64> = (0..k)
        .map(|kk| {
            let sub: Vec<&Pair> = used.iter().copied().filter(|p| p.cluster == kk).collect();
            if sub.is_empty() {
                rho_global
            } else {
                gamma_mode(&sub)
            }
        })
        .collect();
    // 估计值贴到搜索区间的边界，说明模型与数据不匹配，上层不应采信
    let at_boundary = rho_global <= params.rho_min * 1.001 || rho_global >= params.rho_max * 0.999;
    SoupResult {
        rho_global,
        at_boundary,
        rho_low,
        rho_high,
        rho_per_cluster,
        n_markers: markers.len(),
        n_pairs_used: used.len(),
        n_pairs_excluded: n_excluded,
        markers,
        note_zh: format!(
            "tf-idf 标记基因 {} 个（{} 个簇），迭代 {} 轮后使用基因-簇对 {} 个，排除真实表达对 {} 个{}",
            marker_genes.len(),
            k,
            n_iter,
            used.len(),
            n_excluded,
            if at_boundary {
                format!(
                    "；⚠️ 估计值贴在搜索区间端点（{:.0}%–{:.0}%），说明模型与数据不匹配，该估计不可信，已不参与一致性判定",
                    params.rho_min * 100.0,
                    params.rho_max * 100.0
                )
            } else {
                String::new()
            }
        ),
    }
}
