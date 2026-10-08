//! PCA：随机化 SVD（SPEC §8.6）。
//!
//! 依据：Halko N, Martinsson PG, Tropp JA (2011) "Finding structure with randomness",
//! *SIAM Review* 53(2):217–288，Algorithm 4.4（随机化子空间迭代）+ Algorithm 5.1。
//!
//! 输入为已中心化/缩放的稠密矩阵 A（n 细胞 × h 基因，行主序 f32）。步骤：
//! 1. Ω ~ N(0,1)^{h×l}，l = k + 过采样；Y = A Ω；
//! 2. 幂迭代 q 次：Y ← A (Aᵀ Y)，每步做一次 Gram–Schmidt 正交化以保持数值稳定；
//! 3. Q = orth(Y)（n×l）；B = Qᵀ A（l×h）；
//! 4. 小矩阵 B 的 SVD 通过 B Bᵀ（l×l）的 Jacobi 特征分解得到：B = U_B S V_Bᵀ；
//! 5. 主成分得分 = Q U_B S（前 k 列），载荷 V = Bᵀ U_B / S。
//!
//! 全部线代为本模块自含实现（矩阵很小或运算是简单的并行乘加），
//! 大矩阵乘法按固定分块并行、按固定顺序合并，结果与线程数无关（DESIGN §12）。

use rand::Rng;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::normalize::chunk_bounds;
use crate::rng::derive_rng;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcaParams {
    pub n_components: usize,
    pub oversample: usize,
    pub power_iters: usize,
    pub seed: u64,
}

impl Default for PcaParams {
    fn default() -> Self {
        Self {
            n_components: 50,
            oversample: 10,
            power_iters: 2,
            seed: crate::rng::DEFAULT_SEED,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PcaResult {
    pub n: usize,
    pub k: usize,
    /// n × k 行主序得分
    pub scores: Vec<f32>,
    /// 奇异值（降序）
    pub singular_values: Vec<f64>,
    /// 各成分解释的方差比例（相对于 A 的总方差）
    pub explained_variance_ratio: Vec<f64>,
}

/// 计算 A（n×h，行主序）的前 k 个主成分得分。
pub fn randomized_pca(a: &[f32], n: usize, h: usize, params: &PcaParams) -> PcaResult {
    let k = params.n_components.min(h).min(n).max(1);
    let l = (k + params.oversample).min(h).min(n).max(k);

    // 1. Ω（h×l，行主序），Box–Muller 标准正态
    let mut rng = derive_rng(params.seed, "pca", 0);
    let mut omega = vec![0.0f32; h * l];
    for v in omega.iter_mut() {
        let u1: f64 = rng.gen::<f64>().max(1e-300);
        let u2: f64 = rng.gen();
        *v = ((-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()) as f32;
    }
    // Y = A Ω（n×l）
    let mut y = matmul_a_b(a, n, h, &omega, l);
    orthonormalize(&mut y, n, l);

    // 2. 幂迭代
    for _ in 0..params.power_iters {
        let z = matmul_at_b(a, n, h, &y, l); // h×l
        let mut z = z;
        orthonormalize(&mut z, h, l);
        y = matmul_a_b(a, n, h, &z, l);
        orthonormalize(&mut y, n, l);
    }
    let q = y; // n×l，正交

    // 3. B = Qᵀ A（l×h）
    let b = matmul_at_b(&q, n, l, a, h); // 结果 l×h（由 (Qᵀ A) 得到，实现见函数说明）

    // 4. C = B Bᵀ（l×l）→ Jacobi
    let mut c = vec![0.0f64; l * l];
    for i in 0..l {
        for j in i..l {
            let mut s = 0.0f64;
            let bi = &b[i * h..(i + 1) * h];
            let bj = &b[j * h..(j + 1) * h];
            for (x, y) in bi.iter().zip(bj) {
                s += (*x as f64) * (*y as f64);
            }
            c[i * l + j] = s;
            c[j * l + i] = s;
        }
    }
    let (eigvals, eigvecs) = jacobi_eigen_sym(&c, l); // 降序；eigvecs 列 = 特征向量（l×l 行主序）

    // 5. 得分 = Q U_B S = Q (U_B S)；U_B 列 i 乘 s_i。等价于 Q W 其中 W[:, i] = u_i * s_i。
    // 由 B = U S Vᵀ 且 C = B Bᵀ = U S² Uᵀ，U = eigvecs，S = sqrt(eigvals)。
    let singular: Vec<f64> = eigvals.iter().map(|v| v.max(0.0).sqrt()).collect();
    // 符号约定：使每个成分在 A 上的载荷最大绝对值分量为正 —— 通过 B 的行组合 Vᵀ = S⁻¹ Uᵀ B 判断。
    let mut signs = vec![1.0f32; k];
    for comp in 0..k {
        if singular[comp] <= 0.0 {
            continue;
        }
        let mut best = 0.0f64;
        let mut best_sign = 1.0;
        for g in 0..h {
            let mut v = 0.0f64;
            for i in 0..l {
                v += eigvecs[i * l + comp] * b[i * h + g] as f64;
            }
            if v.abs() > best {
                best = v.abs();
                best_sign = if v >= 0.0 { 1.0 } else { -1.0 };
            }
        }
        signs[comp] = best_sign as f32;
    }
    let mut w = vec![0.0f32; l * k];
    for i in 0..l {
        for comp in 0..k {
            w[i * k + comp] = (eigvecs[i * l + comp] * singular[comp]) as f32 * signs[comp];
        }
    }
    let scores = matmul_a_b(&q, n, l, &w, k);

    // 解释方差比例：总方差 = ‖A‖_F² / (n−1)
    let total_ss: f64 = chunk_bounds(n)
        .par_iter()
        .map(|&(s, e)| {
            a[s * h..e * h]
                .iter()
                .map(|v| (*v as f64) * (*v as f64))
                .sum::<f64>()
        })
        .collect::<Vec<f64>>()
        .iter()
        .sum();
    let explained: Vec<f64> = singular
        .iter()
        .take(k)
        .map(|s| {
            if total_ss > 0.0 {
                s * s / total_ss
            } else {
                0.0
            }
        })
        .collect();

    PcaResult {
        n,
        k,
        scores,
        singular_values: singular.into_iter().take(k).collect(),
        explained_variance_ratio: explained,
    }
}

/// A（n×h）· B（h×l）→ n×l。按行并行；每行独立，无需归约。
fn matmul_a_b(a: &[f32], n: usize, h: usize, b: &[f32], l: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; n * l];
    out.par_chunks_mut(l).enumerate().for_each(|(i, row)| {
        let ai = &a[i * h..(i + 1) * h];
        for (g, &aig) in ai.iter().enumerate() {
            if aig == 0.0 {
                continue;
            }
            let bg = &b[g * l..(g + 1) * l];
            for (o, &bgl) in row.iter_mut().zip(bg) {
                *o += aig * bgl;
            }
        }
    });
    out
}

/// Aᵀ（h×n）· B（n×l）→ h×l。A 为 n×h 行主序。
/// 按固定分块并行得到部分和，再按块顺序累加（确定性）。
fn matmul_at_b(a: &[f32], n: usize, h: usize, b: &[f32], l: usize) -> Vec<f32> {
    let partials: Vec<Vec<f32>> = chunk_bounds(n)
        .par_iter()
        .map(|&(s, e)| {
            let mut acc = vec![0.0f32; h * l];
            for i in s..e {
                let ai = &a[i * h..(i + 1) * h];
                let bi = &b[i * l..(i + 1) * l];
                for (g, &aig) in ai.iter().enumerate() {
                    if aig == 0.0 {
                        continue;
                    }
                    let row = &mut acc[g * l..(g + 1) * l];
                    for (o, &bil) in row.iter_mut().zip(bi) {
                        *o += aig * bil;
                    }
                }
            }
            acc
        })
        .collect();
    let mut out = vec![0.0f32; h * l];
    for p in &partials {
        for (o, v) in out.iter_mut().zip(p) {
            *o += v;
        }
    }
    out
}

/// 对 n×l 行主序矩阵的列做两遍修正 Gram–Schmidt 正交归一化（原地）。
fn orthonormalize(y: &mut [f32], n: usize, l: usize) {
    for _pass in 0..2 {
        for j in 0..l {
            for i in 0..j {
                // y_j -= (y_i · y_j) y_i
                let mut dot = 0.0f64;
                for r in 0..n {
                    dot += y[r * l + i] as f64 * y[r * l + j] as f64;
                }
                let d = dot as f32;
                if d != 0.0 {
                    for r in 0..n {
                        y[r * l + j] -= d * y[r * l + i];
                    }
                }
            }
            let mut norm = 0.0f64;
            for r in 0..n {
                norm += (y[r * l + j] as f64).powi(2);
            }
            let norm = norm.sqrt() as f32;
            if norm > 1e-12 {
                for r in 0..n {
                    y[r * l + j] /= norm;
                }
            } else {
                for r in 0..n {
                    y[r * l + j] = 0.0;
                }
            }
        }
    }
}

/// 对称矩阵（m×m，行主序 f64）的循环 Jacobi 特征分解。
/// 返回（特征值降序，特征向量矩阵 m×m 行主序，第 c 列对应第 c 个特征值）。
pub fn jacobi_eigen_sym(c: &[f64], m: usize) -> (Vec<f64>, Vec<f64>) {
    let mut a = c.to_vec();
    let mut v = vec![0.0f64; m * m];
    for i in 0..m {
        v[i * m + i] = 1.0;
    }
    for _sweep in 0..100 {
        let mut off = 0.0;
        for i in 0..m {
            for j in i + 1..m {
                off += a[i * m + j] * a[i * m + j];
            }
        }
        if off < 1e-22 {
            break;
        }
        for p in 0..m {
            for q in p + 1..m {
                let apq = a[p * m + q];
                if apq.abs() < 1e-300 {
                    continue;
                }
                let app = a[p * m + p];
                let aqq = a[q * m + q];
                let theta = (aqq - app) / (2.0 * apq);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let cs = 1.0 / (t * t + 1.0).sqrt();
                let sn = t * cs;
                for k in 0..m {
                    let akp = a[k * m + p];
                    let akq = a[k * m + q];
                    a[k * m + p] = cs * akp - sn * akq;
                    a[k * m + q] = sn * akp + cs * akq;
                }
                for k in 0..m {
                    let apk = a[p * m + k];
                    let aqk = a[q * m + k];
                    a[p * m + k] = cs * apk - sn * aqk;
                    a[q * m + k] = sn * apk + cs * aqk;
                }
                for k in 0..m {
                    let vkp = v[k * m + p];
                    let vkq = v[k * m + q];
                    v[k * m + p] = cs * vkp - sn * vkq;
                    v[k * m + q] = sn * vkp + cs * vkq;
                }
            }
        }
    }
    let mut order: Vec<usize> = (0..m).collect();
    order.sort_by(|&x, &y| {
        a[y * m + y]
            .partial_cmp(&a[x * m + x])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let vals: Vec<f64> = order.iter().map(|&i| a[i * m + i]).collect();
    let mut vecs = vec![0.0f64; m * m];
    for (new_c, &old_c) in order.iter().enumerate() {
        for r in 0..m {
            vecs[r * m + new_c] = v[r * m + old_c];
        }
    }
    (vals, vecs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jacobi_recovers_known_eigenvalues() {
        // [[2,1],[1,2]] → 特征值 3, 1
        let (vals, vecs) = jacobi_eigen_sym(&[2.0, 1.0, 1.0, 2.0], 2);
        assert!((vals[0] - 3.0).abs() < 1e-10 && (vals[1] - 1.0).abs() < 1e-10);
        // 第一特征向量 ∝ (1,1)/√2
        assert!((vecs[0].abs() - vecs[2].abs()).abs() < 1e-10);
    }

    #[test]
    fn pca_finds_dominant_direction() {
        // 200 个点，主要沿 (1,1,0,…) 方向变化，加少量噪声
        let n = 200;
        let h = 6;
        let mut rng = derive_rng(3, "test", 0);
        let mut a = vec![0.0f32; n * h];
        for i in 0..n {
            let t: f32 = rng.gen::<f32>() * 10.0 - 5.0;
            for g in 0..h {
                let noise: f32 = rng.gen::<f32>() * 0.2 - 0.1;
                a[i * h + g] = (if g < 2 { t } else { 0.0 }) + noise;
            }
        }
        // 中心化
        for g in 0..h {
            let mean: f32 = (0..n).map(|i| a[i * h + g]).sum::<f32>() / n as f32;
            for i in 0..n {
                a[i * h + g] -= mean;
            }
        }
        let p = randomized_pca(
            &a,
            n,
            h,
            &PcaParams {
                n_components: 2,
                oversample: 2,
                power_iters: 2,
                seed: 1,
            },
        );
        assert_eq!(p.k, 2);
        assert!(
            p.explained_variance_ratio[0] > 0.95,
            "{:?}",
            p.explained_variance_ratio
        );
        // 得分与 t 强相关：得分方差 ≈ 2 · var(t)
        let s0: Vec<f32> = (0..n).map(|i| p.scores[i * 2]).collect();
        let var = s0.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / (n as f64 - 1.0);
        assert!(var > 10.0, "var={var}");
        // 可复现
        let p2 = randomized_pca(
            &a,
            n,
            h,
            &PcaParams {
                n_components: 2,
                oversample: 2,
                power_iters: 2,
                seed: 1,
            },
        );
        assert_eq!(p.scores, p2.scores);
    }
}
