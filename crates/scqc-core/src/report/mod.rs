//! HTML 报告（SPEC §12；DESIGN §5.16）。
//!
//! - 单文件 HTML，内联 CSS 与 SVG，**不引用任何外部资源**（断网可用）。
//! - 报告中每个数字都来自 [`ReportData`] 的一个字段；该结构同时写成 `report_data.json`，便于逐数核对。
//! - 章节严格对应 SPEC §12 的九节；方法学附录含每个算法的引用、参数与已知局限性。
//! - 数值在引擎侧预先格式化为字符串（模板只做排版），保证输出逐位可复现。

pub mod i18n;
pub mod methods;
pub mod svg;

use serde::{Deserialize, Serialize};

use crate::error::{Result, ScqcError};
pub use methods::MethodEntry;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
}

impl KeyValue {
    pub fn new(k: impl Into<String>, v: impl Into<String>) -> Self {
        Self {
            key: k.into(),
            value: v.into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SampleSection {
    pub platform_id: String,
    pub platform_label: String,
    pub evidence: Vec<String>,
    pub user_overrides: Vec<String>,
    pub species: String,
    pub multiplex_n: Option<u32>,
    pub inputs: Vec<KeyValue>,
    pub started_at_utc: String,
    pub version: String,
    pub git_hash: String,
    pub n_raw_barcodes: Option<usize>,
    pub n_cellranger_cells: usize,
    pub n_genes: usize,
    pub seed: u64,
    pub elapsed_s: String,
    pub caveats_platform: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OverviewSection {
    pub knee_svg: String,
    pub umi_hist_svg: String,
    pub genes_hist_svg: String,
    pub mito_hist_svg: String,
    pub ribo_hist_svg: Option<String>,
    pub stats: Vec<KeyValue>,
    /// 线粒体章节标题（细胞活性 / 核剥离质量 / 探针降采样）
    pub mito_title: String,
    pub mito_note: String,
    pub ribo_hidden_reason: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CellCallingSection {
    pub mode: String,
    pub items: Vec<KeyValue>,
    pub n_rescued: usize,
    pub rescued_median_genes: String,
    pub rescued_top_genes: Vec<String>,
    pub n_lost: usize,
    pub note: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EstimateRow {
    pub name: String,
    pub value: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GeneRow {
    pub gene: String,
    pub a: String,
    pub b: String,
    pub c: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CorrectionSection {
    pub method: String,
    pub forced: bool,
    pub items: Vec<KeyValue>,
    pub top_genes: Vec<GeneRow>,
    pub per_cluster: Vec<GeneRow>,
    pub caveat: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AmbientSection {
    pub available: bool,
    pub skipped_reason: String,
    pub estimates: Vec<EstimateRow>,
    pub verdict: String,
    pub rating: String,
    pub correction_state: String,
    pub reason: String,
    pub relative_diff: String,
    pub interval: String,
    pub reference: Vec<KeyValue>,
    pub reference_note: String,
    pub disagree: bool,
    pub disagreement_reasons: Vec<String>,
    pub severe: bool,
    pub severe_text: Vec<String>,
    pub top_genes: Vec<GeneRow>,
    pub correction: Option<CorrectionSection>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DdqcSection {
    pub items: Vec<KeyValue>,
    pub removed_by_dim: Vec<KeyValue>,
    pub dims: Vec<String>,
    pub notes: Vec<String>,
    pub thresholds: Vec<GeneRow>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DoubletSection {
    pub items: Vec<KeyValue>,
    pub formula: String,
    pub assumption: String,
    pub per_sample: Vec<GeneRow>,
    pub borderline_action: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FinalSection {
    pub waterfall: Vec<KeyValue>,
    pub waterfall_svg: String,
    pub items: Vec<KeyValue>,
    pub removed_by_step: Vec<KeyValue>,
}

/// 计数项：`id` 是稳定的机器标识（不随语言变化），界面自行套用本地化文案。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UiCount {
    pub id: String,
    pub n: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UiGene {
    pub gene: String,
    pub fraction: f64,
}

/// 校正前后对比的一行（SPEC §8.3.4.1）：界面在"环境 RNA"那一屏用它显示
/// "扣掉的到底是什么"。计数是这一批细胞在该基因上的合计。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UiCorrectedGene {
    pub gene: String,
    pub before: f64,
    pub after: f64,
    pub removed_fraction: f64,
}

/// **语言中立的事实块**（SPEC §10.5）。
///
/// 上面各节的字符串是为中文 HTML 报告排版好的成品；桌面界面要中／日／英三语，
/// 不能去解析那些中文串。因此引擎在这里再输出一份"只有数字和稳定枚举 id"的事实，
/// 界面读这里、用自己的词典渲染。新增字段一律带 `#[serde(default)]` 语义（结构整体
/// `Default`），旧版 `report_data.json` 仍可解析。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UiFacts {
    /// 事实块版本；界面据此判断能否渲染
    pub schema: u32,

    // ---- 样本 ----
    pub platform_id: String,
    pub species_id: String,
    pub multiplex_n: Option<u32>,
    pub n_raw_barcodes: Option<usize>,
    pub n_cellranger_cells: usize,
    pub n_genes: usize,
    /// 非基因表达特征被排除的个数（C10）
    pub n_non_gex_excluded: usize,
    /// raw/filtered 基因列表取交集后的共有基因数（未对齐时为 None）
    pub feature_aligned_n_common: Option<usize>,
    /// viability | stripping_quality | dampened
    pub mito_semantics: String,
    pub show_ribo: bool,
    /// 平台识别依据（语言中立：code + args）
    pub evidence: Vec<crate::platform::detect::CodedText>,
    /// 用户覆盖了哪些字段（语言中立：code + args）
    pub user_overrides: Vec<crate::platform::detect::CodedText>,
    pub seed: u64,
    pub version: String,
    pub elapsed_s: f64,

    // ---- 概览（最终保留细胞的中位数）----
    pub median_umi: f64,
    pub median_genes: f64,
    pub median_mito: f64,
    pub median_ribo: Option<f64>,

    // ---- 真细胞鉴定 ----
    /// emptydrops | trust_upstream | raw_missing
    pub cc_mode: String,
    pub cc_n_called: usize,
    pub cc_n_rescued: usize,
    pub cc_n_lost: usize,
    pub cc_knee: u64,
    pub cc_inflection: u64,
    pub cc_lower: Option<u32>,
    pub cc_n_tested: usize,
    pub cc_fdr: f64,
    pub cc_rescued_median_genes: f64,
    pub cc_rescued_top_genes: Vec<String>,

    // ---- 环境 RNA ----
    pub ambient_available: bool,
    pub rho_soup: Option<f64>,
    pub rho_soup_low: Option<f64>,
    pub rho_soup_high: Option<f64>,
    /// SoupX 式估计落在搜索区间端点：数值不可信，未参与判定
    pub rho_soup_unreliable: bool,
    pub rho_decontx: Option<f64>,
    pub decontx_converged: Option<bool>,
    pub decontx_n_iter: Option<usize>,
    pub rho_mito: Option<f64>,
    pub ambient_relative_diff: Option<f64>,
    /// agree | disagree | single | unavailable
    pub ambient_verdict: String,
    /// normal | elevated | severe | unrated | unavailable
    pub ambient_rating: String,
    /// greyed | available | discouraged | unrated
    pub ambient_correction_state: String,
    pub ambient_reference_mean: Option<f64>,
    pub ambient_reference_grey_below: Option<f64>,
    pub ambient_reference_discourage_above: Option<f64>,
    pub ambient_top_genes: Vec<UiGene>,
    /// 实际执行的校正：soup | decontx；未校正为 None
    pub correction_applied: Option<String>,
    pub correction_removed_fraction: Option<f64>,
    pub correction_forced: bool,
    /// 受校正影响最大的基因，前后计数对比；未校正时为空
    pub correction_top_genes: Vec<UiCorrectedGene>,

    // ---- 低质量细胞过滤 ----
    pub ddqc_n_in: usize,
    pub ddqc_n_removed_coarse: usize,
    pub ddqc_n_removed_total: usize,
    /// 维度 id（counts | genes | mito | ribo）→ 剔除数
    pub ddqc_removed_by_dim: Vec<UiCount>,
    pub ddqc_dims: Vec<String>,
    /// 严格模式下"自适应放过、固定阈值拦下"的细胞数
    pub ddqc_n_removed_by_fixed: usize,
    /// 单用固定阈值会剔除的细胞数（对照）
    pub ddqc_fixed_n_removed: usize,
    /// ddqc 比固定阈值多保留的细胞数
    pub ddqc_n_extra_kept: usize,
    pub ddqc_extra_kept_median_genes: f64,
    pub ddqc_strict: bool,
    pub ddqc_mad_multiplier: f64,
    pub ddqc_fixed_min_genes: u32,
    pub ddqc_fixed_max_mito: f64,
    pub ddqc_n_clusters: usize,
    pub mito_guardrail: Option<f64>,

    // ---- 双细胞 ----
    pub dbl_n_in: usize,
    pub dbl_n_samples: usize,
    pub dbl_expected_rate: f64,
    pub dbl_rate_per_1000: f64,
    pub dbl_n_loaded: usize,
    pub dbl_n_high_confidence: usize,
    pub dbl_n_borderline: usize,
    pub dbl_cxds_n: usize,
    pub dbl_df_n: usize,
    pub dbl_jaccard: f64,
    pub dbl_remove_borderline: bool,
    pub dbl_n_removed: usize,

    // ---- 最终 ----
    /// 步骤 id（raw_barcodes | cellranger_cells | scqc_cells | after_ddqc | after_doublet | final）
    pub waterfall: Vec<UiCount>,
    /// 步骤 id（cell_calling | ddqc | doublet）→ 剔除数
    pub removed_by_step: Vec<UiCount>,
    pub n_final: usize,
    /// 警告码（文案由界面本地化；中文全文仍在 `warnings`）
    pub warning_codes: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReportData {
    pub title: String,
    pub sample: SampleSection,
    pub overview: OverviewSection,
    pub cell_calling: CellCallingSection,
    pub ambient: AmbientSection,
    pub ddqc: DdqcSection,
    pub doublet: DoubletSection,
    pub final_result: FinalSection,
    pub methods: Vec<MethodEntry>,
    pub limitations_global: Vec<String>,
    pub params_json: String,
    pub warnings: Vec<KeyValue>,
    /// 语言中立的事实块，供桌面界面（中／日／英）渲染
    #[serde(default)]
    pub ui: UiFacts,
}

const TEMPLATE: &str = include_str!("template.html");
const SECTION: &str = include_str!("section.html");

/// 一份报告正文 + 它那一种语言的全部文案。
///
/// 报告按"英语 + 用户语言"输出两份完整正文（SPEC §12 · v2.3）：不是逐句穿插，
/// 而是两段完整的文档，可以单独把英文那一半发给国外合作者。
#[derive(Debug, Clone, Serialize)]
pub struct ReportDoc {
    pub lang: &'static str,
    pub html_lang: &'static str,
    pub endonym: &'static str,
    pub t: std::collections::BTreeMap<&'static str, String>,
    pub data: ReportData,
    // —— 需要填数的整句（模板里只负责摆放）——
    pub warnings_title: String,
    pub cc_rescued_title: String,
    pub cc_rescued_body: String,
    pub cc_lost_body: String,
    pub amb_corr_title: String,
    pub ddqc_dims_line: String,
    pub dbl_borderline_line: String,
    pub run_line: String,
    pub footer: String,
    pub sub: String,
}

impl ReportDoc {
    pub fn new(lang: i18n::Lang, data: ReportData) -> Self {
        use i18n::M;
        let t = i18n::template_labels(lang);
        let cc = &data.cell_calling;
        let warnings_title = M::WarningsTitle.f(lang, &[&data.warnings.len().to_string()]);
        let cc_rescued_title = M::CcRescuedTitle.f(lang, &[&cc.n_rescued.to_string()]);
        let cc_rescued_body = M::CcRescuedBody.f(
            lang,
            &[&cc.rescued_median_genes, &cc.rescued_top_genes.join(" / ")],
        );
        let cc_lost_body = M::CcLostBody.f(lang, &[&cc.n_lost.to_string()]);
        let amb_corr_title = data
            .ambient
            .correction
            .as_ref()
            .map(|c| M::AmbCorrTitle.f(lang, &[&c.method]))
            .unwrap_or_default();
        let ddqc_dims_line = M::DdqcDims.f(lang, &[&data.ddqc.dims.join(M::ListSep.t(lang))]);
        let dbl_borderline_line = M::DblBorderlineLine.f(lang, &[&data.doublet.borderline_action]);
        let footer = M::Footer.f(lang, &[&data.sample.version]);
        // 运行信息压成一行：时间 · 种子 · 耗时。版本与 git 哈希在标题下的 sub 里已经有了。
        let run_line = M::RunLine.f(
            lang,
            &[
                &data.sample.started_at_utc,
                &data.sample.seed.to_string(),
                &data.sample.elapsed_s,
            ],
        );
        let sub = format!(
            "{}: {} ({}) · {}: {} · {}: {} · scqc {} ({})",
            M::Assay.t(lang),
            data.sample.platform_label,
            data.sample.platform_id,
            M::Species.t(lang),
            data.sample.species,
            M::GeneratedUtc.t(lang),
            data.sample.started_at_utc,
            data.sample.version,
            data.sample.git_hash,
        );
        Self {
            lang: lang.code(),
            html_lang: lang.html_lang(),
            endonym: lang.endonym(),
            t,
            data,
            warnings_title,
            cc_rescued_title,
            cc_rescued_body,
            cc_lost_body,
            amb_corr_title,
            ddqc_dims_line,
            dbl_borderline_line,
            run_line,
            footer,
            sub,
        }
    }
}

/// 渲染 HTML（自动转义；SVG 字段在模板中显式标记 safe）。
/// `docs` 的第一份是主语言（英语），其后是用户选择的语言。
///
/// 两段模板分开渲染再拼装，而不是用 `{% include %}`：这样不依赖 minijinja 的
/// `multi_template` 特性，升级依赖时不会因为特性开关变化而突然渲染失败。
pub fn render_html_multi(docs: &[ReportDoc]) -> Result<String> {
    let first = docs
        .first()
        .ok_or_else(|| ScqcError::internal("报告至少需要一种语言"))?;
    let mut env = minijinja::Environment::new();
    // `{{ n|sep }}`：整数加千位分隔符。报告里的计数大多在 report_build 里就格式化好了，
    // 少数几个直接从数据结构里取的整数（barcode 数、基因数）用这个过滤器。
    env.add_filter("sep", |v: i64| -> String {
        let neg = v < 0;
        let digits = v.unsigned_abs().to_string();
        let b = digits.as_bytes();
        let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
        if neg {
            out.push('-');
        }
        for (i, c) in b.iter().enumerate() {
            if i > 0 && (b.len() - i).is_multiple_of(3) {
                out.push(',');
            }
            out.push(*c as char);
        }
        out
    });
    env.add_template("report.html", TEMPLATE)
        .map_err(|e| ScqcError::internal(format!("报告模板错误：{e}")))?;
    env.add_template("section.html", SECTION)
        .map_err(|e| ScqcError::internal(format!("报告模板错误：{e}")))?;

    let section = env
        .get_template("section.html")
        .map_err(|e| ScqcError::internal(format!("报告模板错误：{e}")))?;
    let mut rendered = Vec::with_capacity(docs.len());
    for d in docs {
        let body = section
            .render(minijinja::context! { d => d })
            .map_err(|e| ScqcError::internal(format!("报告渲染失败：{e}")))?;
        rendered.push(RenderedDoc {
            lang: d.lang,
            html_lang: d.html_lang,
            endonym: d.endonym,
            body,
        });
    }

    let tmpl = env
        .get_template("report.html")
        .map_err(|e| ScqcError::internal(format!("报告模板错误：{e}")))?;
    let ctx = minijinja::context! {
        title => first.data.title.clone(),
        docs => rendered,
        primary_html_lang => first.html_lang,
        primary_report_title => first.t.get("report_title").cloned().unwrap_or_default(),
        primary_language => first.t.get("language").cloned().unwrap_or_default(),
        primary_show_both => first.t.get("show_both").cloned().unwrap_or_default(),
        primary_sub => first.sub.clone(),
    };
    tmpl.render(ctx)
        .map_err(|e| ScqcError::internal(format!("报告渲染失败：{e}")))
}

#[derive(Debug, Clone, Serialize)]
struct RenderedDoc {
    lang: &'static str,
    html_lang: &'static str,
    endonym: &'static str,
    body: String,
}

/// 单语言便捷入口（命令行默认英语）。
pub fn render_html(data: &ReportData) -> Result<String> {
    render_html_multi(&[ReportDoc::new(i18n::Lang::En, data.clone())])
}
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn renders_all_sections_and_escapes() {
        let mut d = ReportData {
            title: "测试 <样本>".into(),
            ..Default::default()
        };
        d.overview.knee_svg = svg::knee_plot(&[1.0, 10.0], &[100, 10], 50, 20, 5, "rank", "UMI");
        d.methods = methods::entries(i18n::Lang::En);
        d.limitations_global = methods::global_limitations(i18n::Lang::En);
        d.warnings.push(KeyValue::new("X", "a<b"));
        let html = render_html(&d).unwrap();
        assert!(html.contains("测试 &lt;样本&gt;"));
        assert!(html.contains("<svg"));
        assert!(html.contains("a&lt;b"));
        for h in [
            "1. Result",
            "2. Cells",
            "3. Quality metrics",
            "4. Filtering",
            "5. Ambient RNA",
            "6. Sample and inputs",
            "7. Methods",
            "8. Parameters",
        ] {
            assert!(html.contains(h), "缺少章节 {h}");
        }
        assert!(!html.contains("http://") || html.contains("http://www.w3.org/2000/svg"));
        // 单语言报告不需要语言切换脚本，因此也不该出现 <script>
        assert!(!html.contains("<script"));
        assert!(!html.contains("<link"));
    }

    /// 双语报告：两份正文都完整，且带语言切换。
    #[test]
    fn renders_two_languages() {
        let mut d = ReportData {
            title: "sample".into(),
            ..Default::default()
        };
        d.methods = methods::entries(i18n::Lang::En);
        d.limitations_global = methods::global_limitations(i18n::Lang::En);
        let mut zh = d.clone();
        zh.methods = methods::entries(i18n::Lang::Zh);
        zh.limitations_global = methods::global_limitations(i18n::Lang::Zh);
        let html = render_html_multi(&[
            ReportDoc::new(i18n::Lang::En, d),
            ReportDoc::new(i18n::Lang::Zh, zh),
        ])
        .unwrap();
        assert!(html.contains("1. Result"));
        assert!(html.contains("1. 结果"));
        assert!(html.contains("langbar"));
        assert_eq!(html.matches("class=\"doc\"").count(), 2);
    }
}
