//! 四个内置 Profile 的字段值（DESIGN §3.4 表）。
//!
//! 只写有依据的值；"待标定"的字段为 `None`。每个值旁注明依据。

use super::profile::*;
use super::species::Species;

pub const SCHEMA_VERSION: u32 = 1;

/// 10x 常用口径：每 1000 个回收细胞约 0.8% 双细胞率（DESIGN §16 Q11 已确认）。
pub const DOUBLET_RATE_PER_1000: f64 = 0.008;

/// 由 id + 物种 + 多重化重数生成 Profile。`evidence` / `user_overrides` 由调用方填。
pub fn build(id: ProfileId, species: Species, multiplex_n: Option<u32>) -> PlatformProfile {
    let guardrail = species.mito_guardrail();
    let base = PlatformProfile {
        schema_version: SCHEMA_VERSION,
        id,
        species: species.clone(),
        evidence: Vec::new(),
        evidence_codes: Vec::new(),
        user_overrides: Vec::new(),
        ddqc_dimensions: vec![
            DdqcDim::Counts,
            DdqcDim::Genes,
            DdqcDim::Mito,
            DdqcDim::Ribo,
        ],
        show_ribo: true,
        ribo_hidden_reason_zh: None,
        mito_semantics: MitoSemantics::Viability,
        mito_guardrail: guardrail,
        mito_mad_min_diff: None,
        // SPEC §8.1.2：scRNA lower = 100
        cell_calling: CellCalling::EmptyDrops { lower: 100 },
        multiplex_n: None,
        doublet_rate_per_1000: DOUBLET_RATE_PER_1000,
        ambient_estimators: vec![AmbientEstimator::Soup, AmbientEstimator::DecontX],
        ambient_reference: None,
        caveats_zh: Vec::new(),
    };

    match id {
        ProfileId::TenxScRna => PlatformProfile {
            // SPEC §7.2.5 / §8.3.2：scRNA 平均 7.79%，<10% 置灰，>20% 不推荐
            ambient_reference: Some(AmbientReference {
                mean: 0.0779,
                grey_below: 0.10,
                discourage_above: 0.20,
            }),
            ..base
        },
        ProfileId::TenxSnRna => PlatformProfile {
            mito_semantics: MitoSemantics::StrippingQuality,
            // SPEC §7.2.2：MAD + 最小差值 0.5%
            mito_mad_min_diff: Some(0.005),
            // SPEC §7.2.4：snRNA 建议 50，需实测标定
            cell_calling: CellCalling::EmptyDrops { lower: 50 },
            ambient_estimators: vec![
                AmbientEstimator::Soup,
                AmbientEstimator::DecontX,
                AmbientEstimator::Mito,
            ],
            // SPEC §7.2.5 / §8.3.2：snRNA 平均 11.80%，<15% 置灰
            ambient_reference: Some(AmbientReference {
                mean: 0.1180,
                grey_below: 0.15,
                discourage_above: 0.20,
            }),
            caveats_zh: vec![
                "单核 EmptyDrops 的 lower=50 为建议值，尚未用实测数据标定。".into(),
                "线粒体法污染度估计（估计器 C）尚未正式标定，仅作交叉参考。".into(),
            ],
            ..base
        },
        ProfileId::TenxFlexSingleplex | ProfileId::TenxFlexMultiplex => PlatformProfile {
            // SPEC §7.2.1【硬约束】：Flex 禁用 percent.ribo 维度
            ddqc_dimensions: vec![DdqcDim::Counts, DdqcDim::Genes, DdqcDim::Mito],
            show_ribo: false,
            ribo_hidden_reason_zh: Some(
                "Flex 探针集不含核糖体蛋白基因，核糖体比例恒为 0，无法作为质控指标。".into(),
            ),
            mito_semantics: MitoSemantics::Dampened,
            // 2026-09-11 真实 Flex 数据实测修订（负责人确认）：Flex 探针集几乎不覆盖线粒体基因，
            // 线粒体比例中位数只有 0.09%–0.31%（GEX 约 2%）。若不设护栏，「簇内中位数 + 2×MAD」
            // 会落在 0.1%–0.5% 的噪声量级，把健康细胞成批删掉（肾单核数据 978 个、PBMC 4-plex 965 个）。
            // 因此沿用单核的 5% 作为下限——这是**未标定的保守取值**，报告中注明，用户可用
            // --mito-guardrail 覆盖。不得借用 GEX 的阈值口径（SPEC §7.2.1）。
            mito_guardrail: Some(0.05),
            // DESIGN §0（已确认）：第一版沿用 Cell Ranger 的细胞集合
            cell_calling: CellCalling::TrustUpstream,
            multiplex_n: if id == ProfileId::TenxFlexMultiplex {
                multiplex_n
            } else {
                None
            },
            // SPEC §7.2.5【硬约束】：Flex 无参考区间，只报数值
            ambient_reference: None,
            caveats_zh: {
                let mut c = vec![
                    "Flex 平台的真细胞鉴定未独立执行，本版沿用 Cell Ranger 的细胞判定（待标定）。".into(),
                    "Flex 线粒体阈值未标定：探针集几乎不覆盖线粒体基因，实测中位数仅 0.1%–0.3%，\
                     因此簇内自适应阈值另加 5% 的下限护栏（保守取值，可用 --mito-guardrail 调整），\
                     否则会把线粒体比例仅 1% 左右的健康细胞误删。".into(),
                    "Flex 平台目前没有公开的环境 RNA 参考基准，污染度只报告数值、不做高低判定。".into(),
                    "Flex 探针集排除核糖体蛋白、非编码 RNA、TCR/IG、HLA/KIR 基因，基因宇宙与 GEX 不同。".into(),
                ];
                if id == ProfileId::TenxFlexMultiplex {
                    c.push(
                        "多重化 Flex 的预期双细胞率按 1/重数 修正，这是启发式而非精确公式，可在参数中调整。"
                            .into(),
                    );
                }
                c
            },
            ..base
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flex_disables_ribo_and_reference() {
        let p = build(ProfileId::TenxFlexMultiplex, Species::Human, Some(16));
        assert!(!p.uses_ribo());
        assert!(!p.show_ribo);
        assert!(p.ambient_reference.is_none());
        // 真实 Flex 数据实测后加的护栏（见上方注释）
        assert_eq!(p.mito_guardrail, Some(0.05));
        assert_eq!(p.cell_calling, CellCalling::TrustUpstream);
        assert_eq!(p.multiplex_n, Some(16));
    }

    #[test]
    fn snrna_min_diff_and_mito_estimator() {
        let p = build(ProfileId::TenxSnRna, Species::Mouse, None);
        assert_eq!(p.mito_mad_min_diff, Some(0.005));
        assert_eq!(p.mito_guardrail, Some(0.05));
        assert!(p.ambient_estimators.contains(&AmbientEstimator::Mito));
        assert_eq!(p.mito_semantics, MitoSemantics::StrippingQuality);
    }

    #[test]
    fn multiplex_scales_doublet_rate() {
        // SPEC §14.2：多重化 Flex 的预期率按重数缩放，明显更低
        let single = build(ProfileId::TenxFlexSingleplex, Species::Human, None);
        let multi = build(ProfileId::TenxFlexMultiplex, Species::Human, Some(16));
        let r1 = single.expected_doublet_rate(10_000);
        let r16 = multi.expected_doublet_rate(10_000);
        assert!((r1 - 0.08).abs() < 1e-12);
        assert!((r16 - 0.005).abs() < 1e-12);
    }
}
