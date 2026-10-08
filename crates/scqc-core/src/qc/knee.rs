//! Barcode-rank 曲线的 knee / inflection（SPEC §8.1.1；DESIGN §5.3）。
//!
//! 依据：Lun et al. 2019 对 barcode-rank 曲线的描述，以及 DropletUtils `barcodeRanks`
//! **文档**中的定义（未阅读其源码）：
//! - inflection：log(total) 对 log(rank) 的一阶导数最小（下降最陡）处；
//! - knee：曲线的有符号曲率最小处。
//!
//! 曲线在 `[lower, max]` 区段上用局部二次回归平滑后求导。
//!
//! 输出同时包含绘图用的 (rank, total) 点列（按唯一 total 折叠）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KneeResult {
    /// 唯一 total 的 (平均秩, total)，按秩升序 —— knee plot 数据
    pub curve_rank: Vec<f64>,
    pub curve_total: Vec<u64>,
    pub knee_total: u64,
    pub knee_rank: f64,
    pub inflection_total: u64,
    pub inflection_rank: f64,
    /// 参与拟合的 total 下界
    pub lower: u64,
    pub n_barcodes_nonzero: usize,
}

/// `totals`：每个 barcode 的总 UMI（0 会被忽略）。`lower`：拟合区段下界。
pub fn barcode_ranks(totals: &[u64], lower: u64) -> KneeResult {
    let mut sorted: Vec<u64> = totals.iter().copied().filter(|&t| t > 0).collect();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    let n = sorted.len();

    // 唯一 total → 平均秩（并列取平均，秩从 1 开始）
    let mut curve_rank = Vec::new();
    let mut curve_total = Vec::new();
    let mut i = 0;
    while i < n {
        let t = sorted[i];
        let mut j = i;
        while j < n && sorted[j] == t {
            j += 1;
        }
        // 秩 i+1 .. j（含），平均 = (i+1+j)/2
        curve_rank.push((i + 1 + j) as f64 / 2.0);
        curve_total.push(t);
        i = j;
    }

    let fallback = KneeResult {
        knee_total: curve_total.first().copied().unwrap_or(0),
        knee_rank: curve_rank.first().copied().unwrap_or(0.0),
        inflection_total: curve_total.first().copied().unwrap_or(0),
        inflection_rank: curve_rank.first().copied().unwrap_or(0.0),
        curve_rank: curve_rank.clone(),
        curve_total: curve_total.clone(),
        lower,
        n_barcodes_nonzero: n,
    };

    // 拟合区段：total >= lower
    let idx: Vec<usize> = (0..curve_total.len())
        .filter(|&k| curve_total[k] >= lower)
        .collect();
    if idx.len() < 10 {
        return fallback;
    }
    let x: Vec<f64> = idx.iter().map(|&k| curve_rank[k].log10()).collect();
    let y: Vec<f64> = idx
        .iter()
        .map(|&k| (curve_total[k] as f64).log10())
        .collect();

    // 局部二次回归求一阶、二阶导数；窗口按 x 跨度的 1/40（至少 7 点）
    let span = (x[x.len() - 1] - x[0]).abs();
    let h = (span / 40.0).max(1e-6);
    let (d1, d2): (Vec<f64>, Vec<f64>) =
        (0..x.len()).map(|k| local_quadratic(&x, &y, k, h)).unzip();
    // 有符号曲率 κ = y'' / (1 + y'^2)^{3/2}
    let curvature: Vec<f64> = d1
        .iter()
        .zip(&d2)
        .map(|(a, b)| b / (1.0 + a * a).powf(1.5))
        .collect();

    // 排除两端各 5% 的点，避免边界效应
    let margin = (x.len() / 20).max(1);
    let range = margin..x.len().saturating_sub(margin);
    if range.is_empty() {
        return fallback;
    }
    let argmin = |v: &[f64]| -> usize {
        let mut best = range.start;
        for k in range.clone() {
            if v[k] < v[best] {
                best = k;
            }
        }
        best
    };
    let k_knee = argmin(&curvature);
    let k_infl = argmin(&d1);

    KneeResult {
        knee_total: curve_total[idx[k_knee]],
        knee_rank: curve_rank[idx[k_knee]],
        inflection_total: curve_total[idx[k_infl]],
        inflection_rank: curve_rank[idx[k_infl]],
        curve_rank,
        curve_total,
        lower,
        n_barcodes_nonzero: n,
    }
}

/// 在 x[k] 附近（|x - x[k]| <= h，至少 7 点）拟合 y = c0 + c1 (x-x0) + c2 (x-x0)^2，
/// 返回一阶导 c1 与二阶导 2 c2。
#[allow(clippy::needless_range_loop)]
fn local_quadratic(x: &[f64], y: &[f64], k: usize, h: f64) -> (f64, f64) {
    let x0 = x[k];
    let mut lo = k;
    let mut hi = k;
    while lo > 0 && x0 - x[lo - 1] <= h {
        lo -= 1;
    }
    while hi + 1 < x.len() && x[hi + 1] - x0 <= h {
        hi += 1;
    }
    while hi - lo + 1 < 7 {
        lo = lo.saturating_sub(1);
        if hi + 1 < x.len() {
            hi += 1;
        }
        if lo == 0 && hi + 1 >= x.len() {
            break;
        }
    }
    // 正规方程（3×3），用简单的高斯消元
    let mut m = [[0.0f64; 3]; 3];
    let mut v = [0.0f64; 3];
    for j in lo..=hi {
        let dx = x[j] - x0;
        let basis = [1.0, dx, dx * dx];
        for r in 0..3 {
            v[r] += basis[r] * y[j];
            for c in 0..3 {
                m[r][c] += basis[r] * basis[c];
            }
        }
    }
    match solve3(m, v) {
        Some(c) => (c[1], 2.0 * c[2]),
        None => (0.0, 0.0),
    }
}

#[allow(clippy::needless_range_loop)]
fn solve3(mut m: [[f64; 3]; 3], mut v: [f64; 3]) -> Option<[f64; 3]> {
    for col in 0..3 {
        let mut piv = col;
        for r in col + 1..3 {
            if m[r][col].abs() > m[piv][col].abs() {
                piv = r;
            }
        }
        if m[piv][col].abs() < 1e-12 {
            return None;
        }
        m.swap(col, piv);
        v.swap(col, piv);
        for r in col + 1..3 {
            let f = m[r][col] / m[col][col];
            for c in col..3 {
                m[r][c] -= f * m[col][c];
            }
            v[r] -= f * v[col];
        }
    }
    let mut out = [0.0; 3];
    for r in (0..3).rev() {
        let mut s = v[r];
        for c in r + 1..3 {
            s -= m[r][c] * out[c];
        }
        out[r] = s / m[r][r];
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_two_population_curve() {
        // 2000 个"细胞"约 3000 UMI，50000 个"空液滴"约 20 UMI → knee 应落在 ~ 2000 秩附近
        let mut totals = Vec::new();
        for i in 0..2000u64 {
            totals.push(2500 + (i * 7) % 1000);
        }
        for i in 0..50_000u64 {
            totals.push(5 + (i * 13) % 60);
        }
        let r = barcode_ranks(&totals, 10);
        assert!(
            r.knee_rank > 1500.0 && r.knee_rank < 2600.0,
            "knee_rank={}",
            r.knee_rank
        );
        assert!(r.knee_total >= 2000, "knee_total={}", r.knee_total);
        assert!(
            r.inflection_rank >= 1900.0 && r.inflection_rank < 3000.0,
            "infl={}",
            r.inflection_rank
        );
        assert_eq!(r.n_barcodes_nonzero, 52_000);
    }

    #[test]
    fn tiny_input_falls_back() {
        let r = barcode_ranks(&[5, 3, 0, 1], 1);
        assert_eq!(r.knee_total, 5);
        assert_eq!(r.curve_total, vec![5, 3, 1]);
    }
}
