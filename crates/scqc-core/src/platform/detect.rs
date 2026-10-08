//! 平台自动识别（SPEC §7.4，C9；DESIGN §5.2）。
//!
//! 识别 = 收集证据 → 投票 → 得出候选；**任何歧义都返回 `NeedInput`，绝不猜测**。
//!
//! 第一版的一个结论（DESIGN Q5 已确认）：scRNA 与 snRNA 无法从矩阵或元数据可靠区分，
//! 因此对 GEX 数据一律要求用户确认 sc / sn。

use serde::{Deserialize, Serialize};

use super::builtin;
use super::profile::{PlatformProfile, ProfileId};
use super::species::{guess_species, Species, SpeciesGuess};
use crate::io::{BarcodeTable, CellRangerMetrics, FeatureTable};

/// 检测层面的化学类型（GEX 还是 Flex），比 ProfileId 粗一级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Assay {
    Gex,
    Flex,
}

/// 语言中立的一条文本：稳定 `code` + 参数，界面用自己的词典渲染（SPEC §10.5）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CodedText {
    pub code: String,
    pub args: Vec<String>,
}

impl CodedText {
    pub fn new(code: &str, args: impl IntoIterator<Item = String>) -> Self {
        Self {
            code: code.to_string(),
            args: args.into_iter().collect(),
        }
    }
}

/// 一条证据：来源、支持哪种化学、强弱。
///
/// `text_zh` 供中文 HTML 报告；`code`/`args` 供三语界面。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub source: String,
    pub supports: Option<Assay>,
    pub strong: bool,
    pub text_zh: String,
    pub code: String,
    pub args: Vec<String>,
}

/// 识别结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Detection {
    pub assay: Option<Assay>,
    pub assay_conflict: bool,
    /// Flex 下：是否多重化（由 barcode 结构判断）
    pub flex_multiplex: Option<bool>,
    pub multiplex_n: Option<u32>,
    pub species: SpeciesGuess,
    /// 可能的 Profile（GEX 下恒为 [scRNA, snRNA]，需用户选）
    pub candidates: Vec<ProfileId>,
    pub evidence: Vec<Evidence>,
}

/// 需要用户提供的信息（NDJSON `need_input`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NeedInput {
    /// "platform" | "species" | "multiplex_n" | "species_rule"
    pub field: String,
    pub message_zh: String,
    pub options: Vec<InputOption>,
    pub evidence: Vec<String>,
    /// 与 `evidence` 一一对应的语言中立版本
    pub evidence_codes: Vec<CodedText>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputOption {
    pub value: String,
    pub label_zh: String,
}

/// 用户（或命令行）给出的选择。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UserChoice {
    pub platform: Option<ProfileId>,
    pub species: Option<Species>,
    pub multiplex_n: Option<u32>,
}

/// 收集证据并给出识别结果。`raw_barcodes` 优先用于多重化判断（含全部 Probe Barcode）。
pub fn detect(
    features: &FeatureTable,
    filtered_barcodes: &BarcodeTable,
    raw_barcodes: Option<&BarcodeTable>,
    metrics: Option<&CellRangerMetrics>,
) -> Detection {
    let mut ev: Vec<Evidence> = Vec::new();

    // 1. metrics_summary 中的 chemistry / assay 字段
    if let Some(m) = metrics {
        let chem = m
            .find("chemistry")
            .or_else(|| m.find("assay"))
            .map(|(k, v)| (k.clone(), v.clone()));
        match chem {
            Some((k, v)) => {
                let lv = v.to_ascii_lowercase();
                let supports =
                    if lv.contains("flex") || lv.contains("fixed rna") || lv.contains("frp") {
                        Some(Assay::Flex)
                    } else if lv.contains("3'")
                        || lv.contains("5'")
                        || lv.contains("3’")
                        || lv.contains("5’")
                        || lv.contains("single cell 3")
                        || lv.contains("single cell 5")
                        || lv.contains("gem-x")
                    {
                        Some(Assay::Gex)
                    } else {
                        None
                    };
                ev.push(Evidence {
                    source: "metrics_summary".into(),
                    supports,
                    strong: supports.is_some(),
                    text_zh: format!("metrics_summary 的 {k} 字段为「{v}」"),
                    code: "metrics_field".into(),
                    args: vec![k.to_string(), v.to_string()],
                });
            }
            None => ev.push(Evidence {
                source: "metrics_summary".into(),
                supports: None,
                strong: false,
                text_zh: "metrics_summary 中没有 chemistry/assay 字段".into(),
                code: "metrics_no_chem".into(),
                args: vec![],
            }),
        }
    } else {
        ev.push(Evidence {
            source: "metrics_summary".into(),
            supports: None,
            strong: false,
            text_zh: "未提供 metrics_summary.csv".into(),
            code: "metrics_absent".into(),
            args: vec![],
        });
    }

    // 2. features 额外列是否含 probe 相关信息
    let probe_col = features.extra_columns.iter().any(|(_, col)| {
        col.iter()
            .take(200)
            .any(|v| v.to_ascii_lowercase().contains("probe"))
    });
    if probe_col {
        ev.push(Evidence {
            source: "features".into(),
            supports: Some(Assay::Flex),
            strong: true,
            text_zh: "features 文件的附加列中出现 probe 相关字段".into(),
            code: "features_probe_col".into(),
            args: vec![],
        });
    }

    // 3. 核糖体蛋白基因是否存在（Flex 探针集不含）
    let species = guess_species(features);
    let n_ge = features.gene_expression_indices().len();
    let n_ribo = species.n_ribo_human.max(species.n_ribo_mouse);
    if n_ge >= 1000 {
        if n_ribo == 0 {
            ev.push(Evidence {
                source: "features".into(),
                supports: Some(Assay::Flex),
                strong: true,
                text_zh: format!(
                    "{n_ge} 个基因中没有任何 RPS/RPL 核糖体蛋白基因（Flex 探针集特征）"
                ),
                code: "features_no_ribo".into(),
                args: vec![n_ge.to_string()],
            });
        } else if n_ribo >= 50 {
            ev.push(Evidence {
                source: "features".into(),
                supports: Some(Assay::Gex),
                strong: true,
                text_zh: format!("存在 {n_ribo} 个 RPS/RPL 核糖体蛋白基因（GEX 参考特征）"),
                code: "features_ribo".into(),
                args: vec![n_ribo.to_string()],
            });
        }
    }

    // 4. barcode 结构（长于 16 nt = GEM + Probe Barcode → Flex 多重化）
    //    Probe Barcode 长度随化学版本变化：Flex v1 = 8 nt（总 24），Flex v2 / Apex = 10 nt（总 26）。
    let bc_for_structure = raw_barcodes.unwrap_or(filtered_barcodes);
    let st = bc_for_structure.structure();
    let total: usize = st.seq_len_counts.values().sum();
    let n16 = st.seq_len_counts.get(&16).copied().unwrap_or(0);
    // 占比最高的"长于 16 nt"的长度
    let long_len = st
        .seq_len_counts
        .iter()
        .filter(|(&l, _)| l > crate::io::barcodes::GEM_BARCODE_LEN)
        .max_by_key(|(_, &c)| c)
        .map(|(&l, &c)| (l, c));
    let mut flex_multiplex = None;
    let mut multiplex_n = None;
    if let Some((len, count)) = long_len.filter(|&(_, c)| total > 0 && c * 10 >= total * 9) {
        flex_multiplex = Some(true);
        multiplex_n = Some(st.n_probe_barcodes as u32);
        let _ = count;
        ev.push(Evidence {
            source: "barcodes".into(),
            supports: Some(Assay::Flex),
            strong: true,
            text_zh: format!(
                "barcode 为 {len} nt（GEM {} + Probe {}），共 {} 种 Probe Barcode → 多重化 Flex",
                crate::io::barcodes::GEM_BARCODE_LEN,
                len - crate::io::barcodes::GEM_BARCODE_LEN,
                st.n_probe_barcodes
            ),
            code: "barcode_multiplex".into(),
            args: vec![
                len.to_string(),
                crate::io::barcodes::GEM_BARCODE_LEN.to_string(),
                (len - crate::io::barcodes::GEM_BARCODE_LEN).to_string(),
                st.n_probe_barcodes.to_string(),
            ],
        });
    } else if total > 0 && n16 * 10 >= total * 9 {
        flex_multiplex = Some(false);
        ev.push(Evidence {
            source: "barcodes".into(),
            supports: None,
            strong: false,
            text_zh: "barcode 为 16 nt，无 Probe Barcode 结构（GEX 或单重 Flex）".into(),
            code: "barcode_16nt".into(),
            args: vec![],
        });
    } else {
        ev.push(Evidence {
            source: "barcodes".into(),
            supports: None,
            strong: false,
            text_zh: format!("barcode 长度分布不典型：{:?}", st.seq_len_counts),
            code: "barcode_atypical".into(),
            args: vec![format!("{:?}", st.seq_len_counts)],
        });
    }

    // 5. 基因总数（弱证据）
    if n_ge > 0 {
        let supports = if n_ge < 25_000 {
            Some(Assay::Flex)
        } else if n_ge >= 30_000 {
            Some(Assay::Gex)
        } else {
            None
        };
        ev.push(Evidence {
            source: "features".into(),
            supports,
            strong: false,
            text_zh: format!("Gene Expression 类型基因共 {n_ge} 个（Flex ≈ 18k，GEX ≈ 36k）"),
            code: "n_gene_expression".into(),
            args: vec![n_ge.to_string()],
        });
    }

    // ---- 投票：只看强证据；强证据冲突 → 歧义 ----
    let strong_flex = ev
        .iter()
        .filter(|e| e.strong && e.supports == Some(Assay::Flex))
        .count();
    let strong_gex = ev
        .iter()
        .filter(|e| e.strong && e.supports == Some(Assay::Gex))
        .count();
    let (assay, conflict) = match (strong_flex > 0, strong_gex > 0) {
        (true, false) => (Some(Assay::Flex), false),
        (false, true) => (Some(Assay::Gex), false),
        (true, true) => (None, true),
        (false, false) => (None, false),
    };

    let candidates = match assay {
        Some(Assay::Gex) => vec![ProfileId::TenxScRna, ProfileId::TenxSnRna],
        Some(Assay::Flex) => match flex_multiplex {
            Some(true) => vec![ProfileId::TenxFlexMultiplex],
            Some(false) => vec![ProfileId::TenxFlexSingleplex],
            None => vec![ProfileId::TenxFlexSingleplex, ProfileId::TenxFlexMultiplex],
        },
        None => ProfileId::ALL.to_vec(),
    };

    Detection {
        assay,
        assay_conflict: conflict,
        flex_multiplex,
        multiplex_n,
        species,
        candidates,
        evidence: ev,
    }
}

fn platform_options(cands: &[ProfileId]) -> Vec<InputOption> {
    cands
        .iter()
        .map(|p| InputOption {
            value: p.as_str().to_string(),
            label_zh: p.label_zh().to_string(),
        })
        .collect()
}

/// 把识别结果与用户选择合成为最终 Profile；信息不足时返回需要用户回答的问题列表。
///
/// C9：这里没有任何"默认值"分支。
pub fn resolve(det: &Detection, choice: &UserChoice) -> Result<PlatformProfile, Vec<NeedInput>> {
    let mut needs = Vec::new();
    let evidence_text: Vec<String> = det.evidence.iter().map(|e| e.text_zh.clone()).collect();
    let evidence_coded: Vec<CodedText> = det
        .evidence
        .iter()
        .map(|e| CodedText {
            code: e.code.clone(),
            args: e.args.clone(),
        })
        .collect();

    // —— 平台 ——
    let platform = match choice.platform {
        Some(p) => Some(p),
        None => {
            if det.candidates.len() == 1 {
                Some(det.candidates[0])
            } else {
                let msg = if det.assay_conflict {
                    "证据相互矛盾，无法确定平台类型，请选择".to_string()
                } else if det.assay == Some(Assay::Gex) {
                    "已识别为 10x GEX 化学，但单细胞与单核无法从数据区分，请选择".to_string()
                } else {
                    "无法确定平台类型，请选择".to_string()
                };
                needs.push(NeedInput {
                    field: "platform".into(),
                    message_zh: msg,
                    options: platform_options(&det.candidates),
                    evidence: evidence_text.clone(),
                    evidence_codes: evidence_coded.clone(),
                });
                None
            }
        }
    };

    // —— 物种 ——
    let species = match &choice.species {
        Some(s) => Some(s.clone()),
        None => match &det.species.species {
            Some(s) => Some(s.clone()),
            None => {
                needs.push(NeedInput {
                    field: "species".into(),
                    message_zh: "无法从基因名判断物种，请选择".into(),
                    options: vec![
                        InputOption {
                            value: "human".into(),
                            label_zh: "人".into(),
                        },
                        InputOption {
                            value: "mouse".into(),
                            label_zh: "小鼠".into(),
                        },
                        InputOption {
                            value: "other".into(),
                            label_zh: "其他（需提供线粒体/核糖体基因前缀与线粒体阈值）".into(),
                        },
                    ],
                    evidence: det.species.evidence.clone(),
                    evidence_codes: det.species.evidence_codes.clone(),
                });
                None
            }
        },
    };
    if let Some(Species::Other {
        mito_guardrail: None,
        ..
    }) = &species
    {
        needs.push(NeedInput {
            field: "species_rule".into(),
            message_zh: "物种为「其他」时没有文献依据的线粒体阈值，请提供线粒体护栏（0–1）".into(),
            options: vec![],
            evidence: vec![],
            evidence_codes: vec![],
        });
    }

    // —— 多重化重数 ——
    let multiplex_n = if platform == Some(ProfileId::TenxFlexMultiplex) {
        match choice.multiplex_n.or(det.multiplex_n) {
            Some(n) if n >= 1 => Some(n),
            _ => {
                needs.push(NeedInput {
                    field: "multiplex_n".into(),
                    message_zh: "多重化 Flex 需要多重化重数（如 4 / 16），无法从数据确定，请填写"
                        .into(),
                    options: vec![],
                    evidence: evidence_text.clone(),
                    evidence_codes: evidence_coded.clone(),
                });
                None
            }
        }
    } else {
        None
    };

    if !needs.is_empty() {
        return Err(needs);
    }
    let (Some(platform), Some(species)) = (platform, species) else {
        // 上面已保证二者均为 Some；此分支只为类型完备。
        return Err(needs);
    };

    let mut profile = builtin::build(platform, species, multiplex_n);
    profile.evidence = evidence_text;
    profile
        .evidence
        .extend(det.species.evidence.iter().cloned());
    profile.evidence_codes = evidence_coded;
    profile
        .evidence_codes
        .extend(det.species.evidence_codes.iter().cloned());
    if choice.platform.is_some() {
        let differs = det.candidates.len() == 1 && det.candidates[0] != platform;
        profile.user_overrides.push(if differs {
            CodedText::new(
                "override_platform_differs",
                [
                    platform.as_str().to_string(),
                    det.candidates[0].as_str().to_string(),
                ],
            )
        } else {
            CodedText::new("override_platform_confirmed", [])
        });
    }
    if choice.species.is_some() {
        profile
            .user_overrides
            .push(CodedText::new("override_species", []));
    }
    if choice.multiplex_n.is_some() {
        profile
            .user_overrides
            .push(CodedText::new("override_multiplex_n", []));
    }
    Ok(profile)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn gex_features() -> FeatureTable {
        let mut names: Vec<String> = (0..36_000).map(|i| format!("GENE{i}")).collect();
        for (i, n) in ["MT-CO1", "MT-CO2", "MT-ND1", "MT-ND2", "MT-CYB", "MT-ATP6"]
            .iter()
            .enumerate()
        {
            names[i] = n.to_string();
        }
        for i in 0..80 {
            names[100 + i] = format!("RPL{i}");
        }
        FeatureTable {
            id: names.iter().map(|n| format!("ENSG_{n}")).collect(),
            name: names.clone(),
            feature_type: vec!["Gene Expression".into(); names.len()],
            extra_columns: vec![],
        }
    }

    fn flex_features() -> FeatureTable {
        let mut names: Vec<String> = (0..18_000).map(|i| format!("GENE{i}")).collect();
        for (i, n) in ["MT-CO1", "MT-CO2", "MT-ND1", "MT-ND2", "MT-CYB", "MT-ATP6"]
            .iter()
            .enumerate()
        {
            names[i] = n.to_string();
        }
        FeatureTable {
            id: names.iter().map(|n| format!("ENSG_{n}")).collect(),
            name: names.clone(),
            feature_type: vec!["Gene Expression".into(); names.len()],
            extra_columns: vec![],
        }
    }

    fn barcodes(seqs: &[&str]) -> BarcodeTable {
        BarcodeTable {
            barcode: seqs.iter().map(|s| s.to_string()).collect(),
            probe_barcode: seqs
                .iter()
                .map(|s| BarcodeTable::parse_probe_barcode(s))
                .collect(),
        }
    }

    #[test]
    fn gex_requires_sc_sn_choice() {
        let f = gex_features();
        let b = barcodes(&["AAACCTGAGAAACCAT-1", "AAACCTGAGAAACCGG-1"]);
        let d = detect(&f, &b, None, None);
        assert_eq!(d.assay, Some(Assay::Gex));
        assert_eq!(
            d.candidates,
            vec![ProfileId::TenxScRna, ProfileId::TenxSnRna]
        );
        // SPEC §14.2：识别不唯一 → need_input，不使用默认值
        let r = resolve(&d, &UserChoice::default());
        let needs = r.err().unwrap();
        assert_eq!(needs[0].field, "platform");
        // 用户选择后可以解析
        let p = resolve(
            &d,
            &UserChoice {
                platform: Some(ProfileId::TenxSnRna),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(p.id, ProfileId::TenxSnRna);
        assert_eq!(p.species, Species::Human);
    }

    #[test]
    fn flex_multiplex_detected() {
        let f = flex_features();
        let b = barcodes(&[
            "AAACAAGCAAACAAGAACTTTAGG-1",
            "AAACAAGCAAACAAGACCGGTTAA-1",
            "AAACAAGCAAACAAGAGGTTCCAA-1",
        ]);
        let d = detect(&f, &b, Some(&b), None);
        assert_eq!(d.assay, Some(Assay::Flex));
        assert_eq!(d.flex_multiplex, Some(true));
        assert_eq!(d.multiplex_n, Some(3));
        let p = resolve(&d, &UserChoice::default()).unwrap();
        assert_eq!(p.id, ProfileId::TenxFlexMultiplex);
        assert_eq!(p.multiplex_n, Some(3));
        assert!(p.user_overrides.is_empty());
    }

    #[test]
    fn unknown_species_needs_input() {
        let mut f = flex_features();
        for n in f.name.iter_mut() {
            if n.starts_with("MT-") {
                *n = "X".into();
            }
        }
        let b = barcodes(&["AAACCTGAGAAACCAT-1"]);
        let d = detect(&f, &b, None, None);
        let needs = resolve(
            &d,
            &UserChoice {
                platform: Some(ProfileId::TenxFlexSingleplex),
                ..Default::default()
            },
        )
        .err()
        .unwrap();
        assert!(needs.iter().any(|n| n.field == "species"));
    }
}
