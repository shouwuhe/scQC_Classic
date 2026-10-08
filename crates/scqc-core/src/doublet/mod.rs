//! 双细胞检测（SPEC §8.5；DESIGN §5.9）。
//!
//! 两个方法：cxds（快筛，§8.5.1）与 DoubletFinder（精确，§8.5.2），并按 §8.5.4 输出三类一致性结论：
//! 两方法均判为双细胞 = 高置信（默认剔除）；仅一方法 = 边界（默认保留并标注）；均为单细胞 = 正常。
//!
//! 执行顺序（§8.5.5）：在低质量细胞过滤之后，**每个样本独立运行**（多重化 Flex 中一个 Probe Barcode
//! = 一个样本）。预期双细胞率由 Platform Profile 计算（§7.2.3），本模块只接收数值。
//!
//! 本模块不含任何平台判断（CI 禁止在本目录出现平台名）。

pub mod cxds;
pub mod doubletfinder;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::matrix::CountMatrix;
use crate::pipeline::cancel::CancelToken;
pub use cxds::{CxdsParams, CxdsResult};
pub use doubletfinder::{DoubletFinderParams, DoubletFinderResult, SweepRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoubletClass {
    /// 两方法均判为单细胞
    Singlet,
    /// 仅一个方法判为双细胞：默认保留并标注
    Borderline,
    /// 两方法均判为双细胞：默认剔除
    HighConfidence,
}

impl DoubletClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            DoubletClass::Singlet => "singlet",
            DoubletClass::Borderline => "borderline",
            DoubletClass::HighConfidence => "high_confidence",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DoubletParams {
    pub cxds: CxdsParams,
    pub df: DoubletFinderParams,
}

/// 输入：`cells` 为已通过低质量过滤的细胞矩阵；`sample` 为每细胞的样本编号（0..S），
/// `sample_labels` 为各样本的显示名；`expected_rate` 为预期双细胞率（已含多重化修正）。
pub struct DoubletInputs<'a> {
    pub cells: &'a CountMatrix,
    pub gene_candidates: &'a [usize],
    pub sample: &'a [u32],
    pub sample_labels: &'a [String],
    pub expected_rate: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampleDoubletSummary {
    pub label: String,
    pub n_cells: usize,
    pub n_expected: usize,
    pub cxds_n_called: usize,
    pub df_n_called: usize,
    pub df_p_k: f64,
    pub df_p_n: f64,
    pub df_n_artificial: usize,
    pub df_sweep_n_cells: usize,
    pub df_sweep: Vec<SweepRow>,
    pub n_high_confidence: usize,
    pub n_borderline: usize,
    pub n_singlet: usize,
    /// 两方法判定集合的 Jaccard 一致性
    pub agreement_jaccard: f64,
    /// 细胞数过少（< 20）时不做检测
    pub skipped_zh: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DoubletSummary {
    pub expected_rate: f64,
    pub n_cells: usize,
    pub n_samples: usize,
    pub n_high_confidence: usize,
    pub n_borderline: usize,
    pub n_singlet: usize,
    pub cxds_n_called: usize,
    pub df_n_called: usize,
    pub agreement_jaccard: f64,
    pub per_sample: Vec<SampleDoubletSummary>,
    pub timing_s: DoubletTiming,
    /// 报告"已知局限性"用：预期率为经验假设
    pub assumption_zh: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DoubletTiming {
    pub cxds_s: f64,
    pub doubletfinder_s: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DoubletOutput {
    pub cxds_score: Vec<f64>,
    pub cxds_call: Vec<bool>,
    pub df_pann: Vec<f64>,
    pub df_call: Vec<bool>,
    pub class: Vec<DoubletClass>,
    pub summary: DoubletSummary,
}

/// 按得分取前 n_expected 个为双细胞（并列按索引），与 DoubletFinder 的卡阈值方式一致。
pub fn call_top(score: &[f64], n_expected: usize) -> Vec<bool> {
    let mut order: Vec<usize> = (0..score.len()).collect();
    order.sort_by(|&a, &b| {
        score[b]
            .partial_cmp(&score[a])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    let mut call = vec![false; score.len()];
    for &j in order.iter().take(n_expected.min(score.len())) {
        call[j] = true;
    }
    call
}

fn jaccard(a: &[bool], b: &[bool]) -> f64 {
    let mut inter = 0usize;
    let mut uni = 0usize;
    for (&x, &y) in a.iter().zip(b) {
        if x && y {
            inter += 1;
        }
        if x || y {
            uni += 1;
        }
    }
    if uni == 0 {
        f64::NAN
    } else {
        inter as f64 / uni as f64
    }
}

/// 运行两个方法（每样本独立）并给出一致性分类。
pub fn run_doublet(
    inputs: &DoubletInputs<'_>,
    params: &DoubletParams,
    cancel: &CancelToken,
    on_progress: &dyn Fn(u8, &str),
) -> Result<DoubletOutput> {
    let n = inputs.cells.n_barcodes();
    let n_samples = inputs.sample_labels.len().max(1);
    let mut cxds_score = vec![f64::NAN; n];
    let mut cxds_call = vec![false; n];
    let mut df_pann = vec![f64::NAN; n];
    let mut df_call = vec![false; n];
    let mut per_sample = Vec::with_capacity(n_samples);
    let mut timing = DoubletTiming::default();

    for s in 0..n_samples {
        cancel.checkpoint()?;
        let label = inputs
            .sample_labels
            .get(s)
            .cloned()
            .unwrap_or_else(|| "sample".to_string());
        let cols: Vec<usize> = (0..n)
            .filter(|&j| inputs.sample.get(j).copied().unwrap_or(0) as usize == s)
            .collect();
        let m = cols.len();
        let n_expected = ((inputs.expected_rate * m as f64).round() as usize).min(m);
        let base_pct = (s * 100 / n_samples) as u8;
        let span = (100 / n_samples).max(1) as u8;
        let prog = |p: u8, msg: &str| {
            let pct = base_pct.saturating_add((p as usize * span as usize / 100) as u8);
            if n_samples > 1 {
                on_progress(pct.min(99), &format!("样本 {label}：{msg}"));
            } else {
                on_progress(pct.min(99), msg);
            }
        };
        if m < 20 {
            per_sample.push(SampleDoubletSummary {
                label,
                n_cells: m,
                n_expected: 0,
                cxds_n_called: 0,
                df_n_called: 0,
                df_p_k: f64::NAN,
                df_p_n: params.df.p_n,
                df_n_artificial: 0,
                df_sweep_n_cells: 0,
                df_sweep: Vec::new(),
                n_high_confidence: 0,
                n_borderline: 0,
                n_singlet: m,
                agreement_jaccard: f64::NAN,
                skipped_zh: Some(format!("细胞数 {m} < 20，未做双细胞检测")),
            });
            for &j in &cols {
                cxds_score[j] = 0.0;
                df_pann[j] = 0.0;
            }
            continue;
        }
        let sub = inputs.cells.select_columns(&cols)?;

        prog(0, "cxds 共表达打分");
        let t = std::time::Instant::now();
        let cx = cxds::cxds(&sub, &params.cxds);
        let cx_call = call_top(&cx.score, n_expected);
        timing.cxds_s += t.elapsed().as_secs_f64();
        cancel.checkpoint()?;

        let t = std::time::Instant::now();
        let df = doubletfinder::doublet_finder(
            &sub,
            inputs.gene_candidates,
            inputs.expected_rate,
            &params.df,
            cancel,
            &|p, msg| prog(5 + (p as usize * 95 / 100) as u8, msg),
        )?;
        timing.doubletfinder_s += t.elapsed().as_secs_f64();

        let mut hi = 0usize;
        let mut bo = 0usize;
        for (k, &j) in cols.iter().enumerate() {
            cxds_score[j] = cx.score[k];
            cxds_call[j] = cx_call[k];
            df_pann[j] = df.p_ann[k];
            df_call[j] = df.call[k];
            match (cx_call[k], df.call[k]) {
                (true, true) => hi += 1,
                (false, false) => {}
                _ => bo += 1,
            }
        }
        per_sample.push(SampleDoubletSummary {
            label,
            n_cells: m,
            n_expected,
            cxds_n_called: cx_call.iter().filter(|&&b| b).count(),
            df_n_called: df.call.iter().filter(|&&b| b).count(),
            df_p_k: df.p_k,
            df_p_n: df.p_n,
            df_n_artificial: df.n_artificial,
            df_sweep_n_cells: df.sweep_n_cells,
            df_sweep: df.sweep,
            n_high_confidence: hi,
            n_borderline: bo,
            n_singlet: m - hi - bo,
            agreement_jaccard: jaccard(&cx_call, &df.call),
            skipped_zh: None,
        });
    }

    let class: Vec<DoubletClass> = cxds_call
        .iter()
        .zip(&df_call)
        .map(|(&a, &b)| match (a, b) {
            (true, true) => DoubletClass::HighConfidence,
            (false, false) => DoubletClass::Singlet,
            _ => DoubletClass::Borderline,
        })
        .collect();
    let n_high = class
        .iter()
        .filter(|&&c| c == DoubletClass::HighConfidence)
        .count();
    let n_border = class
        .iter()
        .filter(|&&c| c == DoubletClass::Borderline)
        .count();
    on_progress(100, "双细胞检测完成");
    Ok(DoubletOutput {
        summary: DoubletSummary {
            expected_rate: inputs.expected_rate,
            n_cells: n,
            n_samples,
            n_high_confidence: n_high,
            n_borderline: n_border,
            n_singlet: n - n_high - n_border,
            cxds_n_called: cxds_call.iter().filter(|&&b| b).count(),
            df_n_called: df_call.iter().filter(|&&b| b).count(),
            agreement_jaccard: jaccard(&cxds_call, &df_call),
            per_sample,
            timing_s: timing,
            assumption_zh: "目前没有从数据直接估计双细胞率的共识方法；本工具使用基于上样细胞数（及多重化重数）的经验公式 \
                            rate = rate_per_1000 × n_loaded / 1000 ÷ multiplex_n，该假设可在参数中调整。"
                .to_string(),
        },
        cxds_score,
        cxds_call,
        df_pann,
        df_call,
        class,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn call_top_ties_are_deterministic() {
        let s = [1.0, 3.0, 3.0, 0.5];
        assert_eq!(call_top(&s, 2), vec![false, true, true, false]);
        assert_eq!(call_top(&s, 0), vec![false; 4]);
        assert_eq!(call_top(&s, 10), vec![true; 4]);
    }

    #[test]
    fn jaccard_basic() {
        assert!((jaccard(&[true, true, false], &[true, false, false]) - 0.5).abs() < 1e-12);
        assert!(jaccard(&[false, false], &[false, false]).is_nan());
    }
}
