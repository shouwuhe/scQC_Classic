//! 估计器 C：线粒体法（仅在 Profile 启用时运行；SPEC §7.2.2；DESIGN §5.9）。
//!
//! 前提：干净的细胞核不含线粒体转录本，因此核中观测到的线粒体计数几乎全部来自环境污染：
//!   ρ_c = min(1, mito_c / (total_c · a_mito))，a_mito = 环境谱中线粒体基因的合计占比。
//! a_mito 过小（< 1e-3）时该估计器不可用。**未正式标定，仅作交叉参考**（报告注明）。

use serde::{Deserialize, Serialize};

use super::profile::AmbientProfile;
use crate::matrix::CountMatrix;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MitoResult {
    pub rho_per_cell: Vec<f64>,
    pub rho_global: f64,
    pub a_mito: f64,
    pub usable: bool,
    pub note_zh: String,
}

pub fn estimate_mito(
    cells: &CountMatrix,
    mito_genes: &[usize],
    profile: &AmbientProfile,
) -> MitoResult {
    let a_mito: f64 = mito_genes
        .iter()
        .map(|&g| profile.fraction.get(g).copied().unwrap_or(0.0))
        .sum();
    let n = cells.n_barcodes();
    if a_mito < 1e-3 || mito_genes.is_empty() {
        return MitoResult {
            rho_per_cell: vec![f64::NAN; n],
            rho_global: f64::NAN,
            a_mito,
            usable: false,
            note_zh: format!("环境谱中线粒体占比 {a_mito:.5} 过低，线粒体法不可用"),
        };
    }
    let mut is_mito = vec![false; cells.n_genes()];
    for &g in mito_genes {
        if g < is_mito.len() {
            is_mito[g] = true;
        }
    }
    let rho: Vec<f64> = (0..n)
        .map(|j| {
            let (idx, val) = cells.column(j);
            let mut total = 0u64;
            let mut mito = 0u64;
            for (&g, &v) in idx.iter().zip(val) {
                total += v as u64;
                if is_mito[g as usize] {
                    mito += v as u64;
                }
            }
            if total == 0 {
                0.0
            } else {
                (mito as f64 / (total as f64 * a_mito)).min(1.0)
            }
        })
        .collect();
    let rho_global = rho.iter().sum::<f64>() / n.max(1) as f64;
    MitoResult {
        rho_per_cell: rho,
        rho_global,
        a_mito,
        usable: true,
        note_zh: "线粒体法：假设干净核不含线粒体转录本，观测线粒体计数全部来自环境（未正式标定）"
            .into(),
    }
}
