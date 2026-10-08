//! 运行参数（DESIGN §8）。`null`/`None` 表示"由 Platform Profile 提供"。
//!
//! `run_params.json` = 解析并填充默认值后的完整版本 + 版本信息，保证可复现（SPEC §14.5）。

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{ErrorCode, Result, ScqcError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RunParams {
    pub seed: u64,
    /// 0 = 自动
    pub threads: usize,
    pub cell_calling: CellCallingParams,
    pub embed: EmbedParams,
    pub ambient: AmbientParams,
    pub ddqc: DdqcParams,
    pub doublet: DoubletParams,
    pub correction: CorrectionParams,
    pub output: OutputParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CellCallingParams {
    pub niters: u32,
    pub fdr: f64,
    pub lower: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbedParams {
    pub n_hvg: usize,
    pub n_pcs: usize,
    pub k: usize,
    pub res_fine: f64,
    pub res_coarse: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AmbientParams {
    pub umi_upper_bound: Option<u32>,
    /// DecontX 迭代上限。实测 500 次在三份真实数据上都没收敛（估计值已稳定到小数点后 3 位，
    /// 但会一直报"未收敛"）；3000 次时肾 1273、PBMC 4-plex 1556、PBMC 3k 874 次收敛，
    /// 估计值变化 ≤ 0.0002。
    pub decontx_max_iter: u32,
    pub decontx_tol: f64,
    /// SoupX 式估计里"算作真实表达"的宽容倍数：λ = 该值 × ρ̂ × 期望环境计数。
    /// 越小排除得越狠、ρ 越低；默认值由 PBMC 3k 与 SoupX 的对照标定。
    pub soup_excl_factor: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DdqcParams {
    pub mad_multiplier: f64,
    pub coarse_min_genes: u32,
    pub coarse_max_mito: f64,
    /// 严格模式：固定阈值（线粒体 < 10%、基因数 > 500）作为硬性下限参与过滤，而不只是对照
    pub fixed_thresholds_are_hard: bool,
    pub fixed_min_genes: u32,
    pub fixed_max_mito: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DoubletParams {
    #[serde(rename = "pN")]
    pub p_n: f64,
    pub ntop: usize,
    pub n_loaded: Option<usize>,
    pub rate_per_1000: Option<f64>,
    pub remove_borderline: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CorrectionParams {
    /// "none" | "soup" | "decontx"
    pub method: String,
    /// true：即使步骤 5 的建议为"置灰 / 不推荐"也执行校正（会发出 warning 并写入报告）
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputParams {
    pub gzip: bool,
    pub write_plots: bool,
}

impl Default for RunParams {
    fn default() -> Self {
        Self {
            seed: crate::rng::DEFAULT_SEED,
            threads: 0,
            cell_calling: CellCallingParams::default(),
            embed: EmbedParams::default(),
            ambient: AmbientParams::default(),
            ddqc: DdqcParams::default(),
            doublet: DoubletParams::default(),
            correction: CorrectionParams::default(),
            output: OutputParams::default(),
        }
    }
}

impl Default for CellCallingParams {
    fn default() -> Self {
        // SPEC §8.1.2
        Self {
            niters: 10_000,
            fdr: 0.01,
            lower: None,
        }
    }
}

impl Default for EmbedParams {
    fn default() -> Self {
        // DESIGN §5.5
        Self {
            n_hvg: 2000,
            n_pcs: 50,
            k: 20,
            res_fine: 1.3,
            res_coarse: 0.3,
        }
    }
}

impl Default for AmbientParams {
    fn default() -> Self {
        // DESIGN §5.8
        Self {
            umi_upper_bound: None,
            decontx_max_iter: 3000,
            decontx_tol: 0.001,
            soup_excl_factor: 1.5,
        }
    }
}

impl Default for DdqcParams {
    fn default() -> Self {
        // SPEC §8.4
        Self {
            mad_multiplier: 2.0,
            coarse_min_genes: 100,
            coarse_max_mito: 0.8,
            fixed_thresholds_are_hard: true,
            fixed_min_genes: 500,
            fixed_max_mito: 0.10,
        }
    }
}

impl Default for DoubletParams {
    fn default() -> Self {
        // SPEC §8.5
        Self {
            p_n: 0.25,
            ntop: 500,
            n_loaded: None,
            rate_per_1000: None,
            // 严格模式（2026-09-11 负责人定的口径）：边界双细胞（只有一个方法判定）也剔除。
            // 双细胞判定本身带随机性，边界那部分最不可信，留着反而污染下游解释。
            remove_borderline: true,
        }
    }
}

impl Default for CorrectionParams {
    fn default() -> Self {
        Self {
            method: "none".into(),
            force: false,
        }
    }
}

impl Default for OutputParams {
    fn default() -> Self {
        Self {
            gzip: true,
            write_plots: true,
        }
    }
}

impl RunParams {
    pub fn from_file(path: &Path) -> Result<Self> {
        let s = std::fs::read_to_string(path).map_err(|e| ScqcError::io_read(path, &e))?;
        let p: RunParams = serde_json::from_str(&s).map_err(|e| {
            ScqcError::new(
                ErrorCode::InvalidParam,
                "参数文件不是合法的 JSON 或含未知字段",
            )
            .with_detail(e.to_string())
        })?;
        p.validate()?;
        Ok(p)
    }

    pub fn validate(&self) -> Result<()> {
        let bad = |msg: &str| Err(ScqcError::new(ErrorCode::InvalidParam, msg.to_string()));
        if !(0.0..1.0).contains(&self.cell_calling.fdr) {
            return bad("cell_calling.fdr 必须在 0 与 1 之间");
        }
        if !(0.0..1.0).contains(&self.doublet.p_n) {
            return bad("doublet.pN 必须在 0 与 1 之间");
        }
        if !["none", "soup", "decontx"].contains(&self.correction.method.as_str()) {
            return bad("correction.method 只能是 none / soup / decontx");
        }
        if self.ddqc.mad_multiplier <= 0.0 {
            return bad("ddqc.mad_multiplier 必须大于 0");
        }
        Ok(())
    }
}

/// `run_params.json` 的完整内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub scqc_version: String,
    pub git_hash: String,
    pub started_at_utc: String,
    pub platform_id: String,
    pub params: RunParams,
}
