//! Simple Good–Turing 频率估计。
//!
//! 依据：Gale WA & Sampson G (1995) "Good-Turing frequency estimation without tears",
//! *Journal of Quantitative Linguistics* 2(3):217–237。
//!
//! 用途（SPEC §8.1.2 / Lun 2019 §Methods）：从空液滴汇总的基因计数向量估计环境表达谱，
//! 使**计数为 0 的基因也获得非零概率**，否则 Dirichlet-multinomial 似然在这些基因上为 -∞。
//!
//! 算法概要（论文 §4–§6）：
//! 1. 统计"频率的频率" N_r（计数恰为 r 的基因数）；
//! 2. 对 N_r 做 Z_r = N_r / (0.5 (t − q)) 平滑（q、t 为相邻的非零 r），
//!    并在 log–log 上做线性回归 log Z_r = a + b·log r（要求 b < −1）；
//! 3. 每个 r 的调整计数 r*：Turing 估计 (r+1) N_{r+1}/N_r 与 LGT 估计 r (1 + 1/r)^{b+1}，
//!    当两者差异不再显著（1.96 倍标准差）时切换到 LGT 并保持；
//! 4. 未见事件总概率 P0 = N_1 / N；已见 r 的概率 p_r = (1 − P0) r* / Σ N_r r*；
//!    未见事件平分 P0。

/// 输入：每个基因的计数（可含 0）。输出：与输入等长的概率向量，和为 1。
///
/// 退化情况：总计数为 0 → 均匀分布；无 N_1（没有计数为 1 的基因）→ P0 = 0，
/// 零计数基因得到极小伪概率（1e-12 量级后归一化），避免 -∞。
pub fn good_turing_proportions(counts: &[u64]) -> Vec<f64> {
    let g = counts.len();
    if g == 0 {
        return Vec::new();
    }
    let n_total: u64 = counts.iter().sum();
    if n_total == 0 {
        return vec![1.0 / g as f64; g];
    }

    // 1. 频率的频率（按 r 升序）
    let mut freq: std::collections::BTreeMap<u64, u64> = std::collections::BTreeMap::new();
    for &c in counts {
        if c > 0 {
            *freq.entry(c).or_insert(0) += 1;
        }
    }
    let rs: Vec<f64> = freq.keys().map(|&r| r as f64).collect();
    let nrs: Vec<f64> = freq.values().map(|&n| n as f64).collect();
    let m = rs.len();
    let n_zero = counts.iter().filter(|&&c| c == 0).count();

    // 2. Z_r 平滑与 log–log 回归
    let mut zr = vec![0.0; m];
    #[allow(clippy::needless_range_loop)]
    for j in 0..m {
        let q = if j == 0 { 0.0 } else { rs[j - 1] };
        let t = if j == m - 1 {
            2.0 * rs[j] - q
        } else {
            rs[j + 1]
        };
        zr[j] = nrs[j] / (0.5 * (t - q));
    }
    let (a, b) = linear_regression(
        &rs.iter().map(|r| r.ln()).collect::<Vec<_>>(),
        &zr.iter().map(|z| z.ln()).collect::<Vec<_>>(),
    );
    // 论文要求 b < -1；不满足时退化为 b = -1（LGT 估计退化为 r 本身）
    let b = if b < -1.0 { b } else { -1.0 };

    // 3. 调整计数 r*
    let n1 = freq.get(&1).copied().unwrap_or(0) as f64;
    let mut r_star = vec![0.0; m];
    let mut use_lgt = false;
    #[allow(clippy::needless_range_loop)]
    for j in 0..m {
        let r = rs[j];
        let lgt = r * (1.0 + 1.0 / r).powf(b + 1.0);
        let next_nr = if j + 1 < m && rs[j + 1] == r + 1.0 {
            nrs[j + 1]
        } else {
            0.0
        };
        if !use_lgt {
            if next_nr == 0.0 {
                use_lgt = true;
            } else {
                let turing = (r + 1.0) * next_nr / nrs[j];
                let var = (r + 1.0).powi(2) * next_nr / nrs[j].powi(2) * (1.0 + next_nr / nrs[j]);
                if (turing - lgt).abs() <= 1.96 * var.sqrt() {
                    use_lgt = true;
                } else {
                    r_star[j] = turing;
                    continue;
                }
            }
        }
        r_star[j] = lgt;
    }
    let _ = a;

    // 4. 概率
    let p0 = if n_zero > 0 { n1 / n_total as f64 } else { 0.0 };
    let norm: f64 = nrs.iter().zip(&r_star).map(|(n, s)| n * s).sum();
    let mut p_by_r: std::collections::HashMap<u64, f64> = std::collections::HashMap::new();
    for (j, (&r, _)) in freq.iter().enumerate() {
        p_by_r.insert(r, (1.0 - p0) * r_star[j] / norm);
    }
    let p_unseen = if n_zero > 0 {
        if p0 > 0.0 {
            p0 / n_zero as f64
        } else {
            1e-12
        }
    } else {
        0.0
    };
    let mut out: Vec<f64> = counts
        .iter()
        .map(|&c| {
            if c == 0 {
                p_unseen
            } else {
                p_by_r.get(&c).copied().unwrap_or(0.0)
            }
        })
        .collect();
    let s: f64 = out.iter().sum();
    if s > 0.0 {
        for p in &mut out {
            *p /= s;
        }
    }
    out
}

fn linear_regression(x: &[f64], y: &[f64]) -> (f64, f64) {
    let n = x.len() as f64;
    if x.len() < 2 {
        return (0.0, -1.0);
    }
    let mx = x.iter().sum::<f64>() / n;
    let my = y.iter().sum::<f64>() / n;
    let sxx: f64 = x.iter().map(|v| (v - mx).powi(2)).sum();
    let sxy: f64 = x.iter().zip(y).map(|(u, v)| (u - mx) * (v - my)).sum();
    if sxx == 0.0 {
        return (my, -1.0);
    }
    let b = sxy / sxx;
    (my - b * mx, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_to_one_and_zero_counts_get_mass() {
        // 典型稀疏计数：大量 0、很多 1、少量大值
        let mut counts = vec![0u64; 1000];
        counts[..300].fill(1);
        counts[300..400].fill(2);
        counts[400..450].fill(3);
        counts[450] = 50;
        counts[451] = 120;
        let p = good_turing_proportions(&counts);
        let s: f64 = p.iter().sum();
        assert!((s - 1.0).abs() < 1e-9);
        assert!(p[999] > 0.0, "零计数基因必须有非零概率");
        assert!(p[451] > p[450] && p[450] > p[0] && p[0] > p[999]);
        // 未见事件总概率 ≈ N1/N
        let n: u64 = counts.iter().sum();
        let p0: f64 = p[452..].iter().sum();
        assert!((p0 - 300.0 / n as f64).abs() < 0.02);
    }

    #[test]
    fn degenerate_inputs() {
        assert!(good_turing_proportions(&[]).is_empty());
        let p = good_turing_proportions(&[0, 0, 0]);
        assert!((p[0] - 1.0 / 3.0).abs() < 1e-12);
        let p = good_turing_proportions(&[5, 0]);
        assert!(p[1] > 0.0 && p[0] > p[1]);
    }
}
