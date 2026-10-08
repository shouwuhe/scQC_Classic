//! 把各步骤的结果组装成 `ReportData`（SPEC §12 九节），再由 `scqc_core::report` 渲染。
//! 所有数值在这里格式化为字符串；同一结构写成 `report_data.json` 供逐数核对。

use std::collections::HashMap;

use scqc_core::ambient::{self, CorrectedMatrix, CorrectionState, Rating, Verdict};
use scqc_core::doublet::DoubletOutput;
use scqc_core::embed::Embedding;
use scqc_core::io::Dataset;
use scqc_core::pipeline::cellcalling::{CellCallingMode, CellCallingOutput};
use scqc_core::pipeline::params::RunRecord;
use scqc_core::platform::{DdqcDim, MitoSemantics, PlatformProfile};
use scqc_core::qc::{BasicMetrics, DdqcResult};
use scqc_core::report::i18n::{Lang, M};
use scqc_core::report::{
    methods, svg, AmbientSection, CellCallingSection, CorrectionSection, DdqcSection,
    DoubletSection, EstimateRow, FinalSection, GeneRow, KeyValue, OverviewSection, ReportData,
    SampleSection, UiCorrectedGene, UiCount, UiFacts, UiGene,
};

pub struct ReportInputs<'a> {
    pub title: String,
    pub ds: &'a Dataset,
    pub profile: &'a PlatformProfile,
    pub record: &'a RunRecord,
    pub cc: &'a CellCallingOutput,
    pub embedding: &'a Embedding,
    pub ambient: Option<&'a ambient::AmbientOutput>,
    pub ambient_skipped_reason: String,
    pub ddqc: &'a DdqcResult,
    pub doublet: &'a DoubletOutput,
    pub n_loaded: usize,
    pub rate_per_1000: f64,
    pub corrected: Option<&'a CorrectedMatrix>,
    pub correction_forced: bool,
    /// 最终保留细胞的基础指标
    pub final_metrics: &'a BasicMetrics,
    pub n_after_ddqc: usize,
    pub n_after_doublet: usize,
    pub n_final: usize,
    pub n_removed_by_step: Vec<(String, usize)>,
    pub warnings: Vec<(String, String)>,
    pub elapsed_s: f64,
    pub params_json: String,
}

/// 非有限值统一写 `n/a`：它在三种语言的科研写作里都通用，
/// 比翻译出来的"不可用/該当なし"更不容易和真正的数值混淆。
/// 整数加千位分隔符（报告里的计数都走它；读者看的是数量级，没有分隔符很难读）。
fn n0(v: usize) -> String {
    let s = v.to_string();
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in b.iter().enumerate() {
        if i > 0 && (b.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*c as char);
    }
    out
}

fn pct(v: f64) -> String {
    if v.is_finite() {
        format!("{:.2}%", v * 100.0)
    } else {
        "n/a".into()
    }
}

fn f1(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.1}")
    } else {
        "n/a".into()
    }
}

fn f0(v: f64) -> String {
    if v.is_finite() {
        n0(v.round().max(0.0) as usize)
    } else {
        "n/a".into()
    }
}

fn median_u64(v: &[u64]) -> f64 {
    let mut s: Vec<u64> = v.to_vec();
    s.sort_unstable();
    if s.is_empty() {
        return f64::NAN;
    }
    let n = s.len();
    if n % 2 == 1 {
        s[n / 2] as f64
    } else {
        (s[n / 2 - 1] + s[n / 2]) as f64 / 2.0
    }
}

fn median_f64(v: &[f64]) -> f64 {
    let mut s: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if s.is_empty() {
        return f64::NAN;
    }
    let n = s.len();
    if n % 2 == 1 {
        s[n / 2]
    } else {
        (s[n / 2 - 1] + s[n / 2]) / 2.0
    }
}

/// 分歧原因（只在判为分歧时给出）。算法侧只给判定，文字在报告层按语言组织。
fn disagreement_reasons(c: &ambient::Consensus, lang: Lang) -> Vec<String> {
    if c.verdict != Verdict::Disagree {
        return Vec::new();
    }
    vec![
        M::AmbReasonA.t(lang).to_string(),
        M::AmbReasonB.t(lang).to_string(),
        M::AmbReasonC.t(lang).to_string(),
    ]
}

/// 校正建议的理由。与 `ambient::consensus` 的判定逻辑一一对应。
fn correction_reason(c: &ambient::Consensus, lang: Lang) -> String {
    let both_low = matches!((c.rho_soup, c.rho_decontx), (Some(x), Some(y))
        if x.max(y) < ambient::consensus::LOW_ABS);
    match (c.verdict, c.rating) {
        (Verdict::Unavailable, _) => M::AmbWhyUnavailable.t(lang).to_string(),
        (Verdict::Disagree, _) => M::AmbWhyDisagree.f(
            lang,
            &[&format!("{:.0}%", c.relative_diff.unwrap_or(0.0) * 100.0)],
        ),
        (_, Rating::Unrated) if both_low => M::AmbWhyBothLow.f(
            lang,
            &[&format!("{:.0}%", ambient::consensus::LOW_ABS * 100.0)],
        ),
        (_, Rating::Unrated) => M::AmbWhyUnrated.t(lang).to_string(),
        (_, Rating::Severe) => M::AmbWhySevere.t(lang).to_string(),
        (_, Rating::Elevated) => M::AmbWhyElevated.t(lang).to_string(),
        (_, Rating::Normal) => M::AmbWhyNormal.t(lang).to_string(),
        (_, Rating::Unavailable) => M::AmbWhyNoEstimate.t(lang).to_string(),
    }
}

/// 校正方法名。
fn correction_method_label(m: scqc_core::ambient::CorrectionMethod, lang: Lang) -> String {
    use scqc_core::ambient::CorrectionMethod as C;
    match m {
        C::Soup => M::CorrMethodSoup,
        C::DecontX => M::CorrMethodDecontx,
    }
    .t(lang)
    .to_string()
}

/// 平台的第一版限制说明。与 `platform::builtin` 的取值一一对应，但文字在报告层按语言给出。
fn platform_caveats(id: scqc_core::platform::ProfileId, lang: Lang) -> Vec<String> {
    use scqc_core::platform::ProfileId as P;
    let msgs: &[M] = match id {
        P::TenxScRna => &[],
        P::TenxSnRna => &[M::CaveatSnLower, M::CaveatSnMito],
        P::TenxFlexSingleplex => &[
            M::CaveatFlexCellCalling,
            M::CaveatFlexMito,
            M::CaveatFlexNoRef,
            M::CaveatFlexProbeSet,
        ],
        P::TenxFlexMultiplex => &[
            M::CaveatFlexCellCalling,
            M::CaveatFlexMito,
            M::CaveatFlexNoRef,
            M::CaveatFlexProbeSet,
            M::CaveatFlexMultiplexRate,
        ],
    };
    msgs.iter().map(|m| m.t(lang).to_string()).collect()
}

/// 不显示核糖体比例的原因（目前只有 Flex 会触发）。
fn ribo_hidden_reason(id: scqc_core::platform::ProfileId, lang: Lang) -> Option<String> {
    use scqc_core::platform::ProfileId as P;
    match id {
        P::TenxFlexSingleplex | P::TenxFlexMultiplex => Some(M::RiboHiddenFlex.t(lang).to_string()),
        _ => None,
    }
}

/// 平台 id → 该语言下的名字。报告与界面各有一套词典，这里是报告的那套。
fn platform_label(id: scqc_core::platform::ProfileId, lang: Lang) -> String {
    use scqc_core::platform::ProfileId as P;
    match id {
        P::TenxScRna => M::PlatformScRna,
        P::TenxSnRna => M::PlatformSnRna,
        P::TenxFlexSingleplex => M::PlatformFlexSingle,
        P::TenxFlexMultiplex => M::PlatformFlexMulti,
    }
    .t(lang)
    .to_string()
}

fn species_label(sp: &scqc_core::platform::Species, lang: Lang) -> String {
    use scqc_core::platform::Species as S;
    match sp {
        S::Human => M::SpeciesHuman,
        S::Mouse => M::SpeciesMouse,
        S::Other { .. } => M::SpeciesOther,
    }
    .t(lang)
    .to_string()
}

fn dim_label(d: DdqcDim, lang: Lang) -> &'static str {
    match d {
        DdqcDim::Counts => M::DimCounts.t(lang),
        DdqcDim::Genes => M::DimGenes.t(lang),
        DdqcDim::Mito => M::DimMito.t(lang),
        DdqcDim::Ribo => M::DimRibo.t(lang),
    }
}

fn verdict_label(v: Verdict, lang: Lang) -> String {
    M::AmbVerdictLine.f(
        lang,
        &[match v {
            Verdict::Agree => M::VerdictAgree.t(lang),
            Verdict::Disagree => M::VerdictDisagree.t(lang),
            Verdict::Single => M::VerdictSingle.t(lang),
            Verdict::Unavailable => M::VerdictUnavailable.t(lang),
        }],
    )
}

fn rating_word(r: Rating, lang: Lang) -> &'static str {
    match r {
        Rating::Normal => M::RatingNormal.t(lang),
        Rating::Elevated => M::RatingElevated.t(lang),
        Rating::Severe => M::RatingSevere.t(lang),
        Rating::Unrated => M::RatingUnrated.t(lang),
        Rating::Unavailable => M::RatingUnavailable.t(lang),
    }
}

fn correction_word(c: CorrectionState, lang: Lang) -> &'static str {
    match c {
        CorrectionState::Greyed => M::CorrGreyed.t(lang),
        CorrectionState::Available => M::CorrAvailable.t(lang),
        CorrectionState::Discouraged => M::CorrDiscouraged.t(lang),
        CorrectionState::Unrated => M::CorrUnrated.t(lang),
    }
}

pub fn build(inp: &ReportInputs<'_>, lang: Lang) -> ReportData {
    let ds = inp.ds;
    let profile = inp.profile;
    let features = &ds.filtered.features;

    // ---- 1. 样本 ----
    // 输入文件：一行一个目录。逐文件的大小与 SHA-256 写在 run_record.json 里——
    // 报告的读者关心结果，不会逐个核对校验和；要核对的人去 run_record.json。
    let mut inputs = Vec::new();
    for (role, dir) in [("raw", ds.raw.as_ref()), ("filtered", Some(&ds.filtered))] {
        if let Some(d) = dir {
            let names = d
                .files
                .iter()
                .map(|f| f.name.clone())
                .collect::<Vec<_>>()
                .join(M::ListSep.t(lang));
            inputs.push(KeyValue::new(
                M::InputFile.f(lang, &[role]),
                M::InputDirLine.f(lang, &[&names, &d.files.len().to_string()]),
            ));
        }
    }
    if let Some(al) = &ds.feature_alignment {
        inputs.push(KeyValue::new(
            M::FeatureAlignment.t(lang),
            M::AlignNote.f(
                lang,
                &[
                    &al.n_raw.to_string(),
                    &al.n_filtered.to_string(),
                    &al.n_common.to_string(),
                    &al.raw_dropped_umi.to_string(),
                    &pct(al.raw_dropped_umi_fraction),
                ],
            ),
        ));
    }
    if let Some(ng) = &ds.non_gex {
        let types = ng
            .types
            .iter()
            .map(|(t, n)| format!("{t} × {n}"))
            .collect::<Vec<_>>()
            .join("; ");
        inputs.push(KeyValue::new(
            M::NonGexFeatures.t(lang),
            M::NonGexNote.f(
                lang,
                &[
                    &ng.n_features_dropped.to_string(),
                    &types,
                    &pct(ng.umi_fraction_filtered),
                    &pct(ng.umi_fraction_raw),
                ],
            ),
        ));
    }
    let sample = SampleSection {
        platform_id: profile.id.as_str().to_string(),
        platform_label: platform_label(profile.id, lang),
        evidence: profile
            .evidence_codes
            .iter()
            .zip(profile.evidence.iter())
            .map(|(c, zh)| scqc_core::report::i18n::evidence_text(&c.code, &c.args, lang, zh))
            .collect(),
        user_overrides: profile
            .user_overrides
            .iter()
            .map(|c| scqc_core::report::i18n::override_text(&c.code, &c.args, lang))
            .collect(),
        species: species_label(&profile.species, lang),
        multiplex_n: profile.multiplex_n,
        inputs,
        started_at_utc: inp.record.started_at_utc.clone(),
        version: inp.record.scqc_version.clone(),
        git_hash: inp.record.git_hash.clone(),
        n_raw_barcodes: ds.raw.as_ref().map(|r| r.matrix.n_barcodes()),
        n_cellranger_cells: ds.filtered.matrix.n_barcodes(),
        n_genes: ds.filtered.matrix.n_genes(),
        seed: inp.record.params.seed,
        elapsed_s: f1(inp.elapsed_s),
        caveats_platform: platform_caveats(profile.id, lang),
    };

    // ---- 2. 概览 ----
    let m = inp.final_metrics;
    let umi: Vec<f64> = m.total_umi.iter().map(|&v| v as f64).collect();
    let genes: Vec<f64> = m.n_genes.iter().map(|&v| v as f64).collect();
    let (mito_word, mito_note) = match profile.mito_semantics {
        MitoSemantics::Viability => (M::MitoViability, M::MitoViabilityNote),
        MitoSemantics::StrippingQuality => (M::MitoStripping, M::MitoStrippingNote),
        MitoSemantics::Dampened => (M::MitoDampened, M::MitoDampenedNote),
    };
    let mito_title = M::MitoHeading.f(lang, &[mito_word.t(lang)]);
    let mito_note = mito_note.t(lang).to_string();
    let mut stats = vec![
        KeyValue::new(M::StatFinalCells.t(lang), n0(m.total_umi.len())),
        KeyValue::new(M::StatMedianUmi.t(lang), f0(median_u64(&m.total_umi))),
        KeyValue::new(
            M::StatMedianGenes.t(lang),
            f0(median_u64(
                &m.n_genes.iter().map(|&v| v as u64).collect::<Vec<_>>(),
            )),
        ),
        KeyValue::new(M::StatMedianMito.t(lang), pct(median_f64(&m.pct_mito))),
    ];
    // 平台不用核糖体这个指标时整行不出现；为什么不用，第 4 节的注记里说了
    if profile.show_ribo {
        stats.push(KeyValue::new(
            M::StatMedianRibo.t(lang),
            pct(median_f64(&m.pct_ribo)),
        ));
    }
    // 直方图右上角的"每箱最多多少细胞"标注：SVG 内部算出峰值，这里只提供模板
    let bin_label = M::SvgMaxPerBin.t(lang).to_string();
    let overview = OverviewSection {
        knee_svg: svg::knee_plot(
            &inp.cc.knee.curve_rank,
            &inp.cc.knee.curve_total,
            inp.cc.knee.knee_total,
            inp.cc.knee.inflection_total,
            inp.cc.knee.lower,
            M::SvgBarcodeRank.t(lang),
            M::SvgTotalUmi.t(lang),
        ),
        umi_hist_svg: svg::histogram(&umi, true, M::HistUmi.t(lang), "#1f77b4", &bin_label),
        genes_hist_svg: svg::histogram(&genes, true, M::HistGenes.t(lang), "#2ca02c", &bin_label),
        mito_hist_svg: svg::histogram(
            &m.pct_mito,
            false,
            M::HistMito.t(lang),
            "#d62728",
            &bin_label,
        ),
        ribo_hist_svg: if profile.show_ribo {
            Some(svg::histogram(
                &m.pct_ribo,
                false,
                M::HistRibo.t(lang),
                "#9467bd",
                &bin_label,
            ))
        } else {
            None
        },
        stats,
        mito_title,
        mito_note,
        ribo_hidden_reason: ribo_hidden_reason(profile.id, lang),
    };

    // ---- 3. 真细胞鉴定 ----
    let s = &inp.cc.summary;
    let (mode_msg, note_msg) = match s.mode {
        CellCallingMode::EmptyDrops => (M::CcEmptyDrops, M::CcNoteEmptyDrops),
        CellCallingMode::TrustUpstream => (M::CcTrustUpstream, M::CcNoteTrust),
        CellCallingMode::RawMissing => (M::CcRawMissing, M::CcNoteTrust),
    };
    let mut cc_items = vec![
        KeyValue::new(M::CcKnee.t(lang), n0(s.knee_total as usize)),
        KeyValue::new(M::CcInflection.t(lang), n0(s.inflection_total as usize)),
    ];
    // lower / Dirichlet α / 候选 barcode 数是算法内部量，读者不关心，也不该在报告里解释；
    // 要核对的人去 cell_calling.json。
    cc_items.push(KeyValue::new(
        M::CellRangerCells.t(lang),
        n0(s.n_cellranger_cells),
    ));
    cc_items.push(KeyValue::new(M::CcCalled.t(lang), n0(s.n_called)));
    cc_items.push(KeyValue::new(M::CcRescued.t(lang), n0(s.n_rescued)));
    cc_items.push(KeyValue::new(M::CcLost.t(lang), n0(s.n_lost)));
    let cell_calling = CellCallingSection {
        mode: mode_msg.t(lang).to_string(),
        items: cc_items,
        n_rescued: s.n_rescued,
        rescued_median_genes: f0(s.rescued_median_genes),
        rescued_top_genes: s.rescued_top_genes.clone(),
        n_lost: s.n_lost,
        note: note_msg.t(lang).to_string(),
    };

    // ---- 4. 环境 RNA ----
    let ambient_sec = match inp.ambient {
        None => AmbientSection {
            available: false,
            skipped_reason: inp.ambient_skipped_reason.clone(),
            ..Default::default()
        },
        Some(ao) => {
            let est = &ao.estimates;
            let c = &est.consensus;
            let mut rows = Vec::new();
            if let Some(sp) = &est.soup {
                rows.push(EstimateRow {
                    name: if sp.at_boundary {
                        M::AmbSoupUnreliable.t(lang).to_string()
                    } else {
                        M::AmbSoupName.t(lang).to_string()
                    },
                    value: pct(sp.rho_global),
                    detail: M::AmbSoupDetail.f(
                        lang,
                        &[
                            &pct(sp.rho_low),
                            &pct(sp.rho_high),
                            &sp.n_markers.to_string(),
                            &sp.n_pairs_used.to_string(),
                            &sp.n_pairs_excluded.to_string(),
                        ],
                    ),
                });
            } else if profile
                .ambient_estimators
                .contains(&scqc_core::platform::AmbientEstimator::Soup)
            {
                rows.push(EstimateRow {
                    name: M::AmbSoupName.t(lang).to_string(),
                    value: M::NotAvailable.t(lang).to_string(),
                    detail: String::new(),
                });
            }
            if let Some(dx) = &est.decontx {
                rows.push(EstimateRow {
                    name: M::AmbDecontxName.t(lang).to_string(),
                    value: pct(dx.rho_global),
                    detail: M::AmbDecontxDetail.f(
                        lang,
                        &[
                            &pct(dx.rho_median),
                            &pct(dx.rho_min),
                            &pct(dx.rho_max),
                            &dx.n_iter.to_string(),
                            if dx.converged {
                                M::AmbConverged.t(lang)
                            } else {
                                M::AmbNotConverged.t(lang)
                            },
                        ],
                    ),
                });
            }
            if let Some(mt) = &est.mito {
                rows.push(EstimateRow {
                    name: M::AmbMitoName.t(lang).to_string(),
                    value: if mt.usable {
                        pct(mt.rho_global)
                    } else {
                        M::NotAvailable.t(lang).to_string()
                    },
                    detail: if mt.usable {
                        M::AmbMitoDetail.f(lang, &[&pct(mt.a_mito)])
                    } else {
                        M::AmbMitoUnusable.t(lang).to_string()
                    },
                });
            }
            let reference = c
                .reference
                .map(|r| {
                    vec![
                        KeyValue::new(M::AmbRefMean.t(lang), pct(r.mean)),
                        KeyValue::new(M::AmbRefGrey.t(lang), pct(r.grey_below)),
                        KeyValue::new(M::AmbRefDiscourage.t(lang), pct(r.discourage_above)),
                        KeyValue::new(
                            M::AmbThisSample.t(lang),
                            format!(
                                "{} → {}",
                                c.interval
                                    .map(|(a, b)| format!("{}–{}", pct(a), pct(b)))
                                    .unwrap_or_else(|| M::NotAvailable.t(lang).to_string()),
                                rating_word(c.rating, lang)
                            ),
                        ),
                    ]
                })
                .unwrap_or_default();
            let severe = c.rating == Rating::Severe;
            let severe_text = if severe {
                let mean_ref = c
                    .reference
                    .map(|r| pct(r.mean))
                    .unwrap_or_else(|| "—".into());
                vec![
                    M::AmbSevere1.f(
                        lang,
                        &[
                            &c.interval
                                .map(|(a, b)| format!("{}–{}", pct(a), pct(b)))
                                .unwrap_or_default(),
                            &mean_ref,
                        ],
                    ),
                    M::AmbSevere2.t(lang).to_string(),
                    M::AmbSevere3.t(lang).to_string(),
                    M::AmbSevere4.t(lang).to_string(),
                ]
            } else {
                Vec::new()
            };
            let top_genes = est
                .top_affected_genes
                .iter()
                .take(8)
                .map(|g| GeneRow {
                    gene: g.gene.clone(),
                    a: f0(g.observed_counts),
                    b: f0(g.expected_ambient_counts),
                    c: pct(g.ambient_fraction),
                })
                .collect();
            let correction = inp.corrected.map(|cm| {
                let sm = &cm.summary;
                CorrectionSection {
                    method: correction_method_label(sm.method, lang),
                    forced: inp.correction_forced,
                    items: vec![
                        KeyValue::new(
                            M::AmbCorrMethod.t(lang),
                            correction_method_label(sm.method, lang),
                        ),
                        KeyValue::new(
                            M::AmbCorrRho.t(lang),
                            M::AmbCorrRhoValue.f(
                                lang,
                                &[&pct(sm.rho_mean), &pct(sm.rho_min), &pct(sm.rho_max)],
                            ),
                        ),
                        KeyValue::new(
                            M::AmbCorrTotals.t(lang),
                            M::AmbCorrTotalsValue.f(
                                lang,
                                &[
                                    &f0(sm.total_before),
                                    &f0(sm.total_after),
                                    &pct(sm.removed_fraction),
                                ],
                            ),
                        ),
                        KeyValue::new(M::AmbCorrCells.t(lang), sm.n_cells.to_string()),
                        KeyValue::new(
                            M::AmbCorrShortfall.t(lang),
                            sm.n_cells_shortfall.to_string(),
                        ),
                        KeyValue::new(
                            M::AmbCorrOutput.t(lang),
                            M::AmbCorrOutputValue.f(
                                lang,
                                &[if sm.integer_output {
                                    M::AmbCorrInteger.t(lang)
                                } else {
                                    M::AmbCorrReal.t(lang)
                                }],
                            ),
                        ),
                    ],
                    top_genes: sm
                        .top_genes
                        .iter()
                        .map(|g| GeneRow {
                            gene: features.name.get(g.gene_index).cloned().unwrap_or_default(),
                            a: f0(g.before),
                            b: f0(g.after),
                            c: pct(g.removed_fraction),
                        })
                        .collect(),
                    per_cluster: sm
                        .per_cluster
                        .iter()
                        .map(|k| GeneRow {
                            gene: format!("{} {}", M::AmbCorrCluster.t(lang), k.cluster),
                            a: format!("{} / {}", k.n_cells, f0(k.before)),
                            b: f0(k.after),
                            c: pct(k.removed_fraction),
                        })
                        .collect(),
                    caveat: M::AmbCaveat.t(lang).to_string(),
                }
            });
            AmbientSection {
                available: true,
                skipped_reason: String::new(),
                estimates: rows,
                verdict: verdict_label(c.verdict, lang),
                rating: M::AmbRatingLine.f(lang, &[rating_word(c.rating, lang)]),
                correction_state: M::AmbCorrectionLine
                    .f(lang, &[correction_word(c.correction, lang)]),
                reason: correction_reason(c, lang),
                relative_diff: M::AmbRelativeDiff.f(
                    lang,
                    &[
                        &c.relative_diff
                            .map(|d| format!("{:.0}%", d * 100.0))
                            .unwrap_or_else(|| "—".into()),
                        &c.interval
                            .map(|(a, b)| format!("{}–{}", pct(a), pct(b)))
                            .unwrap_or_else(|| "—".into()),
                    ],
                ),
                interval: c
                    .interval
                    .map(|(a, b)| format!("{}–{}", pct(a), pct(b)))
                    .unwrap_or_else(|| "—".into()),
                reference,
                reference_note: if c.reference.is_none() {
                    M::AmbNoReference.t(lang).to_string()
                } else {
                    String::new()
                },
                disagree: c.verdict == Verdict::Disagree,
                // 分歧原因在报告里按语言重新组织：算法侧只给结构化的判定结果
                disagreement_reasons: disagreement_reasons(c, lang),
                severe,
                severe_text,
                top_genes,
                correction,
            }
        }
    };

    // ---- 5. ddqc ----
    let dq = inp.ddqc;
    // 只留核心：进来多少、删了多少、剩多少，再加一行"换成固定阈值会删多少"作为对照。
    // 粗过滤门槛、MAD 倍数、逐簇阈值都是参数与中间量，在 run_params.json / ddqc.json 里。
    let ddqc_items = vec![
        KeyValue::new(M::DdqcInput.t(lang), n0(dq.n_cells)),
        KeyValue::new(M::DdqcRemovedTotal.t(lang), n0(dq.n_removed_total)),
        KeyValue::new(M::DdqcKept.t(lang), n0(dq.n_cells - dq.n_removed_total)),
        KeyValue::new(M::DdqcFixedAlone.t(lang), n0(dq.fixed_n_removed)),
    ];
    // 只保留"为什么少了一个指标"这一条：它影响读者怎么看结果。
    // 护栏取值、MAD 最小差这些是参数，在 run_params.json 与第 6 节的平台说明里。
    let mut notes = Vec::new();
    if !profile.uses_ribo() {
        notes.push(M::DdqcNoteRiboDisabled.f(
            lang,
            &[&ribo_hidden_reason(profile.id, lang).unwrap_or_else(|| "-".into())],
        ));
    }
    let thresholds = dq
        .thresholds
        .iter()
        .map(|t| {
            let fmt = |v: f64| match t.dim {
                DdqcDim::Mito | DdqcDim::Ribo => pct(v),
                // 下限维度的阈值可能为负（中位数 − 2×MAD < 0），此时不会剔除任何细胞
                _ if v < 0.0 => M::DdqcInactive.f(lang, &[&f0(v)]),
                _ => f0(v),
            };
            GeneRow {
                gene: M::DdqcClusterLabel
                    .f(lang, &[&t.cluster.to_string(), dim_label(t.dim, lang)]),
                a: format!("{} / {}", t.n_cells, fmt(t.median)),
                b: format!("{} / {}", fmt(t.mad), fmt(t.threshold)),
                c: t.n_removed.to_string(),
            }
        })
        .collect();
    let ddqc_sec = DdqcSection {
        items: ddqc_items,
        removed_by_dim: dq
            .removed_by_dim
            .iter()
            .map(|(d, n)| KeyValue::new(dim_label(*d, lang), n.to_string()))
            .collect(),
        dims: profile
            .ddqc_dimensions
            .iter()
            .map(|d| dim_label(*d, lang).to_string())
            .collect(),
        notes,
        thresholds,
    };

    // ---- 6. 双细胞 ----
    let db = &inp.doublet.summary;
    let per_sample = db
        .per_sample
        .iter()
        .map(|s| GeneRow {
            gene: s.label.clone(),
            a: format!("{} / {}", s.n_cells, s.n_expected),
            b: format!("{} / {}", s.cxds_n_called, s.df_n_called),
            c: format!(
                "{} / {} / {}",
                s.n_high_confidence,
                s.n_borderline,
                if s.df_p_k.is_finite() {
                    s.df_p_k.to_string()
                } else {
                    "—".into()
                }
            ),
        })
        .collect();
    let pk_txt = db
        .per_sample
        .iter()
        .filter(|s| s.df_p_k.is_finite())
        .map(|s| s.df_p_k.to_string())
        .collect::<Vec<_>>()
        .join(" / ");
    // 两个方法各判了多少、重合多少，压成一行；pK、Jaccard、逐样本明细在 doublet.json 里。
    let mut dbl_items = vec![
        KeyValue::new(M::DblCells.t(lang), n0(db.n_cells)),
        KeyValue::new(M::DblExpectedRate.t(lang), pct(db.expected_rate)),
        KeyValue::new(
            M::DblByMethod.t(lang),
            M::DblByMethodValue.f(
                lang,
                &[
                    &n0(db.cxds_n_called),
                    &n0(db.df_n_called),
                    &n0(db.n_high_confidence),
                ],
            ),
        ),
        KeyValue::new(M::DblHigh.t(lang), n0(db.n_high_confidence)),
        KeyValue::new(M::DblBorderline.t(lang), n0(db.n_borderline)),
        KeyValue::new(
            M::DblFinalRate.t(lang),
            pct(db.n_high_confidence as f64 / db.n_cells.max(1) as f64),
        ),
    ];
    // 只有多重化样本才需要这一行
    if db.n_samples > 1 {
        dbl_items.insert(
            1,
            KeyValue::new(M::DblSamples.t(lang), db.n_samples.to_string()),
        );
    }
    let _ = &pk_txt;
    let doublet_sec = DoubletSection {
        items: dbl_items,
        formula: M::DblFormula.f(
            lang,
            &[
                &inp.rate_per_1000.to_string(),
                &inp.n_loaded.to_string(),
                &profile.multiplex_n.unwrap_or(1).to_string(),
                &pct(db.expected_rate),
            ],
        ),
        assumption: M::DblAssumption.t(lang).to_string(),
        per_sample,
        borderline_action: if inp.record.params.doublet.remove_borderline {
            M::DblBorderlineRemoved.t(lang).to_string()
        } else {
            M::DblBorderlineKept.t(lang).to_string()
        },
    };

    // ---- 7. 最终结果 ----
    let mut wf: Vec<(String, usize)> = Vec::new();
    // 与 wf 一一对应的稳定 id，供界面本地化（语言中立事实块）
    let mut wf_ids: Vec<(&'static str, usize)> = Vec::new();
    if let Some(r) = ds.raw.as_ref() {
        wf.push((M::WfRawBarcodes.t(lang).to_string(), r.matrix.n_barcodes()));
        wf_ids.push(("raw_barcodes", r.matrix.n_barcodes()));
    }
    wf.push((
        M::WfCellRanger.t(lang).to_string(),
        ds.filtered.matrix.n_barcodes(),
    ));
    wf_ids.push(("cellranger_cells", ds.filtered.matrix.n_barcodes()));
    wf.push((M::WfScqc.t(lang).to_string(), inp.cc.summary.n_called));
    wf_ids.push(("scqc_cells", inp.cc.summary.n_called));
    wf.push((M::WfAfterDdqc.t(lang).to_string(), inp.n_after_ddqc));
    wf_ids.push(("after_ddqc", inp.n_after_ddqc));
    wf.push((M::WfAfterDoublet.t(lang).to_string(), inp.n_after_doublet));
    wf_ids.push(("after_doublet", inp.n_after_doublet));
    if inp.n_final != inp.n_after_doublet || wf.is_empty() {
        wf.push((M::WfFinal.t(lang).to_string(), inp.n_final));
        wf_ids.push(("final", inp.n_final));
    } else if let Some(last) = wf.last_mut() {
        last.0 = M::WfAfterDoubletFinal.t(lang).to_string();
        if let Some(l) = wf_ids.last_mut() {
            l.0 = "after_doublet_final";
        }
    }
    let waterfall_svg = svg::waterfall(&wf);
    let final_sec = FinalSection {
        waterfall: wf.iter().map(|(k, v)| KeyValue::new(k, n0(*v))).collect(),
        waterfall_svg,
        items: vec![
            KeyValue::new(
                M::FinalRatio.t(lang),
                M::FinalRatioValue.f(
                    lang,
                    &[
                        &n0(inp.n_final),
                        &n0(ds.filtered.matrix.n_barcodes()),
                        &pct(inp.n_final as f64 / ds.filtered.matrix.n_barcodes().max(1) as f64),
                    ],
                ),
            ),
            KeyValue::new(M::FinalOutputs.t(lang), M::FinalOutputsValue.t(lang)),
        ],
        removed_by_step: inp
            .n_removed_by_step
            .iter()
            .enumerate()
            .map(|(i, (_, v))| {
                let label = match i {
                    0 => M::StepCellCalling,
                    1 => M::StepDdqc,
                    _ => M::StepDoublet,
                };
                KeyValue::new(label.t(lang), v.to_string())
            })
            .collect(),
    };

    // ---- 8. 方法学附录（填参数）----
    let p = &inp.record.params;
    let mut methods = methods::entries(lang);
    let mut fill: HashMap<&str, Vec<(String, String)>> = HashMap::new();
    fill.insert(
        "emptydrops",
        vec![
            (
                "lower".into(),
                p.cell_calling
                    .lower
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| M::ParamAuto100.t(lang).to_string()),
            ),
            ("niters".into(), p.cell_calling.niters.to_string()),
            ("FDR".into(), p.cell_calling.fdr.to_string()),
            (M::ParamMode.t(lang).into(), mode_msg.t(lang).to_string()),
        ],
    );
    fill.insert(
        "embed",
        vec![
            (M::ParamHvg.t(lang).into(), p.embed.n_hvg.to_string()),
            (M::ParamPcs.t(lang).into(), p.embed.n_pcs.to_string()),
            (M::ParamK.t(lang).into(), p.embed.k.to_string()),
            (
                M::ParamRes.t(lang).into(),
                format!("{} / {}", p.embed.res_fine, p.embed.res_coarse),
            ),
            (
                M::ParamActualClusters.t(lang).into(),
                format!(
                    "{} / {}",
                    inp.embedding.fine.n_communities, inp.embedding.coarse.n_communities
                ),
            ),
        ],
    );
    fill.insert(
        "soup",
        vec![
            (
                M::ParamAmbientUpper.t(lang).into(),
                p.ambient
                    .umi_upper_bound
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| M::ParamSameAsLower.t(lang).to_string()),
            ),
            (
                M::ParamTfidf.t(lang).into(),
                M::ParamTfidfValue.t(lang).into(),
            ),
            (
                M::ParamPrior.t(lang).into(),
                M::ParamPriorValue.t(lang).into(),
            ),
        ],
    );
    fill.insert(
        "decontx",
        vec![
            (
                M::ParamMaxIter.t(lang).into(),
                format!(
                    "{} / |ΔLL| < {}",
                    p.ambient.decontx_max_iter, p.ambient.decontx_tol
                ),
            ),
            (
                M::ParamClustering.t(lang).into(),
                M::ParamCoarseClusters.t(lang).into(),
            ),
        ],
    );
    fill.insert(
        "consensus",
        vec![(
            M::ParamDisagreeRule.t(lang).into(),
            M::ParamDisagreeValue.t(lang).into(),
        )],
    );
    fill.insert(
        "correct",
        vec![
            (M::ParamMethod.t(lang).into(), p.correction.method.clone()),
            (M::ParamForce.t(lang).into(), p.correction.force.to_string()),
        ],
    );
    fill.insert(
        "ddqc",
        vec![
            (M::DdqcMad.t(lang).into(), p.ddqc.mad_multiplier.to_string()),
            (
                M::ParamCoarseFilter.t(lang).into(),
                M::ParamCoarseFilterValue.f(
                    lang,
                    &[
                        &p.ddqc.coarse_min_genes.to_string(),
                        &pct(p.ddqc.coarse_max_mito),
                    ],
                ),
            ),
            (
                M::ParamDims.t(lang).into(),
                profile
                    .ddqc_dimensions
                    .iter()
                    .map(|d| dim_label(*d, lang))
                    .collect::<Vec<_>>()
                    .join("; "),
            ),
            (
                M::ParamMitoGuardrail.t(lang).into(),
                profile
                    .mito_guardrail
                    .map(pct)
                    .unwrap_or_else(|| M::NoneValue.t(lang).to_string()),
            ),
        ],
    );
    fill.insert(
        "cxds",
        vec![
            ("ntop".into(), p.doublet.ntop.to_string()),
            ("binThresh".into(), "0".into()),
        ],
    );
    fill.insert(
        "doubletfinder",
        vec![
            ("pN".into(), p.doublet.p_n.to_string()),
            (
                M::ParamPk.t(lang).into(),
                db.per_sample
                    .iter()
                    .filter(|s| s.df_p_k.is_finite())
                    .map(|s| s.df_p_k.to_string())
                    .collect::<Vec<_>>()
                    .join(" / "),
            ),
            (M::ParamExpectedRate.t(lang).into(), pct(db.expected_rate)),
        ],
    );
    for m in methods.iter_mut() {
        if let Some(v) = fill.remove(m.key.as_str()) {
            m.params = v;
        }
    }

    // ---- 9. 语言中立事实块（供桌面界面三语渲染）----
    let dim_id = |d: DdqcDim| -> String {
        match d {
            DdqcDim::Counts => "counts",
            DdqcDim::Genes => "genes",
            DdqcDim::Mito => "mito",
            DdqcDim::Ribo => "ribo",
        }
        .to_string()
    };
    let ao = inp.ambient;
    let cons = ao.map(|a| &a.estimates.consensus);
    let soup = ao.and_then(|a| a.estimates.soup.as_ref());
    let dcx = ao.and_then(|a| a.estimates.decontx.as_ref());
    let mito_est = ao.and_then(|a| a.estimates.mito.as_ref());
    let ui = UiFacts {
        schema: 2,
        platform_id: profile.id.as_str().to_string(),
        species_id: match &profile.species {
            scqc_core::platform::Species::Human => "human".into(),
            scqc_core::platform::Species::Mouse => "mouse".into(),
            scqc_core::platform::Species::Other { .. } => "other".into(),
        },
        multiplex_n: profile.multiplex_n,
        n_raw_barcodes: ds.raw.as_ref().map(|r| r.matrix.n_barcodes()),
        n_cellranger_cells: ds.filtered.matrix.n_barcodes(),
        n_genes: ds.filtered.matrix.n_genes(),
        n_non_gex_excluded: ds
            .non_gex
            .as_ref()
            .map(|n| n.n_features_dropped)
            .unwrap_or(0),
        feature_aligned_n_common: ds.feature_alignment.as_ref().map(|a| a.n_common),
        mito_semantics: match profile.mito_semantics {
            MitoSemantics::Viability => "viability".into(),
            MitoSemantics::StrippingQuality => "stripping_quality".into(),
            MitoSemantics::Dampened => "dampened".into(),
        },
        show_ribo: profile.show_ribo,
        evidence: profile.evidence_codes.clone(),
        user_overrides: profile.user_overrides.clone(),
        seed: inp.record.params.seed,
        version: inp.record.scqc_version.clone(),
        elapsed_s: inp.elapsed_s,

        median_umi: median_u64(&m.total_umi),
        median_genes: median_u64(&m.n_genes.iter().map(|&v| v as u64).collect::<Vec<_>>()),
        median_mito: median_f64(&m.pct_mito),
        median_ribo: if profile.show_ribo {
            Some(median_f64(&m.pct_ribo))
        } else {
            None
        },

        cc_mode: match s.mode {
            CellCallingMode::EmptyDrops => "emptydrops".into(),
            CellCallingMode::TrustUpstream => "trust_upstream".into(),
            CellCallingMode::RawMissing => "raw_missing".into(),
        },
        cc_n_called: s.n_called,
        cc_n_rescued: s.n_rescued,
        cc_n_lost: s.n_lost,
        cc_knee: s.knee_total,
        cc_inflection: s.inflection_total,
        cc_lower: s.lower,
        cc_n_tested: s.n_tested,
        cc_fdr: p.cell_calling.fdr,
        cc_rescued_median_genes: s.rescued_median_genes,
        cc_rescued_top_genes: s.rescued_top_genes.clone(),

        ambient_available: ao.is_some(),
        rho_soup: soup.map(|x| x.rho_global),
        rho_soup_low: soup.map(|x| x.rho_low),
        rho_soup_high: soup.map(|x| x.rho_high),
        rho_soup_unreliable: soup.map(|x| x.at_boundary).unwrap_or(false),
        rho_decontx: dcx.map(|x| x.rho_global),
        decontx_converged: dcx.map(|x| x.converged),
        decontx_n_iter: dcx.map(|x| x.n_iter),
        rho_mito: mito_est.filter(|x| x.usable).map(|x| x.rho_global),
        ambient_relative_diff: cons.and_then(|c| c.relative_diff),
        ambient_verdict: cons
            .map(|c| match c.verdict {
                Verdict::Agree => "agree",
                Verdict::Disagree => "disagree",
                Verdict::Single => "single",
                Verdict::Unavailable => "unavailable",
            })
            .unwrap_or("unavailable")
            .to_string(),
        ambient_rating: cons
            .map(|c| match c.rating {
                Rating::Normal => "normal",
                Rating::Elevated => "elevated",
                Rating::Severe => "severe",
                Rating::Unrated => "unrated",
                Rating::Unavailable => "unavailable",
            })
            .unwrap_or("unavailable")
            .to_string(),
        ambient_correction_state: cons
            .map(|c| match c.correction {
                CorrectionState::Greyed => "greyed",
                CorrectionState::Available => "available",
                CorrectionState::Discouraged => "discouraged",
                CorrectionState::Unrated => "unrated",
            })
            .unwrap_or("greyed")
            .to_string(),
        ambient_reference_mean: cons.and_then(|c| c.reference).map(|r| r.mean),
        ambient_reference_grey_below: cons.and_then(|c| c.reference).map(|r| r.grey_below),
        ambient_reference_discourage_above: cons
            .and_then(|c| c.reference)
            .map(|r| r.discourage_above),
        ambient_top_genes: ao
            .map(|a| {
                a.estimates
                    .top_affected_genes
                    .iter()
                    .take(20)
                    .map(|g| UiGene {
                        gene: g.gene.clone(),
                        fraction: g.ambient_fraction,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        correction_applied: inp.corrected.map(|c| c.summary.method.as_str().to_string()),
        correction_removed_fraction: inp.corrected.map(|c| c.summary.removed_fraction),
        correction_forced: inp.correction_forced,
        correction_top_genes: inp
            .corrected
            .map(|c| {
                c.summary
                    .top_genes
                    .iter()
                    .take(8)
                    .map(|g| UiCorrectedGene {
                        gene: features.name.get(g.gene_index).cloned().unwrap_or_default(),
                        before: g.before,
                        after: g.after,
                        removed_fraction: g.removed_fraction,
                    })
                    .collect()
            })
            .unwrap_or_default(),

        ddqc_n_in: dq.n_cells,
        ddqc_n_removed_coarse: dq.n_removed_coarse,
        ddqc_n_removed_total: dq.n_removed_total,
        ddqc_removed_by_dim: dq
            .removed_by_dim
            .iter()
            .map(|(d, n)| UiCount {
                id: dim_id(*d),
                n: *n,
            })
            .collect(),
        ddqc_dims: profile.ddqc_dimensions.iter().map(|d| dim_id(*d)).collect(),
        ddqc_n_removed_by_fixed: dq.n_removed_by_fixed,
        ddqc_fixed_n_removed: dq.fixed_n_removed,
        ddqc_n_extra_kept: dq.n_extra_kept,
        ddqc_extra_kept_median_genes: dq.n_extra_kept_median_genes,
        ddqc_strict: dq.params.fixed_thresholds_are_hard,
        ddqc_mad_multiplier: dq.params.mad_multiplier,
        ddqc_fixed_min_genes: dq.params.fixed_min_genes,
        ddqc_fixed_max_mito: dq.params.fixed_max_mito,
        ddqc_n_clusters: inp.embedding.fine.n_communities,
        mito_guardrail: profile.mito_guardrail,

        dbl_n_in: db.n_cells,
        dbl_n_samples: db.n_samples,
        dbl_expected_rate: db.expected_rate,
        dbl_rate_per_1000: inp.rate_per_1000,
        dbl_n_loaded: inp.n_loaded,
        dbl_n_high_confidence: db.n_high_confidence,
        dbl_n_borderline: db.n_borderline,
        dbl_cxds_n: db.cxds_n_called,
        dbl_df_n: db.df_n_called,
        dbl_jaccard: db.agreement_jaccard,
        dbl_remove_borderline: p.doublet.remove_borderline,
        dbl_n_removed: inp.n_after_ddqc.saturating_sub(inp.n_after_doublet),

        waterfall: wf_ids
            .iter()
            .map(|(id, n)| UiCount {
                id: (*id).to_string(),
                n: *n,
            })
            .collect(),
        removed_by_step: vec![
            UiCount {
                id: "cell_calling".into(),
                n: inp.n_removed_by_step.first().map(|(_, v)| *v).unwrap_or(0),
            },
            UiCount {
                id: "ddqc".into(),
                n: dq.n_removed_total,
            },
            UiCount {
                id: "doublet".into(),
                n: inp.n_after_ddqc.saturating_sub(inp.n_after_doublet),
            },
        ],
        n_final: inp.n_final,
        warning_codes: inp.warnings.iter().map(|(c, _)| c.clone()).collect(),
    };

    ReportData {
        title: inp.title.clone(),
        sample,
        overview,
        cell_calling,
        ambient: ambient_sec,
        ddqc: ddqc_sec,
        doublet: doublet_sec,
        final_result: final_sec,
        methods,
        limitations_global: methods::global_limitations(lang),
        params_json: inp.params_json.clone(),
        ui,
        warnings: inp
            .warnings
            .iter()
            .map(|(c, m)| KeyValue::new(c, scqc_core::report::i18n::warning_text(c, lang, m)))
            .collect(),
    }
}
