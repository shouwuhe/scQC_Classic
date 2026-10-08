//! 单一方法环境 RNA 校正（SPEC §8.3；DESIGN §5.11）。
//!
//! **硬约束 C8：禁止串联两种校正。** 在类型层面保证：`correct_*` 只接受 `CountMatrix`（原始计数），
//! 输出 `CorrectedMatrix`（不同类型，且不提供转回 `CountMatrix` 的方法），因此无法把一次校正的结果
//! 再喂给另一次校正。
//!
//! - **SoupX 式**（默认，Young & Behjati 2020 思路）：每个细胞应扣除的环境 UMI 数 target_c = round(ρ_c × total_c)，
//!   按环境谱 a_g 在该细胞已表达的基因上分配（最大余数法取整，逐位确定），单基因扣除不超过其观测计数；
//!   若有基因被"扣空"，差额按 a_g 在剩余基因上继续分配，直到达到 target_c 或无基因可扣。结果为整数计数。
//! - **DecontX 式**：物化 `decontx::decontx_counts` 的期望原生计数（非整数，MTX 以 real 写出）。
//!
//! 报告固定句（§8.3.4）见 [`CAVEAT_ZH`]。

use serde::{Deserialize, Serialize};

use super::decontx::{decontx_counts, DecontXParams, DecontXResult};
use super::profile::AmbientProfile;
use crate::matrix::CountMatrix;

pub const CAVEAT_ZH: &str =
    "所有环境 RNA 校正方法都存在表达量偏倚，对高表达基因校正更强，对低表达基因校正效果有限。";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorrectionMethod {
    Soup,
    DecontX,
}

impl CorrectionMethod {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "soup" | "soupx" => Some(Self::Soup),
            "decontx" => Some(Self::DecontX),
            _ => None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Soup => "soup",
            Self::DecontX => "decontx",
        }
    }
    pub fn label_zh(&self) -> &'static str {
        match self {
            Self::Soup => "SoupX 式减法（整数）",
            Self::DecontX => "DecontX 期望原生计数（非整数）",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorrectedGene {
    pub gene_index: usize,
    pub before: f64,
    pub after: f64,
    pub removed: f64,
    pub removed_fraction: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorrectedCluster {
    pub cluster: u32,
    pub n_cells: usize,
    pub before: f64,
    pub after: f64,
    pub removed_fraction: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorrectionSummary {
    pub method: CorrectionMethod,
    pub method_zh: String,
    pub integer_output: bool,
    pub n_cells: usize,
    pub n_genes: usize,
    /// 所用 ρ 的均值 / 最小 / 最大（SoupX 式为全局值；DecontX 为逐细胞值）
    pub rho_mean: f64,
    pub rho_min: f64,
    pub rho_max: f64,
    pub total_before: f64,
    pub total_after: f64,
    pub removed_fraction: f64,
    /// 目标扣除量未能完全达成的细胞数（可扣的基因已扣空）
    pub n_cells_shortfall: usize,
    pub top_genes: Vec<CorrectedGene>,
    pub per_cluster: Vec<CorrectedCluster>,
    pub caveat_zh: String,
}

/// 校正后的矩阵。**不是 `CountMatrix`**，不能再作为任何校正的输入（C8）。
#[derive(Debug, Clone)]
pub struct CorrectedMatrix {
    pub method: CorrectionMethod,
    pub n_genes: usize,
    pub n_cells: usize,
    /// 按列（细胞）升序、列内按基因升序的 (基因, 细胞, 值)；值 > 0
    pub entries: Vec<(u32, u32, f32)>,
    pub summary: CorrectionSummary,
}

impl CorrectedMatrix {
    pub fn nnz(&self) -> usize {
        self.entries.len()
    }
    /// 逐基因求和
    pub fn gene_totals(&self) -> Vec<f64> {
        let mut t = vec![0.0f64; self.n_genes];
        for &(g, _, v) in &self.entries {
            t[g as usize] += v as f64;
        }
        t
    }
}

/// 最大余数法：把整数 `total` 按权重 `w` 分配到各项，返回整数分配（和恰为 total；权重和为 0 时全 0）。
fn largest_remainder(total: u64, w: &[f64]) -> Vec<u64> {
    let s: f64 = w.iter().sum();
    let n = w.len();
    if total == 0 || s <= 0.0 || n == 0 {
        return vec![0; n];
    }
    let mut alloc = vec![0u64; n];
    let mut frac: Vec<(f64, usize)> = Vec::with_capacity(n);
    let mut assigned = 0u64;
    for (i, &wi) in w.iter().enumerate() {
        let x = total as f64 * wi / s;
        let f = x.floor();
        alloc[i] = f as u64;
        assigned += f as u64;
        frac.push((x - f, i));
    }
    let mut left = total.saturating_sub(assigned);
    frac.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.cmp(&b.1))
    });
    for &(_, i) in &frac {
        if left == 0 {
            break;
        }
        alloc[i] += 1;
        left -= 1;
    }
    alloc
}

fn finish_summary(
    method: CorrectionMethod,
    cells: &CountMatrix,
    cluster: &[u32],
    rho: &[f64],
    entries: &[(u32, u32, f32)],
    n_shortfall: usize,
    integer_output: bool,
) -> CorrectionSummary {
    let g = cells.n_genes();
    let n = cells.n_barcodes();
    let mut before_g = vec![0.0f64; g];
    let mut after_g = vec![0.0f64; g];
    let k = cluster
        .iter()
        .copied()
        .max()
        .map(|m| m as usize + 1)
        .unwrap_or(0);
    let mut before_k = vec![0.0f64; k];
    let mut after_k = vec![0.0f64; k];
    let mut n_k = vec![0usize; k];
    for j in 0..n {
        let (idx, val) = cells.column(j);
        let kk = cluster.get(j).copied().unwrap_or(0) as usize;
        if kk < k {
            n_k[kk] += 1;
        }
        for (&gi, &v) in idx.iter().zip(val) {
            before_g[gi as usize] += v as f64;
            if kk < k {
                before_k[kk] += v as f64;
            }
        }
    }
    for &(gi, j, v) in entries {
        after_g[gi as usize] += v as f64;
        let kk = cluster.get(j as usize).copied().unwrap_or(0) as usize;
        if kk < k {
            after_k[kk] += v as f64;
        }
    }
    let total_before: f64 = before_g.iter().sum();
    let total_after: f64 = after_g.iter().sum();
    let mut genes: Vec<CorrectedGene> = (0..g)
        .filter(|&gi| before_g[gi] > 0.0)
        .map(|gi| CorrectedGene {
            gene_index: gi,
            before: before_g[gi],
            after: after_g[gi],
            removed: before_g[gi] - after_g[gi],
            removed_fraction: (before_g[gi] - after_g[gi]) / before_g[gi],
        })
        .collect();
    genes.sort_by(|a, b| {
        b.removed
            .partial_cmp(&a.removed)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.gene_index.cmp(&b.gene_index))
    });
    genes.truncate(20);
    let per_cluster = (0..k)
        .map(|kk| CorrectedCluster {
            cluster: kk as u32,
            n_cells: n_k[kk],
            before: before_k[kk],
            after: after_k[kk],
            removed_fraction: if before_k[kk] > 0.0 {
                (before_k[kk] - after_k[kk]) / before_k[kk]
            } else {
                0.0
            },
        })
        .collect();
    let finite: Vec<f64> = rho.iter().copied().filter(|v| v.is_finite()).collect();
    let rho_mean = if finite.is_empty() {
        f64::NAN
    } else {
        finite.iter().sum::<f64>() / finite.len() as f64
    };
    CorrectionSummary {
        method,
        method_zh: method.label_zh().to_string(),
        integer_output,
        n_cells: n,
        n_genes: g,
        rho_mean,
        rho_min: finite.iter().copied().fold(f64::INFINITY, f64::min),
        rho_max: finite.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        total_before,
        total_after,
        removed_fraction: if total_before > 0.0 {
            (total_before - total_after) / total_before
        } else {
            0.0
        },
        n_cells_shortfall: n_shortfall,
        top_genes: genes,
        per_cluster,
        caveat_zh: CAVEAT_ZH.to_string(),
    }
}

/// SoupX 式减法校正。`rho_per_cell[j]` 为细胞 j 的污染度（SoupX 默认全局同一值）。
pub fn correct_soup(
    cells: &CountMatrix,
    cluster: &[u32],
    profile: &AmbientProfile,
    rho_per_cell: &[f64],
) -> CorrectedMatrix {
    let n = cells.n_barcodes();
    let mut entries: Vec<(u32, u32, f32)> = Vec::with_capacity(cells.nnz());
    let mut n_shortfall = 0usize;
    for j in 0..n {
        let (idx, val) = cells.column(j);
        let total: u64 = val.iter().map(|&v| v as u64).sum();
        let rho = rho_per_cell.get(j).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let mut target = (rho * total as f64).round() as u64;
        let mut remaining: Vec<u64> = val.iter().map(|&v| v as u64).collect();
        // 迭代分配：每轮只在仍有计数的基因上按环境谱权重分配
        let mut active: Vec<usize> = (0..idx.len())
            .filter(|&t| profile.fraction[idx[t] as usize] > 0.0)
            .collect();
        while target > 0 && !active.is_empty() {
            let w: Vec<f64> = active
                .iter()
                .map(|&t| profile.fraction[idx[t] as usize])
                .collect();
            let alloc = largest_remainder(target, &w);
            let mut progressed = false;
            let mut next_active = Vec::with_capacity(active.len());
            for (a, &t) in alloc.iter().zip(&active) {
                let take = (*a).min(remaining[t]);
                if take > 0 {
                    progressed = true;
                }
                remaining[t] -= take;
                target -= take;
                if remaining[t] > 0 {
                    next_active.push(t);
                }
            }
            active = next_active;
            if !progressed {
                break;
            }
        }
        if target > 0 {
            n_shortfall += 1;
        }
        for (t, &gi) in idx.iter().enumerate() {
            if remaining[t] > 0 {
                entries.push((gi, j as u32, remaining[t] as f32));
            }
        }
    }
    let summary = finish_summary(
        CorrectionMethod::Soup,
        cells,
        cluster,
        rho_per_cell,
        &entries,
        n_shortfall,
        true,
    );
    CorrectedMatrix {
        method: CorrectionMethod::Soup,
        n_genes: cells.n_genes(),
        n_cells: n,
        entries,
        summary,
    }
}

/// DecontX 式校正：用步骤 5 的 θ（= 1 − ρ）与簇谱物化期望原生计数。
pub fn correct_decontx(
    cells: &CountMatrix,
    cluster: &[u32],
    profile: &AmbientProfile,
    estimate: &DecontXResult,
    params: &DecontXParams,
) -> CorrectedMatrix {
    let theta: Vec<f64> = estimate
        .rho_per_cell
        .iter()
        .map(|r| (1.0 - r).clamp(0.0, 1.0))
        .collect();
    let mut entries = decontx_counts(cells, cluster, profile, &theta, params.phi_pseudocount);
    entries.retain(|e| e.2 > 0.0);
    let summary = finish_summary(
        CorrectionMethod::DecontX,
        cells,
        cluster,
        &estimate.rho_per_cell,
        &entries,
        0,
        false,
    );
    CorrectedMatrix {
        method: CorrectionMethod::DecontX,
        n_genes: cells.n_genes(),
        n_cells: cells.n_barcodes(),
        entries,
        summary,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::matrix::Triplet;

    #[test]
    fn largest_remainder_sums_to_total() {
        let a = largest_remainder(10, &[0.5, 0.3, 0.2]);
        assert_eq!(a.iter().sum::<u64>(), 10);
        assert_eq!(a, vec![5, 3, 2]);
        let b = largest_remainder(7, &[1.0, 1.0, 1.0]);
        assert_eq!(b.iter().sum::<u64>(), 7);
        assert_eq!(b, vec![3, 2, 2]);
        assert_eq!(largest_remainder(5, &[0.0, 0.0]), vec![0, 0]);
    }

    #[test]
    fn soup_removes_target_and_stays_integer() {
        // 3 基因 × 2 细胞；环境谱只含基因 0、1
        let trip = vec![
            Triplet {
                row: 0,
                col: 0,
                value: 10,
            },
            Triplet {
                row: 1,
                col: 0,
                value: 10,
            },
            Triplet {
                row: 2,
                col: 0,
                value: 80,
            },
            Triplet {
                row: 0,
                col: 1,
                value: 1,
            },
            Triplet {
                row: 2,
                col: 1,
                value: 99,
            },
        ];
        let m = CountMatrix::from_triplets(3, 2, trip).unwrap();
        let profile = AmbientProfile {
            umi_upper: 100,
            n_droplets: 1,
            total_umi: 10,
            counts: vec![5, 5, 0],
            fraction: vec![0.5, 0.5, 0.0],
            good_turing: vec![0.49, 0.49, 0.02],
        };
        let c = correct_soup(&m, &[0, 0], &profile, &[0.1, 0.1]);
        // 细胞 0：target 10 → 基因 0、1 各扣 5；基因 2 不动
        let cell0: Vec<(u32, f32)> = c
            .entries
            .iter()
            .filter(|e| e.1 == 0)
            .map(|e| (e.0, e.2))
            .collect();
        assert_eq!(cell0, vec![(0, 5.0), (1, 5.0), (2, 80.0)]);
        // 细胞 1：target 10，只有基因 0 可扣（1 个）→ 差额无法达成
        let cell1: Vec<(u32, f32)> = c
            .entries
            .iter()
            .filter(|e| e.1 == 1)
            .map(|e| (e.0, e.2))
            .collect();
        assert_eq!(cell1, vec![(2, 99.0)]);
        assert_eq!(c.summary.n_cells_shortfall, 1);
        assert!((c.summary.total_before - 200.0).abs() < 1e-9);
        assert!((c.summary.total_after - 189.0).abs() < 1e-9);
        assert!(c.summary.integer_output);
    }
}
