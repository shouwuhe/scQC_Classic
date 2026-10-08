//! 环境 RNA 诊断（步骤 5）：双估计器 + 可选第三估计器 + 一致性判定（SPEC §8.2）。
//!
//! 【硬约束】默认只诊断不校正；两个估计器估计的都是"来自空液滴环境谱"的污染
//! （DecontX 以 background 模式运行）；必须报告全部估计值。
//! 本目录不出现平台名：哪些估计器启用、参考区间是什么，全部来自 `PlatformProfile` 字段。

pub mod consensus;
pub mod correct;
pub mod decontx;
pub mod mito;
pub mod profile;
pub mod soup;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::matrix::CountMatrix;
use crate::pipeline::cancel::CancelToken;
use crate::platform::{AmbientEstimator, AmbientReference};

pub use consensus::{Consensus, CorrectionState, Rating, Verdict};
pub use correct::{CorrectedMatrix, CorrectionMethod, CorrectionSummary};
pub use decontx::{DecontXParams, DecontXResult};
pub use mito::MitoResult;
pub use profile::AmbientProfile;
pub use soup::{SoupParams, SoupResult};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AffectedGene {
    pub gene: String,
    pub observed_counts: f64,
    pub expected_ambient_counts: f64,
    /// min(1, 期望环境计数 / 观测计数)
    pub ambient_fraction: f64,
}

/// `ambient_estimates.json` 的内容（DESIGN §10.5）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AmbientEstimates {
    pub profile: AmbientProfileSummary,
    pub soup: Option<SoupSummary>,
    pub decontx: Option<DecontXSummary>,
    pub mito: Option<MitoSummary>,
    pub consensus: Consensus,
    pub top_affected_genes: Vec<AffectedGene>,
    pub timing_s: AmbientTiming,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AmbientProfileSummary {
    pub umi_upper: u64,
    pub n_droplets: usize,
    pub total_umi: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoupSummary {
    pub rho_global: f64,
    /// 众数落在搜索区间端点：数值仍然报出，但不参与一致性判定
    pub at_boundary: bool,
    pub rho_low: f64,
    pub rho_high: f64,
    pub rho_per_cluster: Vec<f64>,
    pub n_markers: usize,
    pub n_pairs_used: usize,
    pub n_pairs_excluded: usize,
    pub note_zh: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecontXSummary {
    pub rho_global: f64,
    pub rho_median: f64,
    pub rho_min: f64,
    pub rho_max: f64,
    pub n_iter: usize,
    pub converged: bool,
    pub log_likelihood: f64,
    pub delta: (f64, f64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MitoSummary {
    pub rho_global: f64,
    pub a_mito: f64,
    pub usable: bool,
    pub note_zh: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AmbientTiming {
    pub soup_s: f64,
    pub decontx_s: f64,
    pub mito_s: f64,
}

pub struct AmbientOutput {
    pub estimates: AmbientEstimates,
    pub soup: Option<SoupResult>,
    pub decontx: Option<DecontXResult>,
    pub mito: Option<MitoResult>,
}

pub struct AmbientInputs<'a> {
    pub cells: &'a CountMatrix,
    pub coarse_cluster: &'a [u32],
    pub profile: &'a AmbientProfile,
    pub gene_names: &'a [String],
    pub mito_genes: &'a [usize],
    pub estimators: &'a [AmbientEstimator],
    pub reference: Option<AmbientReference>,
    pub decontx_params: DecontXParams,
    pub soup_params: SoupParams,
}

pub fn run_ambient(
    inp: &AmbientInputs<'_>,
    cancel: &CancelToken,
    on_progress: &dyn Fn(u8, &str),
) -> Result<AmbientOutput> {
    let mut timing = AmbientTiming::default();
    let mut soup = None;
    let mut decontx = None;
    let mut mito = None;

    if inp.estimators.contains(&AmbientEstimator::Soup) {
        on_progress(5, "估计器 A：空液滴谱（SoupX 式）");
        let t = std::time::Instant::now();
        soup = Some(soup::estimate_soup(
            inp.cells,
            inp.coarse_cluster,
            inp.profile,
            &inp.soup_params,
        ));
        timing.soup_s = t.elapsed().as_secs_f64();
        cancel.checkpoint()?;
    }
    if inp.estimators.contains(&AmbientEstimator::DecontX) {
        on_progress(20, "估计器 B：DecontX（background 模式）变分 EM");
        let t = std::time::Instant::now();
        let max_iter = inp.decontx_params.max_iter.max(1);
        let r = decontx::decontx_background(
            inp.cells,
            inp.coarse_cluster,
            inp.profile,
            &inp.decontx_params,
            cancel,
            &|it, _ll| {
                if it % 10 == 0 {
                    let pct = 20 + ((it * 70) / max_iter).min(70) as u8;
                    on_progress(pct, "DecontX 迭代中");
                }
            },
        )?;
        timing.decontx_s = t.elapsed().as_secs_f64();
        decontx = Some(r);
        cancel.checkpoint()?;
    }
    if inp.estimators.contains(&AmbientEstimator::Mito) {
        on_progress(92, "估计器 C：线粒体法");
        let t = std::time::Instant::now();
        mito = Some(mito::estimate_mito(inp.cells, inp.mito_genes, inp.profile));
        timing.mito_s = t.elapsed().as_secs_f64();
    }

    // 落在网格端点的 SoupX 式估计不可信：数值照常报出，但不参与一致性判定（否则会伪造分歧）
    let rho_soup = soup
        .as_ref()
        .filter(|s| !s.at_boundary)
        .map(|s| s.rho_global);
    let rho_dx = decontx.as_ref().map(|d| d.rho_global);
    let rho_mito = mito.as_ref().filter(|m| m.usable).map(|m| m.rho_global);
    let consensus = consensus::assess(rho_soup, rho_dx, rho_mito, inp.reference);

    // 受影响最大的基因：期望环境计数 = ρ · N_total · a_g
    let rho_for_genes = consensus
        .interval
        .map(|(lo, hi)| (lo + hi) / 2.0)
        .unwrap_or(0.0);
    let gene_totals = inp.cells.row_sums();
    let n_total: f64 = gene_totals.iter().map(|&v| v as f64).sum();
    let mut affected: Vec<AffectedGene> = gene_totals
        .iter()
        .enumerate()
        .filter(|(_, &obs)| obs > 0)
        .map(|(g, &obs)| {
            let exp = rho_for_genes * n_total * inp.profile.fraction[g];
            AffectedGene {
                gene: inp.gene_names.get(g).cloned().unwrap_or_default(),
                observed_counts: obs as f64,
                expected_ambient_counts: exp,
                ambient_fraction: (exp / obs as f64).min(1.0),
            }
        })
        .collect();
    affected.sort_by(|a, b| {
        b.expected_ambient_counts
            .partial_cmp(&a.expected_ambient_counts)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.gene.cmp(&b.gene))
    });
    affected.truncate(20);

    let dx_summary = decontx.as_ref().map(|d| {
        let mut v: Vec<f64> = d.rho_per_cell.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let med = if v.is_empty() {
            f64::NAN
        } else {
            v[v.len() / 2]
        };
        DecontXSummary {
            rho_global: d.rho_global,
            rho_median: med,
            rho_min: v.first().copied().unwrap_or(f64::NAN),
            rho_max: v.last().copied().unwrap_or(f64::NAN),
            n_iter: d.n_iter,
            converged: d.converged,
            log_likelihood: d.log_likelihood,
            delta: d.delta,
        }
    });
    let estimates = AmbientEstimates {
        profile: AmbientProfileSummary {
            umi_upper: inp.profile.umi_upper,
            n_droplets: inp.profile.n_droplets,
            total_umi: inp.profile.total_umi,
        },
        soup: soup.as_ref().map(|s| SoupSummary {
            rho_global: s.rho_global,
            at_boundary: s.at_boundary,
            rho_low: s.rho_low,
            rho_high: s.rho_high,
            rho_per_cluster: s.rho_per_cluster.clone(),
            n_markers: s.n_markers,
            n_pairs_used: s.n_pairs_used,
            n_pairs_excluded: s.n_pairs_excluded,
            note_zh: s.note_zh.clone(),
        }),
        decontx: dx_summary,
        mito: mito.as_ref().map(|m| MitoSummary {
            rho_global: m.rho_global,
            a_mito: m.a_mito,
            usable: m.usable,
            note_zh: m.note_zh.clone(),
        }),
        consensus,
        top_affected_genes: affected,
        timing_s: timing,
    };
    on_progress(100, "环境 RNA 诊断完成");
    Ok(AmbientOutput {
        estimates,
        soup,
        decontx,
        mito,
    })
}
