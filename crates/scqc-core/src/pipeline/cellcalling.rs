//! 步骤 3：真细胞鉴定（knee + EmptyDrops，或沿用上游判定）。
//!
//! 策略由 `PlatformProfile.cell_calling` 决定（DESIGN §6）：
//! - `EmptyDrops{lower}`：在 raw 矩阵上运行 knee + EmptyDrops；
//! - `TrustUpstream`：只画 knee，细胞集合 = filtered 矩阵；
//! - raw 缺失：同 TrustUpstream，并标记 `RawMissing`。
//!
//! 产出"Cell Ranger 判空但本工具判细胞"（rescued）与"Cell Ranger 判细胞但本工具判空"（lost）
//! 两个列表，是 SPEC §8.1.2 要求的差异化证据。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::cancel::CancelToken;
use super::params::RunParams;
use super::{Progress, Step};
use crate::error::Result;
use crate::io::Dataset;
use crate::platform::{CellCalling, PlatformProfile};
use crate::qc::{barcode_ranks, empty_drops, EmptyDropsParams, EmptyDropsResult, KneeResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellCallingMode {
    EmptyDrops,
    TrustUpstream,
    RawMissing,
}

/// 一个"找回"或"丢失"的细胞的表达特征。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellDelta {
    pub barcode: String,
    pub total_umi: u64,
    pub n_genes: u32,
    pub fdr: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellCallingSummary {
    pub mode: CellCallingMode,
    pub lower: Option<u32>,
    pub knee_total: u64,
    pub inflection_total: u64,
    pub n_raw_barcodes: usize,
    pub n_cellranger_cells: usize,
    pub n_called: usize,
    pub n_rescued: usize,
    pub n_lost: usize,
    /// rescued 细胞的基因数中位数与 top 表达基因（SPEC §17.4 汇报口径）
    pub rescued_median_genes: f64,
    pub rescued_top_genes: Vec<String>,
    pub emptydrops_alpha: Option<f64>,
    pub n_tested: usize,
    pub note_zh: String,
}

pub struct CellCallingOutput {
    pub summary: CellCallingSummary,
    pub knee: KneeResult,
    pub emptydrops: Option<EmptyDropsResult>,
    /// 被判为细胞的 barcode 在 **raw** 矩阵中的列索引（raw 缺失时为 filtered 列索引）
    pub called_cols: Vec<usize>,
    pub rescued: Vec<CellDelta>,
    pub lost: Vec<CellDelta>,
}

pub fn run_cell_calling(
    ds: &Dataset,
    profile: &PlatformProfile,
    params: &RunParams,
    ambient: Option<&crate::ambient::AmbientProfile>,
    cancel: &CancelToken,
    progress: &Progress<'_>,
) -> Result<CellCallingOutput> {
    let filtered_set: HashMap<&str, usize> = ds
        .filtered
        .barcodes
        .barcode
        .iter()
        .enumerate()
        .map(|(i, b)| (b.as_str(), i))
        .collect();

    let (mode, lower) = match (&ds.raw, profile.cell_calling) {
        (None, _) => (CellCallingMode::RawMissing, None),
        (Some(_), CellCalling::TrustUpstream) => (CellCallingMode::TrustUpstream, None),
        (Some(_), CellCalling::EmptyDrops { lower }) => (
            CellCallingMode::EmptyDrops,
            Some(params.cell_calling.lower.unwrap_or(lower)),
        ),
    };

    // knee：有 raw 用 raw，否则用 filtered
    let totals_for_knee = match &ds.raw {
        Some(r) => r.matrix.column_sums(),
        None => ds.filtered.matrix.column_sums(),
    };
    progress.report(Step::CellCalling, 5, "计算 barcode-rank 曲线");
    let knee = barcode_ranks(&totals_for_knee, lower.unwrap_or(1) as u64);
    cancel.checkpoint()?;

    let mut emptydrops = None;
    let mut rescued = Vec::new();
    let mut lost = Vec::new();
    let called_cols: Vec<usize>;
    let note_zh;

    match mode {
        CellCallingMode::EmptyDrops => {
            let raw = ds
                .raw
                .as_ref()
                .ok_or_else(|| crate::ScqcError::internal("raw 缺失"))?;
            let lower_v = lower.unwrap_or(100);
            let ed_params = EmptyDropsParams {
                lower: lower_v,
                niters: params.cell_calling.niters,
                fdr: params.cell_calling.fdr,
                retain: Some(knee.knee_total),
                seed: params.seed,
                include_multinomial_coef: true,
            };
            progress.report(Step::CellCalling, 10, "EmptyDrops：估计环境表达谱");
            let res = empty_drops(
                &raw.matrix,
                &ed_params,
                cancel,
                &|pct| {
                    progress.report(
                        Step::CellCalling,
                        10 + (pct as u32 * 85 / 100) as u8,
                        "EmptyDrops：Monte Carlo 采样中",
                    );
                },
                ambient,
            )?;
            let n_genes_col = |j: usize| raw.matrix.column(j).0.len() as u32;
            let mut called = Vec::new();
            for row in &res.rows {
                let bc = raw.barcodes.barcode[row.col].as_str();
                let in_cr = filtered_set.contains_key(bc);
                if row.called {
                    called.push(row.col);
                    if !in_cr {
                        rescued.push(CellDelta {
                            barcode: bc.to_string(),
                            total_umi: row.total,
                            n_genes: n_genes_col(row.col),
                            fdr: row.fdr,
                        });
                    }
                } else if in_cr {
                    lost.push(CellDelta {
                        barcode: bc.to_string(),
                        total_umi: row.total,
                        n_genes: n_genes_col(row.col),
                        fdr: row.fdr,
                    });
                }
            }
            called_cols = called;
            note_zh = format!(
                "EmptyDrops：lower={lower_v}，retain(knee)={}，niters={}，FDR≤{}",
                knee.knee_total, ed_params.niters, ed_params.fdr
            );
            emptydrops = Some(res);
        }
        CellCallingMode::TrustUpstream => {
            let raw = ds
                .raw
                .as_ref()
                .ok_or_else(|| crate::ScqcError::internal("raw 缺失"))?;
            let raw_index: HashMap<&str, usize> = raw
                .barcodes
                .barcode
                .iter()
                .enumerate()
                .map(|(i, b)| (b.as_str(), i))
                .collect();
            called_cols = ds
                .filtered
                .barcodes
                .barcode
                .iter()
                .filter_map(|b| raw_index.get(b.as_str()).copied())
                .collect();
            note_zh =
                "本平台第一版沿用上游（Cell Ranger）的细胞判定，未独立运行 EmptyDrops（待标定）。"
                    .to_string();
        }
        CellCallingMode::RawMissing => {
            called_cols = (0..ds.filtered.matrix.n_barcodes()).collect();
            note_zh = "缺少 raw 矩阵，真细胞鉴定不可用，沿用 filtered 矩阵的细胞集合。".to_string();
        }
    }

    // rescued 的表达特征：基因数中位数、平均 CP10k 最高的 10 个基因
    let (rescued_median_genes, rescued_top_genes) = match (&ds.raw, rescued.is_empty()) {
        (Some(raw), false) => {
            let raw_index: HashMap<&str, usize> = raw
                .barcodes
                .barcode
                .iter()
                .enumerate()
                .map(|(i, b)| (b.as_str(), i))
                .collect();
            let mut genes: Vec<u64> = rescued.iter().map(|c| c.n_genes as u64).collect();
            genes.sort_unstable();
            let med = if genes.len() % 2 == 1 {
                genes[genes.len() / 2] as f64
            } else {
                (genes[genes.len() / 2 - 1] + genes[genes.len() / 2]) as f64 / 2.0
            };
            let mut mean_cp10k = vec![0.0f64; raw.matrix.n_genes()];
            for c in &rescued {
                if let Some(&j) = raw_index.get(c.barcode.as_str()) {
                    let (idx, val) = raw.matrix.column(j);
                    let t = c.total_umi.max(1) as f64;
                    for (&g, &v) in idx.iter().zip(val) {
                        mean_cp10k[g as usize] += v as f64 / t * 1e4;
                    }
                }
            }
            let mut order: Vec<usize> = (0..mean_cp10k.len()).collect();
            order.sort_by(|&a, &b| {
                mean_cp10k[b]
                    .partial_cmp(&mean_cp10k[a])
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.cmp(&b))
            });
            let top: Vec<String> = order
                .iter()
                .take(10)
                .filter(|&&g| mean_cp10k[g] > 0.0)
                .map(|&g| raw.features.name[g].clone())
                .collect();
            (med, top)
        }
        _ => (0.0, Vec::new()),
    };

    let summary = CellCallingSummary {
        mode,
        lower,
        knee_total: knee.knee_total,
        inflection_total: knee.inflection_total,
        n_raw_barcodes: ds.raw.as_ref().map(|r| r.matrix.n_barcodes()).unwrap_or(0),
        n_cellranger_cells: ds.filtered.matrix.n_barcodes(),
        n_called: called_cols.len(),
        n_rescued: rescued.len(),
        n_lost: lost.len(),
        rescued_median_genes,
        rescued_top_genes,
        emptydrops_alpha: emptydrops.as_ref().map(|e| e.alpha),
        n_tested: emptydrops.as_ref().map(|e| e.n_tested).unwrap_or(0),
        note_zh,
    };
    progress.report(
        Step::CellCalling,
        100,
        format!("判定细胞 {} 个", summary.n_called),
    );
    Ok(CellCallingOutput {
        summary,
        knee,
        emptydrops,
        called_cols,
        rescued,
        lost,
    })
}
