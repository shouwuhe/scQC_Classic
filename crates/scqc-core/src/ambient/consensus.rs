//! 一致性判定、参考区间与校正建议（SPEC §8.2.3 / §8.3.2；DESIGN §5.10）。
//!
//! 【硬约束】必须报告全部估计值；两法相对差 ≥ 50% 判为分歧显著，默认不建议校正；
//! 污染度 > 20% 不推荐校正；无参考区间（Flex）时只报数值不做高低判定；置灰逻辑必须实现。

use serde::{Deserialize, Serialize};

use crate::platform::AmbientReference;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Agree,
    Disagree,
    /// 只有一个可用估计（另一估计器失败）
    Single,
    /// 两个估计器都不可用
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    Normal,
    Elevated,
    Severe,
    /// 无参考区间，不判定
    Unrated,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorrectionState {
    /// 置灰：不允许校正
    Greyed,
    /// 可用，默认关闭
    Available,
    /// 不推荐（严重污染或分歧）
    Discouraged,
    /// 无法判断（无参考区间）：可用但只给数值
    Unrated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Consensus {
    pub rho_soup: Option<f64>,
    pub rho_decontx: Option<f64>,
    pub rho_mito: Option<f64>,
    pub relative_diff: Option<f64>,
    pub verdict: Verdict,
    /// 一致时的区间 [min, max]；分歧时亦给出两值范围
    pub interval: Option<(f64, f64)>,
    pub rating: Rating,
    pub reference: Option<AmbientReference>,
    pub correction: CorrectionState,
    pub reason_zh: String,
    pub disagreement_reasons_zh: Vec<String>,
}

/// 两个估计都低于这个绝对水平时，不按"分歧"处理（相对差在低值区没有实际意义）。
pub const LOW_ABS: f64 = 0.05;

pub fn assess(
    rho_soup: Option<f64>,
    rho_decontx: Option<f64>,
    rho_mito: Option<f64>,
    reference: Option<AmbientReference>,
) -> Consensus {
    let ok = |v: Option<f64>| v.filter(|x| x.is_finite());
    let (a, b) = (ok(rho_soup), ok(rho_decontx));
    let (verdict, relative_diff, interval, level) = match (a, b) {
        (Some(x), Some(y)) => {
            let mean = (x + y) / 2.0;
            let rd = if mean > 0.0 {
                (x - y).abs() / mean
            } else {
                0.0
            };
            // 相对差在污染度都很低时会虚高（真实 Flex 4-plex 实测 0.5% vs 2.5%，相对差 133%，
            // 但两者都远低于任何校正阈值，判成"分歧"只会制造无意义的警示）。
            // 因此加一条绝对量的豁免：两个估计都 < LOW_ABS 时按"一致（都很低）"处理。
            let v = if rd < 0.5 || x.max(y) < LOW_ABS {
                Verdict::Agree
            } else {
                Verdict::Disagree
            };
            (v, Some(rd), Some((x.min(y), x.max(y))), Some(x.max(y)))
        }
        (Some(x), None) | (None, Some(x)) => (Verdict::Single, None, Some((x, x)), Some(x)),
        (None, None) => (Verdict::Unavailable, None, None, None),
    };

    let rating = match (level, reference) {
        (None, _) => Rating::Unavailable,
        (Some(_), None) => Rating::Unrated,
        (Some(l), Some(r)) => {
            if l > r.discourage_above {
                Rating::Severe
            } else if l >= r.grey_below {
                Rating::Elevated
            } else {
                Rating::Normal
            }
        }
    };

    let both_low = matches!((a, b), (Some(x), Some(y)) if x.max(y) < LOW_ABS);
    let disagreement_reasons_zh = vec![
        "(a) 聚类分辨率不合适，建议使用更粗的细胞群标签重算".to_string(),
        "(b) 不同细胞类型解离难易度差异大，导致污染在细胞间高度不均匀".to_string(),
        "(c) 存在与环境谱高度相似的细胞群".to_string(),
    ];

    let (correction, reason_zh) = match (verdict, rating) {
        (Verdict::Unavailable, _) => (
            CorrectionState::Greyed,
            "两个估计器均不可用，无法判断是否需要校正".to_string(),
        ),
        (Verdict::Disagree, _) => (
            CorrectionState::Discouraged,
            format!(
                "两种方法分歧显著（相对差 {:.0}%），不推荐校正（无论数值高低）",
                relative_diff.unwrap_or(0.0) * 100.0
            ),
        ),
        (_, Rating::Unrated) if both_low => (
            CorrectionState::Greyed,
            format!(
                "两个估计值都低于 {:.0}%，属于低污染；本平台虽无参考基准，但在这个水平上不建议校正",
                LOW_ABS * 100.0
            ),
        ),
        (_, Rating::Unrated) => (
            CorrectionState::Unrated,
            "本平台没有公开的环境 RNA 参考基准，只报告数值，不做高低判定".to_string(),
        ),
        (_, Rating::Severe) => (
            CorrectionState::Discouraged,
            "污染度显著高于参考范围（> 20%），计算校正可靠性下降，建议核查实验环节".to_string(),
        ),
        (_, Rating::Elevated) => (
            CorrectionState::Available,
            "污染度偏高，可选择校正，建议对比校正前后结果".to_string(),
        ),
        (_, Rating::Normal) => (
            CorrectionState::Greyed,
            "当前污染水平下校正弊大于利（所有校正方法在干净数据上都会过度校正）".to_string(),
        ),
        (_, Rating::Unavailable) => (CorrectionState::Greyed, "无可用估计".to_string()),
    };

    Consensus {
        rho_soup: a,
        rho_decontx: b,
        rho_mito: ok(rho_mito),
        relative_diff,
        verdict,
        interval,
        rating,
        reference,
        correction,
        reason_zh,
        disagreement_reasons_zh: if verdict == Verdict::Disagree {
            disagreement_reasons_zh
        } else {
            Vec::new()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sc_ref() -> AmbientReference {
        AmbientReference {
            mean: 0.0779,
            grey_below: 0.10,
            discourage_above: 0.20,
        }
    }

    #[test]
    fn agree_low_is_greyed() {
        let c = assess(Some(0.05), Some(0.06), None, Some(sc_ref()));
        assert_eq!(c.verdict, Verdict::Agree);
        assert_eq!(c.rating, Rating::Normal);
        assert_eq!(c.correction, CorrectionState::Greyed);
    }

    #[test]
    fn disagree_is_discouraged_regardless_of_level() {
        // SPEC §8.2.3 案例：1.0% vs 9.8%
        let c = assess(Some(0.010), Some(0.098), None, Some(sc_ref()));
        assert_eq!(c.verdict, Verdict::Disagree);
        assert_eq!(c.correction, CorrectionState::Discouraged);
        assert_eq!(c.disagreement_reasons_zh.len(), 3);
    }

    #[test]
    fn severe_and_elevated() {
        let c = assess(Some(0.25), Some(0.30), None, Some(sc_ref()));
        assert_eq!(c.rating, Rating::Severe);
        assert_eq!(c.correction, CorrectionState::Discouraged);
        let c = assess(Some(0.12), Some(0.15), None, Some(sc_ref()));
        assert_eq!(c.rating, Rating::Elevated);
        assert_eq!(c.correction, CorrectionState::Available);
    }

    /// 真实 Flex 4-plex 实测：0.5% vs 2.5%，相对差 133% 但都极低——
    /// 这种情况不按"分歧"处理，否则只会制造无意义的警示。
    #[test]
    fn both_low_is_not_a_disagreement() {
        let c = assess(Some(0.005), Some(0.025), None, None);
        assert_eq!(c.verdict, Verdict::Agree);
        assert_eq!(c.rating, Rating::Unrated);
        assert_eq!(c.correction, CorrectionState::Greyed);
        assert!(c.reason_zh.contains("低污染"));
        // 但只要有一个估计够高，仍然按分歧处理
        let c = assess(Some(0.005), Some(0.098), None, None);
        assert_eq!(c.verdict, Verdict::Disagree);
    }

    #[test]
    fn flex_is_unrated() {
        let c = assess(Some(0.30), Some(0.32), None, None);
        assert_eq!(c.rating, Rating::Unrated);
        assert_eq!(c.correction, CorrectionState::Unrated);
    }
}
