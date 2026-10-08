//! 估计器 B：DecontX（background 模式）变分 EM（SPEC §8.2.2；DESIGN §5.8）。
//!
//! 依据：Yang S et al. (2020) "Decontamination of ambient RNA in single-cell RNA-seq with
//! DecontX", Genome Biology 21:57；参考实现 celda（MIT），保留其版权声明的精神：
//! Copyright (c) celda authors, MIT License。
//!
//! 模型：细胞 j（属于簇 z_j）的计数向量 X_j ~ Multinomial(N_j, θ_j φ_{z_j} + (1 − θ_j) η)，
//!   θ_j ~ Beta(δ₁, δ₂) 为原生表达占比，φ_k 为簇 k 的原生表达分布，
//!   **background 模式下 η 固定为空液滴的经验分布**（【硬约束】SPEC §8.2.1）。
//!
//! 变分 EM（论文 Methods 的坐标上升更新）：
//! - E 步：每个计数属于原生的责任 r_ij = θ_j φ_{z_j,i} / (θ_j φ_{z_j,i} + (1 − θ_j) η_i)；
//! - M 步：θ_j = (Σ_i x_ij r_ij + δ₁ − 1) / (N_j + δ₁ + δ₂ − 2)（Beta 先验的 MAP）；
//!   φ_{k,i} ∝ Σ_{j∈k} x_ij r_ij + ε（Dirichlet 平滑，避免 0）；
//!   可选：由 θ 的矩估计更新 δ（celda `estimateDelta = TRUE` 的近似）；
//! - 收敛：对数似然 Σ_ij x_ij log(θ_j φ_{z_j,i} + (1 − θ_j) η_i) 的**绝对**变化 < tol
//!   （与 celda 的 `convergence` 口径一致）或达最大迭代。
//!
//! 可辨识性说明：当簇分布 φ 可自由估计时，似然在 ρ ≤ min_i(x_ki/η_i) 的区间内是平的，
//! 估计由 Beta 先验从 0.5 向该上界收缩，因此需要足够的迭代次数（不能过早停止）。
//!
//! 输出：每细胞污染度 1 − θ_j；`decontx_counts`（x_ij r_ij，非整数）按需物化。
//! 确定性：θ 更新逐细胞并行、φ 用固定分块部分和顺序合并（DESIGN §12）。

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::profile::AmbientProfile;
use crate::embed::normalize::chunk_bounds;
use crate::matrix::CountMatrix;
use crate::pipeline::cancel::CancelToken;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecontXParams {
    pub max_iter: usize,
    pub tol: f64,
    pub delta: (f64, f64),
    pub estimate_delta: bool,
    /// φ 的 Dirichlet 平滑伪计数（按基因）
    pub phi_pseudocount: f64,
    /// θ 初值 ~ Beta(δ) 的随机种子（与 celda 一致：随机初始化；本实现用派生种子保证可复现）
    pub seed: u64,
    /// true：平均场变分更新（E 步用 exp(ψ(a_j)) 与 exp(ψ(b_j)) 作权重，论文口径）；
    /// false：MAP-EM（E 步直接用 θ_j）
    pub variational: bool,
}

impl Default for DecontXParams {
    fn default() -> Self {
        Self {
            max_iter: 500,
            tol: 0.001,
            delta: (10.0, 10.0),
            estimate_delta: true,
            phi_pseudocount: 1e-3,
            seed: crate::rng::DEFAULT_SEED,
            variational: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecontXResult {
    pub rho_per_cell: Vec<f64>,
    pub rho_global: f64,
    pub n_iter: usize,
    pub converged: bool,
    pub log_likelihood: f64,
    pub delta: (f64, f64),
}

/// `cells`：细胞矩阵（raw counts）；`cluster`：粗簇标签；`profile.good_turing` 作为 η。
pub fn decontx_background(
    cells: &CountMatrix,
    cluster: &[u32],
    profile: &AmbientProfile,
    params: &DecontXParams,
    cancel: &CancelToken,
    on_iter: &dyn Fn(usize, f64),
) -> crate::Result<DecontXResult> {
    let g = cells.n_genes();
    let n = cells.n_barcodes();
    let k = cluster
        .iter()
        .copied()
        .max()
        .map(|m| m as usize + 1)
        .unwrap_or(1);
    let eta: Vec<f64> = profile.good_turing.clone();
    let totals: Vec<f64> = cells.column_sums().iter().map(|&t| t as f64).collect();

    // 初始化：θ_j ~ Beta(δ₁, δ₂)（全部相同的初值会使 δ 的矩估计退化并把 θ 锁死）；
    // φ_k = 簇内计数和归一化（加伪计数）
    let mut rng = crate::rng::derive_rng(params.seed, "decontx", 0);
    let mut theta: Vec<f64> = (0..n)
        .map(|_| sample_beta(&mut rng, params.delta.0, params.delta.1).clamp(1e-3, 1.0 - 1e-3))
        .collect();
    let mut prev_theta = theta.clone();
    let mut phi = cluster_profiles(cells, cluster, k, &[], None, params.phi_pseudocount);
    let (mut d1, mut d2) = params.delta;
    // 变分权重：每细胞 (w_native, w_contam)，初始用 θ 与 1−θ
    let mut weights: Vec<(f64, f64)> = theta.iter().map(|&t| (t, 1.0 - t)).collect();
    let mut prev_ll = f64::NEG_INFINITY;
    let mut converged = false;
    let mut n_iter = 0;

    for it in 1..=params.max_iter {
        n_iter = it;
        cancel.checkpoint()?;
        // E 步 + θ 的 M 步（逐细胞独立）；同时得到每细胞对数似然
        let (theta_new, ll_parts): (Vec<f64>, Vec<f64>) = (0..n)
            .into_par_iter()
            .map(|j| {
                let kk = cluster[j] as usize;
                let th = theta[j];
                let (wa, wb) = weights[j];
                let (idx, val) = cells.column(j);
                let mut native = 0.0f64;
                let mut ll = 0.0f64;
                for (&gi, &v) in idx.iter().zip(val) {
                    let gi = gi as usize;
                    let ph = phi[kk * g + gi];
                    let et = eta[gi];
                    let a = wa * ph;
                    let b = wb * et;
                    let s = a + b;
                    let x = v as f64;
                    if s > 0.0 {
                        native += x * a / s;
                    }
                    let mix = th * ph + (1.0 - th) * et;
                    if mix > 0.0 {
                        ll += x * mix.ln();
                    }
                }
                let nj = totals[j];
                let t = if params.variational {
                    // q(θ_j) = Beta(native + δ₁, contam + δ₂)，θ_j 取后验均值
                    (native + d1) / (nj + d1 + d2)
                } else {
                    (native + d1 - 1.0) / (nj + d1 + d2 - 2.0)
                }
                .clamp(1e-6, 1.0 - 1e-6);
                (t, ll)
            })
            .unzip();
        theta = theta_new;
        // 更新变分权重
        weights = (0..n)
            .map(|j| {
                if params.variational {
                    let nj = totals[j];
                    let a = theta[j] * (nj + d1 + d2); // = native + δ₁
                    let b = (nj + d1 + d2) - a; // = contam + δ₂
                    let (a, b) = (a.max(1e-6), b.max(1e-6));
                    let la = statrs::function::gamma::digamma(a)
                        - statrs::function::gamma::digamma(a + b);
                    let lb = statrs::function::gamma::digamma(b)
                        - statrs::function::gamma::digamma(a + b);
                    (la.exp(), lb.exp())
                } else {
                    (theta[j], 1.0 - theta[j])
                }
            })
            .collect();
        let ll: f64 = ll_parts.iter().sum();

        // φ 的 M 步：Σ_{j∈k} x_ij r_ij（用更新后的 θ 再算一次责任，与论文坐标上升一致）
        let eff_theta: Vec<f64> = weights.iter().map(|&(a, b)| a / (a + b)).collect();
        phi = cluster_profiles(
            cells,
            cluster,
            k,
            &eff_theta,
            Some((phi.as_slice(), eta.as_slice())),
            params.phi_pseudocount,
        );

        // δ 的矩估计（先验总强度限制在 [2, 1000]，避免先验过强锁死 θ）
        if params.estimate_delta && n > 1 {
            let m = theta.iter().sum::<f64>() / n as f64;
            let v = theta.iter().map(|t| (t - m).powi(2)).sum::<f64>() / (n as f64 - 1.0);
            if v > 1e-12 && m > 0.0 && m < 1.0 {
                let common = (m * (1.0 - m) / v - 1.0).clamp(2.0, 1000.0);
                d1 = (m * common).max(1.0);
                d2 = ((1.0 - m) * common).max(1.0);
            }
        }

        on_iter(it, ll);
        let d_ll = (ll - prev_ll).abs();
        let d_rho = theta
            .iter()
            .zip(&prev_theta)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f64, f64::max);
        prev_theta.clone_from(&theta);
        if it > 1 && (d_ll < params.tol || d_rho < 1e-7) {
            converged = true;
            prev_ll = ll;
            break;
        }
        prev_ll = ll;
    }

    let rho: Vec<f64> = theta.iter().map(|t| 1.0 - t).collect();
    let rho_global = rho.iter().sum::<f64>() / n.max(1) as f64;
    Ok(DecontXResult {
        rho_per_cell: rho,
        rho_global,
        n_iter,
        converged,
        log_likelihood: prev_ll,
        delta: (d1, d2),
    })
}

/// Beta(a, b) 采样：两个 Gamma 变量之比（Marsaglia & Tsang 2000 的 Gamma 采样器）。
fn sample_beta(rng: &mut rand_chacha::ChaCha8Rng, a: f64, b: f64) -> f64 {
    let x = sample_gamma(rng, a);
    let y = sample_gamma(rng, b);
    if x + y > 0.0 {
        x / (x + y)
    } else {
        0.5
    }
}

fn sample_gamma(rng: &mut rand_chacha::ChaCha8Rng, shape: f64) -> f64 {
    use rand::Rng;
    if shape < 1.0 {
        // Gamma(a) = Gamma(a+1) · U^{1/a}
        let u: f64 = rng.gen::<f64>().max(1e-300);
        return sample_gamma(rng, shape + 1.0) * u.powf(1.0 / shape);
    }
    let d = shape - 1.0 / 3.0;
    let c = 1.0 / (9.0 * d).sqrt();
    loop {
        // 标准正态（Box–Muller）
        let u1: f64 = rng.gen::<f64>().max(1e-300);
        let u2: f64 = rng.gen();
        let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
        let v = (1.0 + c * z).powi(3);
        if v <= 0.0 {
            continue;
        }
        let u: f64 = rng.gen::<f64>().max(1e-300);
        if u.ln() < 0.5 * z * z + d - d * v + d * v.ln() {
            return d * v;
        }
    }
}

/// 簇原生分布 φ（k × g，行主序）。`theta` 为空时用原始计数（初始化）；
/// 否则用责任 r_ij（需要当前 φ 与 η）。
fn cluster_profiles(
    cells: &CountMatrix,
    cluster: &[u32],
    k: usize,
    theta: &[f64],
    current: Option<(&[f64], &[f64])>,
    pseudocount: f64,
) -> Vec<f64> {
    let g = cells.n_genes();
    let n = cells.n_barcodes();
    let partials: Vec<Vec<f64>> = chunk_bounds(n)
        .par_iter()
        .map(|&(a, b)| {
            let mut acc = vec![0.0f64; k * g];
            for j in a..b {
                let kk = cluster[j] as usize;
                let (idx, val) = cells.column(j);
                match current {
                    None => {
                        for (&gi, &v) in idx.iter().zip(val) {
                            acc[kk * g + gi as usize] += v as f64;
                        }
                    }
                    Some((phi, eta)) => {
                        let th = theta[j];
                        for (&gi, &v) in idx.iter().zip(val) {
                            let gi = gi as usize;
                            let na = th * phi[kk * g + gi];
                            let s = na + (1.0 - th) * eta[gi];
                            if s > 0.0 {
                                acc[kk * g + gi] += v as f64 * na / s;
                            }
                        }
                    }
                }
            }
            acc
        })
        .collect();
    let mut phi = vec![0.0f64; k * g];
    for p in &partials {
        for (o, v) in phi.iter_mut().zip(p) {
            *o += v;
        }
    }
    for kk in 0..k {
        let row = &mut phi[kk * g..(kk + 1) * g];
        let mut s = 0.0;
        for v in row.iter_mut() {
            *v += pseudocount;
            s += *v;
        }
        if s > 0.0 {
            for v in row.iter_mut() {
                *v /= s;
            }
        }
    }
    phi
}

/// 物化 DecontX 校正计数 x_ij r_ij（非整数），返回与输入同结构的三元组（行主序 (gene, cell, value)）。
pub fn decontx_counts(
    cells: &CountMatrix,
    cluster: &[u32],
    profile: &AmbientProfile,
    theta: &[f64],
    phi_pseudocount: f64,
) -> Vec<(u32, u32, f32)> {
    let g = cells.n_genes();
    let k = cluster
        .iter()
        .copied()
        .max()
        .map(|m| m as usize + 1)
        .unwrap_or(1);
    // 用最终 θ 重算 φ（一次），再算责任
    let phi0 = cluster_profiles(cells, cluster, k, &[], None, phi_pseudocount);
    let phi = cluster_profiles(
        cells,
        cluster,
        k,
        theta,
        Some((phi0.as_slice(), profile.good_turing.as_slice())),
        phi_pseudocount,
    );
    let mut out = Vec::with_capacity(cells.nnz());
    for j in 0..cells.n_barcodes() {
        let kk = cluster[j] as usize;
        let th = theta[j];
        let (idx, val) = cells.column(j);
        for (&gi, &v) in idx.iter().zip(val) {
            let na = th * phi[kk * g + gi as usize];
            let s = na + (1.0 - th) * profile.good_turing[gi as usize];
            let r = if s > 0.0 { na / s } else { 1.0 };
            out.push((gi, j as u32, (v as f64 * r) as f32));
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::matrix::Triplet;
    use rand::Rng;

    /// 合成：两个细胞群各有专属基因；环境谱均匀；细胞计数 = 80% 原生 + 20% 环境。
    /// DecontX 应估计出接近 0.2 的污染度。
    #[test]
    fn recovers_planted_contamination() {
        let g = 60;
        let mut rng = crate::rng::derive_rng(5, "dx", 0);
        let mut trip = Vec::new();
        let n_per = 150;
        let mut cluster = Vec::new();
        for c in 0..2u32 {
            for j in 0..n_per {
                let col = (c as usize * n_per + j) as u32;
                cluster.push(c);
                let mut counts = vec![0u32; g];
                for _ in 0..800 {
                    let ambient = rng.gen::<f64>() < 0.2;
                    let gi = if ambient {
                        rng.gen_range(0..g)
                    } else if c == 0 {
                        rng.gen_range(0..20)
                    } else {
                        rng.gen_range(20..40)
                    };
                    counts[gi] += 1;
                }
                for (gi, &v) in counts.iter().enumerate() {
                    if v > 0 {
                        trip.push(Triplet {
                            row: gi as u32,
                            col,
                            value: v,
                        });
                    }
                }
            }
        }
        let cells = CountMatrix::from_triplets(g, 2 * n_per, trip).unwrap();
        let profile = AmbientProfile {
            umi_upper: 100,
            n_droplets: 1000,
            total_umi: 6000,
            counts: vec![100; g],
            fraction: vec![1.0 / g as f64; g],
            good_turing: vec![1.0 / g as f64; g],
        };
        let r = decontx_background(
            &cells,
            &cluster,
            &profile,
            &DecontXParams::default(),
            &CancelToken::new(),
            &|_, _| {},
        )
        .unwrap();
        assert!(r.converged, "未收敛（{} 次）", r.n_iter);
        assert!(
            (r.rho_global - 0.2).abs() < 0.06,
            "rho_global={}",
            r.rho_global
        );
        let dc = decontx_counts(
            &cells,
            &cluster,
            &profile,
            &r.rho_per_cell.iter().map(|x| 1.0 - x).collect::<Vec<_>>(),
            1e-3,
        );
        assert_eq!(dc.len(), cells.nnz());
    }
}
