//! 物种：线粒体 / 核糖体基因命名规则与自动识别。
//!
//! - 人：`MT-` 前缀（如 MT-CO1），核糖体 `RPS*` / `RPL*`
//! - 鼠：`mt-` 前缀（如 mt-Co1），核糖体 `Rps*` / `Rpl*`
//! - 其他：用户必须提供前缀（C9 精神：不猜）
//!
//! 线粒体护栏（SPEC §7.2.4，Osorio & Cai 2021）：人 10%，鼠 5%，其他无依据。

use serde::{Deserialize, Serialize};

use crate::io::FeatureTable;
use crate::qc::GeneSets;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Species {
    Human,
    Mouse,
    Other {
        mito_prefix: String,
        ribo_prefixes: Vec<String>,
        /// 用户提供的线粒体护栏（0–1）；None 表示未提供
        mito_guardrail: Option<f64>,
    },
}

impl Species {
    pub fn rule(&self) -> SpeciesRule {
        match self {
            Species::Human => SpeciesRule {
                mito_prefix: "MT-".to_string(),
                ribo_prefixes: vec!["RPS".to_string(), "RPL".to_string()],
                case_sensitive: true,
            },
            Species::Mouse => SpeciesRule {
                mito_prefix: "mt-".to_string(),
                ribo_prefixes: vec!["Rps".to_string(), "Rpl".to_string()],
                case_sensitive: true,
            },
            Species::Other {
                mito_prefix,
                ribo_prefixes,
                ..
            } => SpeciesRule {
                mito_prefix: mito_prefix.clone(),
                ribo_prefixes: ribo_prefixes.clone(),
                case_sensitive: true,
            },
        }
    }

    /// 线粒体护栏（阈值下限）。人 0.10 / 鼠 0.05 / 其他由用户给。
    pub fn mito_guardrail(&self) -> Option<f64> {
        match self {
            Species::Human => Some(0.10),
            Species::Mouse => Some(0.05),
            Species::Other { mito_guardrail, .. } => *mito_guardrail,
        }
    }

    pub fn label_zh(&self) -> String {
        match self {
            Species::Human => "人".to_string(),
            Species::Mouse => "小鼠".to_string(),
            Species::Other { mito_prefix, .. } => format!("其他（线粒体前缀 {mito_prefix}）"),
        }
    }

    pub fn parse_simple(s: &str) -> Option<Species> {
        match s.to_ascii_lowercase().as_str() {
            "human" | "人" => Some(Species::Human),
            "mouse" | "小鼠" | "鼠" => Some(Species::Mouse),
            _ => None,
        }
    }
}

/// 基因名匹配规则。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeciesRule {
    pub mito_prefix: String,
    pub ribo_prefixes: Vec<String>,
    pub case_sensitive: bool,
}

impl SpeciesRule {
    fn starts(&self, name: &str, prefix: &str) -> bool {
        if self.case_sensitive {
            name.starts_with(prefix)
        } else {
            name.to_ascii_lowercase()
                .starts_with(&prefix.to_ascii_lowercase())
        }
    }

    /// 核糖体蛋白基因：RPS/RPL 前缀后紧跟数字，另含 RPLP0/1/2 与 RPSA
    /// （与 ddqc_R 的正则 `^RP[SL][0-9]|^RPLP[0-9]|^RPSA` 一致；排除 RPS6KA1 之类的激酶）。
    fn is_ribo(&self, name: &str) -> bool {
        self.ribo_prefixes.iter().any(|p| {
            if !self.starts(name, p) {
                return false;
            }
            let rest = &name[p.len()..];
            let first = rest.chars().next();
            let digit_next = first.map(|c| c.is_ascii_digit()).unwrap_or(false);
            let rplp = p.to_ascii_uppercase().ends_with('L')
                && rest.len() >= 2
                && rest[..1].eq_ignore_ascii_case("P")
                && rest[1..2].chars().all(|c| c.is_ascii_digit());
            let rpsa = p.to_ascii_uppercase().ends_with('S') && rest.eq_ignore_ascii_case("A");
            (digit_next || rplp || rpsa) && !name.contains("KA") && !name.contains("Ka")
        })
    }

    pub fn gene_sets(&self, features: &FeatureTable) -> GeneSets {
        let mut sets = GeneSets::default();
        for (i, name) in features.name.iter().enumerate() {
            if features.feature_type[i] != "Gene Expression" {
                continue;
            }
            if self.starts(name, &self.mito_prefix) {
                sets.mito.push(i);
            } else if self.is_ribo(name) {
                sets.ribo.push(i);
            }
        }
        sets
    }
}

/// 物种自动识别的结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeciesGuess {
    pub species: Option<Species>,
    pub n_mito_human: usize,
    pub n_mito_mouse: usize,
    pub n_ribo_human: usize,
    pub n_ribo_mouse: usize,
    pub evidence: Vec<String>,
    /// 与 `evidence` 一一对应的语言中立版本（界面三语渲染用）
    #[serde(default)]
    pub evidence_codes: Vec<crate::platform::detect::CodedText>,
}

/// 从基因名猜物种。人鼠线粒体基因各 13 个蛋白编码基因（MT-ND1…MT-CYB）。
pub fn guess_species(features: &FeatureTable) -> SpeciesGuess {
    let human = Species::Human.rule();
    let mouse = Species::Mouse.rule();
    let mut g = SpeciesGuess {
        species: None,
        n_mito_human: 0,
        n_mito_mouse: 0,
        n_ribo_human: 0,
        n_ribo_mouse: 0,
        evidence: Vec::new(),
        evidence_codes: Vec::new(),
    };
    for (i, name) in features.name.iter().enumerate() {
        if features.feature_type[i] != "Gene Expression" {
            continue;
        }
        if human.starts(name, &human.mito_prefix) {
            g.n_mito_human += 1;
        }
        if mouse.starts(name, &mouse.mito_prefix) {
            g.n_mito_mouse += 1;
        }
        if human.is_ribo(name) {
            g.n_ribo_human += 1;
        }
        if mouse.is_ribo(name) {
            g.n_ribo_mouse += 1;
        }
    }
    g.evidence.push(format!(
        "基因名中 MT- 前缀 {} 个、mt- 前缀 {} 个；RPS/RPL {} 个、Rps/Rpl {} 个",
        g.n_mito_human, g.n_mito_mouse, g.n_ribo_human, g.n_ribo_mouse
    ));
    g.evidence_codes
        .push(crate::platform::detect::CodedText::new(
            "species_counts",
            [
                g.n_mito_human.to_string(),
                g.n_mito_mouse.to_string(),
                g.n_ribo_human.to_string(),
                g.n_ribo_mouse.to_string(),
            ],
        ));
    let h = g.n_mito_human >= 5;
    let m = g.n_mito_mouse >= 5;
    g.species = match (h, m) {
        (true, false) => Some(Species::Human),
        (false, true) => Some(Species::Mouse),
        (true, true) => {
            g.evidence
                .push("同时存在人与鼠的线粒体基因（可能是混合物种参考），无法判定".into());
            g.evidence_codes
                .push(crate::platform::detect::CodedText::new("species_mixed", []));
            None
        }
        (false, false) => {
            g.evidence
                .push("未找到 MT-/mt- 前缀的线粒体基因，无法判定物种".into());
            g.evidence_codes
                .push(crate::platform::detect::CodedText::new(
                    "species_unknown",
                    [],
                ));
            None
        }
    };
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feats(names: &[&str]) -> FeatureTable {
        FeatureTable {
            id: names.iter().map(|s| format!("ENSG_{s}")).collect(),
            name: names.iter().map(|s| s.to_string()).collect(),
            feature_type: vec!["Gene Expression".to_string(); names.len()],
            extra_columns: vec![],
        }
    }

    #[test]
    fn human_detection_and_sets() {
        let f = feats(&[
            "MT-CO1", "MT-CO2", "MT-ND1", "MT-ND2", "MT-CYB", "RPS6", "RPL13A", "RPS6KA1", "CD3E",
        ]);
        let g = guess_species(&f);
        assert_eq!(g.species, Some(Species::Human));
        let sets = Species::Human.rule().gene_sets(&f);
        assert_eq!(sets.mito, vec![0, 1, 2, 3, 4]);
        assert_eq!(sets.ribo, vec![5, 6]); // RPS6KA1 被排除
        let f2 = feats(&["RPLP0", "RPSA", "RPL3", "RPLX", "RPSAP1"]);
        assert_eq!(Species::Human.rule().gene_sets(&f2).ribo, vec![0, 1, 2]);
    }

    #[test]
    fn mouse_detection() {
        let f = feats(&[
            "mt-Co1", "mt-Co2", "mt-Nd1", "mt-Nd2", "mt-Cytb", "Rps6", "Cd3e",
        ]);
        assert_eq!(guess_species(&f).species, Some(Species::Mouse));
    }

    #[test]
    fn unknown_species() {
        let f = feats(&["GeneA", "GeneB"]);
        assert_eq!(guess_species(&f).species, None);
    }
}
