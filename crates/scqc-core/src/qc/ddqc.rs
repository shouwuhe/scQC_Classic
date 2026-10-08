//! ddqc —— 数据驱动的低质量细胞过滤（SPEC §8.4；DESIGN §5.12）。
//!
//! 依据：Subramanian A et al. (2022) "Biology-inspired data-driven quality control for
//! scientific discovery in single-cell transcriptomics", Genome Biology 23:267；
//! 参考实现 ddqc_R（BSD-3-Clause）。
//!
//! 算法：
//! 1. 粗过滤：基因数 < `coarse_min_genes`(100) 或线粒体比例 > `coarse_max_mito`(80%) 剔除；
//! 2. 精细聚类（外部传入，res 1.3）；
//! 3. 每簇每维度用中位数 ± k·MAD（MAD 取 R `mad()` 口径，常数 1.4826）判离群：
//!    - 下限维度（总 UMI、基因数）：阈值 = min(median − k·MAD, 下限护栏)；
//!    - 上限维度（线粒体、核糖体）：阈值 = max(median + k·MAD, 上限护栏)；
//!
//!    护栏语义（DESIGN Q3，已确认）：基因数 > 200 或线粒体 < 护栏值的细胞绝不因该维度被删。
//!    线粒体可选"MAD + 最小差值"：阈值 = median + max(k·MAD, min_diff)（OSCA `isOutlier(min_diff)`）。
//! 4. 同时给出固定阈值对照组（percent.mt < 10%、基因数 > 500）的结果，用于"多保留了什么"。
//!
//! 哪些维度启用、护栏取值、最小差值，全部来自 `PlatformProfile` 字段（本模块不问平台名）。

use serde::{Deserialize, Serialize};

use crate::platform::DdqcDim;
use crate::qc::metrics::BasicMetrics;

/// R `mad()` 的默认常数。
pub const MAD_SCALE_R: f64 = 1.4826;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DdqcParams {
    pub dims: Vec<DdqcDim>,
    pub mad_multiplier: f64,
    pub mad_scale: f64,
    pub coarse_min_genes: u32,
    pub coarse_max_mito: f64,
    /// 基因数阈值上限（护栏）
    pub genes_lower_bound: f64,
    /// 线粒体阈值下限（护栏）；None ⇒ 不加护栏（报告注明）
    pub mito_upper_bound: Option<f64>,
    /// 线粒体最小差值（比例，如 0.005）
    pub mito_min_diff: Option<f64>,
    /// 固定阈值对照组
    pub fixed_min_genes: u32,
    pub fixed_max_mito: f64,
    /// **严格模式**（项目负责人 2026-09-11 定的默认口径）：固定阈值不只是对照，
    /// 而是硬性下限——细胞必须同时通过"簇内自适应阈值"和"固定阈值"才保留。
    /// 理由：现在的测序通量下细胞数不稀缺，宁可少留一些，也不要让特征暧昧的细胞
    /// 拖低下游的科学解释度。设为 false 则回到 ddqc 原口径（只做对照，不参与过滤）。
    pub fixed_thresholds_are_hard: bool,
}

impl Default for DdqcParams {
    fn default() -> Self {
        Self {
            dims: vec![
                DdqcDim::Counts,
                DdqcDim::Genes,
                DdqcDim::Mito,
                DdqcDim::Ribo,
            ],
            mad_multiplier: 2.0,
            mad_scale: MAD_SCALE_R,
            coarse_min_genes: 100,
            coarse_max_mito: 0.8,
            genes_lower_bound: 200.0,
            mito_upper_bound: Some(0.10),
            mito_min_diff: None,
            fixed_min_genes: 500,
            fixed_max_mito: 0.10,
            fixed_thresholds_are_hard: true,
        }
    }
}

/// 一个细胞的剔除原因（第一个未通过的维度）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DdqcFail {
    pub dim: DdqcDim,
    pub cluster: Option<u32>,
    pub median: f64,
    pub mad: f64,
    pub threshold: f64,
    pub observed: f64,
    /// ">" 或 "<"
    pub comparator: String,
    pub coarse: bool,
}

/// 每簇每维度的阈值（报告用）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterThreshold {
    pub cluster: u32,
    pub dim: DdqcDim,
    pub n_cells: usize,
    pub median: f64,
    pub mad: f64,
    pub threshold: f64,
    pub n_removed: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DdqcResult {
    pub pass: Vec<bool>,
    pub fail: Vec<Option<DdqcFail>>,
    pub thresholds: Vec<ClusterThreshold>,
    pub n_cells: usize,
    pub n_removed_coarse: usize,
    pub n_removed_total: usize,
    pub removed_by_dim: Vec<(DdqcDim, usize)>,
    /// 固定阈值对照组：pass 向量与剔除数
    pub fixed_pass: Vec<bool>,
    pub fixed_n_removed: usize,
    /// 严格模式下"自适应阈值放过、固定阈值拦下"的细胞数
    pub n_removed_by_fixed: usize,
    /// ddqc 保留但固定阈值会删掉的细胞数（产品卖点）
    pub n_extra_kept: usize,
    pub n_extra_kept_median_genes: f64,
    pub params: DdqcParams,
}

fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn mad(v: &[f64], med: f64, scale: f64) -> f64 {
    let mut dev: Vec<f64> = v.iter().map(|x| (x - med).abs()).collect();
    scale * median(&mut dev)
}

fn is_upper(dim: DdqcDim) -> bool {
    matches!(dim, DdqcDim::Mito | DdqcDim::Ribo)
}

fn value_of(dim: DdqcDim, m: &BasicMetrics, j: usize) -> f64 {
    match dim {
        DdqcDim::Counts => m.total_umi[j] as f64,
        DdqcDim::Genes => m.n_genes[j] as f64,
        DdqcDim::Mito => m.pct_mito[j],
        DdqcDim::Ribo => m.pct_ribo[j],
    }
}

/// `metrics`：细胞矩阵上的基础指标；`cluster`：精细簇标签。
pub fn ddqc(metrics: &BasicMetrics, cluster: &[u32], params: &DdqcParams) -> DdqcResult {
    let n = metrics.len();
    let mut pass = vec![true; n];
    let mut fail: Vec<Option<DdqcFail>> = vec![None; n];

    // 1. 粗过滤
    let mut n_removed_coarse = 0;
    for j in 0..n {
        let genes = metrics.n_genes[j] as f64;
        let mito = metrics.pct_mito[j];
        if genes < params.coarse_min_genes as f64 {
            pass[j] = false;
            fail[j] = Some(DdqcFail {
                dim: DdqcDim::Genes,
                cluster: None,
                median: f64::NAN,
                mad: f64::NAN,
                threshold: params.coarse_min_genes as f64,
                observed: genes,
                comparator: "<".into(),
                coarse: true,
            });
            n_removed_coarse += 1;
        } else if mito > params.coarse_max_mito {
            pass[j] = false;
            fail[j] = Some(DdqcFail {
                dim: DdqcDim::Mito,
                cluster: None,
                median: f64::NAN,
                mad: f64::NAN,
                threshold: params.coarse_max_mito,
                observed: mito,
                comparator: ">".into(),
                coarse: true,
            });
            n_removed_coarse += 1;
        }
    }

    // 2–3. 簇内 MAD（粗过滤后的细胞参与统计）
    let k = cluster
        .iter()
        .copied()
        .max()
        .map(|m| m as usize + 1)
        .unwrap_or(0);
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); k];
    for j in 0..n {
        if pass[j] {
            members[cluster[j] as usize].push(j);
        }
    }
    let mut thresholds = Vec::new();
    let mut removed_by_dim: Vec<(DdqcDim, usize)> = params.dims.iter().map(|&d| (d, 0)).collect();
    for &dim in &params.dims {
        for (c, mem) in members.iter().enumerate() {
            if mem.is_empty() {
                continue;
            }
            let mut vals: Vec<f64> = mem.iter().map(|&j| value_of(dim, metrics, j)).collect();
            let med = median(&mut vals);
            let md = mad(&vals, med, params.mad_scale);
            let (threshold, upper) = if is_upper(dim) {
                let mut delta = params.mad_multiplier * md;
                if dim == DdqcDim::Mito {
                    if let Some(min_diff) = params.mito_min_diff {
                        delta = delta.max(min_diff);
                    }
                }
                let mut t = med + delta;
                if dim == DdqcDim::Mito {
                    if let Some(b) = params.mito_upper_bound {
                        t = t.max(b);
                    }
                }
                (t, true)
            } else {
                let mut t = med - params.mad_multiplier * md;
                if dim == DdqcDim::Genes {
                    t = t.min(params.genes_lower_bound);
                }
                (t, false)
            };
            let mut n_removed = 0;
            for &j in mem {
                if !pass[j] {
                    continue;
                }
                let v = value_of(dim, metrics, j);
                let out = if upper { v > threshold } else { v < threshold };
                if out {
                    pass[j] = false;
                    fail[j] = Some(DdqcFail {
                        dim,
                        cluster: Some(c as u32),
                        median: med,
                        mad: md,
                        threshold,
                        observed: v,
                        comparator: if upper { ">".into() } else { "<".into() },
                        coarse: false,
                    });
                    n_removed += 1;
                }
            }
            if let Some(e) = removed_by_dim.iter_mut().find(|e| e.0 == dim) {
                e.1 += n_removed;
            }
            thresholds.push(ClusterThreshold {
                cluster: c as u32,
                dim,
                n_cells: mem.len(),
                median: med,
                mad: md,
                threshold,
                n_removed,
            });
        }
    }

    // 4. 固定阈值：默认作为硬性下限（严格模式），同时始终作为对照统计
    let fixed_pass: Vec<bool> = (0..n)
        .map(|j| {
            metrics.pct_mito[j] < params.fixed_max_mito
                && metrics.n_genes[j] > params.fixed_min_genes
        })
        .collect();
    let fixed_n_removed = fixed_pass.iter().filter(|&&p| !p).count();
    let extra: Vec<usize> = (0..n).filter(|&j| pass[j] && !fixed_pass[j]).collect();
    let mut extra_genes: Vec<f64> = extra.iter().map(|&j| metrics.n_genes[j] as f64).collect();
    let n_extra_kept_median_genes = if extra.is_empty() {
        0.0
    } else {
        median(&mut extra_genes)
    };

    let mut n_removed_by_fixed = 0usize;
    if params.fixed_thresholds_are_hard {
        for j in 0..n {
            if pass[j] && !fixed_pass[j] {
                pass[j] = false;
                n_removed_by_fixed += 1;
                let mito_bad = metrics.pct_mito[j] >= params.fixed_max_mito;
                fail[j] = Some(DdqcFail {
                    dim: if mito_bad {
                        DdqcDim::Mito
                    } else {
                        DdqcDim::Genes
                    },
                    cluster: None,
                    median: f64::NAN,
                    mad: f64::NAN,
                    threshold: if mito_bad {
                        params.fixed_max_mito
                    } else {
                        params.fixed_min_genes as f64
                    },
                    observed: if mito_bad {
                        metrics.pct_mito[j]
                    } else {
                        metrics.n_genes[j] as f64
                    },
                    comparator: if mito_bad { ">=".into() } else { "<=".into() },
                    coarse: true,
                });
                if let Some(e) = removed_by_dim.iter_mut().find(|(d, _)| {
                    *d == if mito_bad {
                        DdqcDim::Mito
                    } else {
                        DdqcDim::Genes
                    }
                }) {
                    e.1 += 1;
                }
            }
        }
    }

    let n_removed_total = pass.iter().filter(|&&p| !p).count();
    DdqcResult {
        n_removed_by_fixed,
        pass,
        fail,
        thresholds,
        n_cells: n,
        n_removed_coarse,
        n_removed_total,
        removed_by_dim,
        fixed_pass,
        fixed_n_removed,
        n_extra_kept: extra.len(),
        n_extra_kept_median_genes,
        params: params.clone(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn metrics(genes: &[u32], mito: &[f64]) -> BasicMetrics {
        BasicMetrics {
            total_umi: genes.iter().map(|&g| g as u64 * 3).collect(),
            n_genes: genes.to_vec(),
            pct_mito: mito.to_vec(),
            pct_ribo: vec![0.2; genes.len()],
        }
    }

    #[test]
    fn guardrail_protects_low_mito_cells() {
        // 一个簇：线粒体 1%–3%，一个细胞 6%：MAD 很小，但护栏 10% 保护它
        let genes = vec![1000; 10];
        let mito = vec![
            0.01, 0.012, 0.015, 0.02, 0.022, 0.025, 0.03, 0.018, 0.021, 0.06,
        ];
        let r = ddqc(
            &metrics(&genes, &mito),
            &[0; 10],
            &DdqcParams {
                dims: vec![DdqcDim::Mito],
                ..Default::default()
            },
        );
        assert_eq!(r.n_removed_total, 0);
        // 无护栏时会被删
        let r2 = ddqc(
            &metrics(&genes, &mito),
            &[0; 10],
            &DdqcParams {
                dims: vec![DdqcDim::Mito],
                mito_upper_bound: None,
                ..Default::default()
            },
        );
        assert_eq!(r2.n_removed_total, 1);
        assert_eq!(r2.fail[9].as_ref().map(|f| f.dim), Some(DdqcDim::Mito));
    }

    #[test]
    fn min_diff_prevents_zero_mad_collapse() {
        // 多数细胞线粒体为 0 → MAD = 0；min_diff 0.5% 保护 0.3% 的细胞，但 2% 的仍被删（护栏 None 模拟）
        let genes = vec![1000; 12];
        let mut mito = vec![0.0; 12];
        mito[10] = 0.003;
        mito[11] = 0.02;
        let p = DdqcParams {
            dims: vec![DdqcDim::Mito],
            mito_upper_bound: None,
            mito_min_diff: Some(0.005),
            ..Default::default()
        };
        let r = ddqc(&metrics(&genes, &mito), &[0; 12], &p);
        assert!(r.pass[10]);
        assert!(!r.pass[11]);
        // 没有 min_diff：0.3% 也会被删（MAD = 0）
        let r2 = ddqc(
            &metrics(&genes, &mito),
            &[0; 12],
            &DdqcParams {
                mito_min_diff: None,
                ..p
            },
        );
        assert!(!r2.pass[10]);
    }

    #[test]
    fn coarse_filter_and_fixed_control() {
        let genes = vec![50, 800, 900, 1000, 300];
        let mito = vec![0.01, 0.9, 0.02, 0.03, 0.04];
        let r = ddqc(
            &metrics(&genes, &mito),
            &[0; 5],
            &DdqcParams {
                dims: vec![],
                ..Default::default()
            },
        );
        assert_eq!(r.n_removed_coarse, 2);
        assert!(r.fail[0].as_ref().unwrap().coarse);
        // 固定阈值：基因 > 500 且 mito < 10% → 保留 2 个（900、1000），删 3 个；
        // ddqc 保留 3 个（含 300 基因的细胞）→ 多保留 1 个
        assert_eq!(r.fixed_n_removed, 3);
        assert_eq!(r.n_extra_kept, 1);
        // 严格模式（默认）：固定阈值是硬下限 → 那个 300 基因的细胞也被删
        assert_eq!(r.n_removed_by_fixed, 1);
        assert!(!r.pass[4]);
        // 关掉严格模式则回到 ddqc 原口径：固定阈值只做对照
        let r2 = ddqc(
            &metrics(&genes, &mito),
            &[0; 5],
            &DdqcParams {
                dims: vec![],
                fixed_thresholds_are_hard: false,
                ..Default::default()
            },
        );
        assert_eq!(r2.n_removed_by_fixed, 0);
        assert!(r2.pass[4]);
        assert_eq!(r2.n_extra_kept, 1);
    }
}
