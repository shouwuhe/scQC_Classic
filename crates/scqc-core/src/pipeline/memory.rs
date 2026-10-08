//! 内存预检（SPEC §10.4【硬约束】；DESIGN §9.6）。
//!
//! 估算 = raw 三元组峰值 + 两份 CSC + HVG 稠密矩阵 + DoubletFinder 合并矩阵
//!        + Monte Carlo 缓冲 + 固定余量，与系统可用内存比较。
//! 公式中的常数是保守上界；随着各步实现落地会在 PROGRESS.md 中记录实测校准。

use serde::{Deserialize, Serialize};

use crate::error::{ErrorCode, Result, ScqcError};

/// 估算输入。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MemoryInput {
    pub raw_nnz: usize,
    pub raw_barcodes: usize,
    pub filtered_nnz: usize,
    pub n_cells: usize,
    pub n_genes: usize,
    pub n_hvg: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryEstimate {
    pub estimated_bytes: u64,
    pub available_bytes: u64,
    pub total_bytes: u64,
    /// 本机在当前基因数/稀疏度下建议的最大细胞数
    pub recommended_max_cells: usize,
    pub breakdown: Vec<(String, u64)>,
}

const FIXED_MARGIN: u64 = 1_500 * 1024 * 1024; // 1.5 GB：进程基线、报告、临时缓冲

pub fn estimate(input: &MemoryInput) -> (u64, Vec<(String, u64)>) {
    let triplets = input.raw_nnz as u64 * 12; // 读取时的三元组 (u32,u32,u32)
    let csc_raw = input.raw_nnz as u64 * 8 + input.raw_barcodes as u64 * 4;
    let csc_filtered = input.filtered_nnz as u64 * 8 * 2; // CSC + CSR 副本
    let hvg_dense = input.n_cells as u64 * input.n_hvg as u64 * 4;
    let df_dense = (input.n_cells as f64 * 1.25) as u64 * input.n_hvg as u64 * 4;
    let mc_buffers = 64 * 1024 * 1024; // EmptyDrops 每线程采样缓冲的保守上界
    let per_cell_tables = input.n_cells as u64 * 512; // 各类逐细胞指标、kNN 表
    let breakdown = vec![
        ("raw 三元组读取峰值".to_string(), triplets),
        ("raw 矩阵 CSC".to_string(), csc_raw),
        ("filtered 矩阵 CSC+CSR".to_string(), csc_filtered),
        ("HVG 稠密矩阵".to_string(), hvg_dense),
        ("DoubletFinder 合并矩阵".to_string(), df_dense),
        ("Monte Carlo 缓冲".to_string(), mc_buffers),
        ("逐细胞表".to_string(), per_cell_tables),
        ("固定余量".to_string(), FIXED_MARGIN),
    ];
    // 三元组与 CSC 不同时存在峰值：取 max(三元组+CSC_raw, 后续)；保守起见直接求和。
    let total = breakdown.iter().map(|(_, b)| *b).sum();
    (total, breakdown)
}

/// 查询系统内存并做预检。超出可用内存 → `OOM_RISK` 错误。
pub fn precheck(input: &MemoryInput) -> Result<MemoryEstimate> {
    let (estimated, breakdown) = estimate(input);
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let available = sys.available_memory();
    let total = sys.total_memory();
    let est = MemoryEstimate {
        estimated_bytes: estimated,
        available_bytes: available,
        total_bytes: total,
        recommended_max_cells: recommend_max_cells(input, available),
        breakdown,
    };
    if estimated > available {
        return Err(ScqcError::new(
            ErrorCode::OomRisk,
            format!(
                "预估内存需求 {:.1} GB，超出本机当前可用内存 {:.1} GB（总内存 {:.1} GB）。本机建议不超过约 {} 万细胞",
                gb(estimated),
                gb(available),
                gb(total),
                est.recommended_max_cells / 10_000
            ),
        ));
    }
    Ok(est)
}

fn gb(b: u64) -> f64 {
    b as f64 / (1024.0 * 1024.0 * 1024.0)
}

/// 反解：在同样的基因数 / 每细胞非零数下，可用内存能容纳多少细胞。
fn recommend_max_cells(input: &MemoryInput, available: u64) -> usize {
    if input.n_cells == 0 {
        return 0;
    }
    let (base, _) = estimate(&MemoryInput {
        n_cells: 0,
        filtered_nnz: 0,
        ..*input
    });
    if available <= base {
        return 0;
    }
    let (one_step, _) = estimate(input);
    let per_cell = (one_step.saturating_sub(base)) as f64 / input.n_cells as f64;
    if per_cell <= 0.0 {
        return usize::MAX;
    }
    ((available - base) as f64 / per_cell) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_is_monotone_in_cells() {
        let a = MemoryInput {
            raw_nnz: 10_000_000,
            raw_barcodes: 700_000,
            filtered_nnz: 5_000_000,
            n_cells: 5_000,
            n_genes: 36_000,
            n_hvg: 2_000,
        };
        let b = MemoryInput {
            n_cells: 50_000,
            filtered_nnz: 50_000_000,
            ..a
        };
        assert!(estimate(&b).0 > estimate(&a).0);
        // 5 万细胞的估算应低于 8 GB（SPEC §13 峰值内存目标）
        assert!(estimate(&b).0 < 8 * 1024 * 1024 * 1024);
    }
}
