//! `PlatformProfile` 数据结构（DESIGN §3.4，对应 SPEC §7.3）。
//!
//! 字段值有依据的写死在 `builtin.rs`；无依据的字段为 `None`，
//! 由流程在需要时返回 `need_input` 或在报告中注明（SPEC §17.3）。

use serde::{Deserialize, Serialize};

use super::species::Species;

/// 内置 Profile 标识。以后接入其他厂商只需新增变体 + `builtin.rs` 常量。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProfileId {
    #[serde(rename = "10x-scRNA")]
    TenxScRna,
    #[serde(rename = "10x-snRNA")]
    TenxSnRna,
    #[serde(rename = "10x-Flex-singleplex")]
    TenxFlexSingleplex,
    #[serde(rename = "10x-Flex-multiplex")]
    TenxFlexMultiplex,
}

impl ProfileId {
    pub const ALL: [ProfileId; 4] = [
        ProfileId::TenxScRna,
        ProfileId::TenxSnRna,
        ProfileId::TenxFlexSingleplex,
        ProfileId::TenxFlexMultiplex,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ProfileId::TenxScRna => "10x-scRNA",
            ProfileId::TenxSnRna => "10x-snRNA",
            ProfileId::TenxFlexSingleplex => "10x-Flex-singleplex",
            ProfileId::TenxFlexMultiplex => "10x-Flex-multiplex",
        }
    }

    pub fn parse(s: &str) -> Option<ProfileId> {
        Self::ALL
            .iter()
            .copied()
            .find(|p| p.as_str().eq_ignore_ascii_case(s))
    }

    /// 中文名称（界面 / 报告用）
    pub fn label_zh(&self) -> &'static str {
        match self {
            ProfileId::TenxScRna => "10x 单细胞（scRNA-seq）",
            ProfileId::TenxSnRna => "10x 单核（snRNA-seq）",
            ProfileId::TenxFlexSingleplex => "10x Flex 单重",
            ProfileId::TenxFlexMultiplex => "10x Flex 多重化",
        }
    }
}

/// ddqc 的离群检测维度（SPEC §8.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DdqcDim {
    /// 总 UMI（下限）
    Counts,
    /// 基因数（下限）
    Genes,
    /// 线粒体比例（上限）
    Mito,
    /// 核糖体比例（下限）
    Ribo,
}

/// 线粒体比例的语义（SPEC §7.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MitoSemantics {
    /// 细胞活性指标
    Viability,
    /// 核剥离质量（语义反转）
    StrippingQuality,
    /// 探针覆盖被压低，数值不可跨平台比较
    Dampened,
}

impl MitoSemantics {
    pub fn label_zh(&self) -> &'static str {
        match self {
            MitoSemantics::Viability => "细胞活性",
            MitoSemantics::StrippingQuality => "核剥离质量",
            MitoSemantics::Dampened => "探针降采样后的线粒体信号（数值偏低，不可与 GEX 比较）",
        }
    }
}

/// 真细胞鉴定策略（DESIGN §0：Flex 第一版沿用上游判定）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum CellCalling {
    /// 运行 knee + EmptyDrops；`lower` 为环境液滴的 UMI 上界
    EmptyDrops { lower: u32 },
    /// 直接采用 Cell Ranger 的 filtered 细胞集合，只画 knee plot
    TrustUpstream,
}

/// 环境 RNA 估计器种类（SPEC §8.2.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmbientEstimator {
    /// 估计器 A：SoupX 式（从论文实现）
    Soup,
    /// 估计器 B：DecontX background 模式
    DecontX,
    /// 估计器 C：线粒体法（仅 snRNA）
    Mito,
}

/// 环境 RNA 参考区间（SPEC §7.2.5）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AmbientReference {
    /// 平台平均污染度（scRNA 7.79% / snRNA 11.80%）
    pub mean: f64,
    /// 低于此值：校正开关置灰（scRNA 0.10 / snRNA 0.15）
    pub grey_below: f64,
    /// 高于此值：不推荐校正（0.20）
    pub discourage_above: f64,
}

/// 平台配置。所有平台差异都在这里，算法只读字段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlatformProfile {
    pub schema_version: u32,
    pub id: ProfileId,
    pub species: Species,
    /// 识别依据，原样写入 platform_profile.json
    pub evidence: Vec<String>,
    /// 与 `evidence` 一一对应的语言中立版本（界面三语渲染用）
    #[serde(default)]
    pub evidence_codes: Vec<crate::platform::detect::CodedText>,
    /// 用户改过哪些字段（如 "platform"、"species"、"multiplex_n"）
    /// 用户覆盖了哪些字段。语言中立（`code` + `args`），界面与报告各用自己的词典渲染。
    pub user_overrides: Vec<crate::platform::detect::CodedText>,

    // —— 指标有效性 ——
    pub ddqc_dimensions: Vec<DdqcDim>,
    pub show_ribo: bool,
    /// 不显示核糖体比例时的原因（报告用）
    pub ribo_hidden_reason_zh: Option<String>,
    pub mito_semantics: MitoSemantics,
    /// ddqc 线粒体阈值护栏（阈值不低于此值）；None ⇒ 不加护栏并在报告注明（Flex）
    pub mito_guardrail: Option<f64>,
    /// snRNA：MAD 的最小差值 0.005
    pub mito_mad_min_diff: Option<f64>,

    // —— 真细胞鉴定 ——
    pub cell_calling: CellCalling,

    // —— 双细胞 ——
    pub multiplex_n: Option<u32>,
    /// 每 1000 个上样细胞的双细胞率（10x 常用口径 0.008），界面可调
    pub doublet_rate_per_1000: f64,

    // —— 环境 RNA ——
    pub ambient_estimators: Vec<AmbientEstimator>,
    /// None ⇒ 只报数值，不做高低判定（Flex）
    pub ambient_reference: Option<AmbientReference>,

    /// 第一版限制说明（写入报告"已知局限性"）
    pub caveats_zh: Vec<String>,
}

impl PlatformProfile {
    /// 预期双细胞率（SPEC §7.2.3 / §8.5.3）：
    /// rate = per_1000 × n_loaded / 1000 ÷ multiplex_n
    pub fn expected_doublet_rate(&self, n_loaded: usize) -> f64 {
        let base = self.doublet_rate_per_1000 * n_loaded as f64 / 1000.0;
        let m = self.multiplex_n.unwrap_or(1).max(1) as f64;
        (base / m).clamp(0.0, 1.0)
    }

    pub fn uses_ribo(&self) -> bool {
        self.ddqc_dimensions.contains(&DdqcDim::Ribo)
    }
}
