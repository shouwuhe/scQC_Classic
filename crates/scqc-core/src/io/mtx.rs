//! Matrix Market（coordinate）读写。
//!
//! 依据：Matrix Market 格式规范（NIST）。Cell Ranger 写出的头部为
//! `%%MatrixMarket matrix coordinate integer general`；部分工具写 `real`，
//! 只要所有值为整数即接受。坐标为 1-based。

use std::io::BufRead;
use std::path::Path;

use crate::error::{ErrorCode, Result, ScqcError};
use crate::matrix::{CountMatrix, Triplet};

/// 读取 MTX 文件；`expect_genes` / `expect_barcodes` 来自 features / barcodes 文件，
/// 用于校验头部维度一致。
pub fn read_mtx(path: &Path, expect_genes: usize, expect_barcodes: usize) -> Result<CountMatrix> {
    let mut reader = super::open_maybe_gz(path)?;
    let mut line = String::new();
    let mut lineno = 0usize;

    // ---- 头部 ----
    let n = reader
        .read_line(&mut line)
        .map_err(|e| ScqcError::io_read(path, &e))?;
    lineno += 1;
    if n == 0 {
        return Err(ScqcError::format(path, 1, "文件为空"));
    }
    let header = line.trim();
    if !header.starts_with("%%MatrixMarket") {
        return Err(ScqcError::format(path, 1, "缺少 %%MatrixMarket 头部"));
    }
    let tokens: Vec<String> = header
        .split_whitespace()
        .map(|s| s.to_ascii_lowercase())
        .collect();
    if tokens.len() < 5 || tokens[1] != "matrix" || tokens[2] != "coordinate" {
        return Err(ScqcError::format(
            path,
            1,
            "只支持 coordinate 格式的稀疏矩阵",
        ));
    }
    let field = tokens[3].as_str();
    if field != "integer" && field != "real" {
        return Err(ScqcError::format(
            path,
            1,
            format!("数值类型 {field} 不支持，只接受 integer 或 real"),
        ));
    }
    if tokens[4] != "general" {
        return Err(ScqcError::format(path, 1, "只支持 general（非对称）矩阵"));
    }

    // ---- 跳过注释，读维度行 ----
    let (rows, cols, nnz) = loop {
        line.clear();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| ScqcError::io_read(path, &e))?;
        lineno += 1;
        if n == 0 {
            return Err(ScqcError::format(path, lineno, "缺少维度行"));
        }
        let t = line.trim();
        if t.is_empty() || t.starts_with('%') {
            continue;
        }
        let mut it = t.split_whitespace();
        let parse = |s: Option<&str>| -> Option<usize> { s.and_then(|s| s.parse::<usize>().ok()) };
        match (parse(it.next()), parse(it.next()), parse(it.next())) {
            (Some(r), Some(c), Some(z)) => break (r, c, z),
            _ => {
                return Err(ScqcError::format(
                    path,
                    lineno,
                    "维度行应为三个整数：基因数 barcode数 非零元数",
                ))
            }
        }
    };
    if rows != expect_genes {
        return Err(ScqcError::new(
            ErrorCode::InputMismatch,
            format!("matrix.mtx 声明 {rows} 个基因，但 features 文件有 {expect_genes} 行"),
        ));
    }
    if cols != expect_barcodes {
        return Err(ScqcError::new(
            ErrorCode::InputMismatch,
            format!("matrix.mtx 声明 {cols} 个 barcode，但 barcodes 文件有 {expect_barcodes} 行"),
        ));
    }
    if nnz > u32::MAX as usize {
        return Err(ScqcError::new(
            ErrorCode::FormatInvalid,
            "非零元数量超过 2^32，超出本工具支持范围",
        ));
    }

    // ---- 三元组 ----
    let mut triplets: Vec<Triplet> = Vec::with_capacity(nnz);
    loop {
        line.clear();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| ScqcError::io_read(path, &e))?;
        if n == 0 {
            break;
        }
        lineno += 1;
        let t = line.trim();
        if t.is_empty() || t.starts_with('%') {
            continue;
        }
        let mut it = t.split_ascii_whitespace();
        let r = it.next().and_then(|s| s.parse::<u64>().ok());
        let c = it.next().and_then(|s| s.parse::<u64>().ok());
        let v = it.next().and_then(parse_count);
        match (r, c, v) {
            (Some(r), Some(c), Some(v)) if r >= 1 && c >= 1 => {
                if r > rows as u64 || c > cols as u64 {
                    return Err(ScqcError::format(
                        path,
                        lineno,
                        format!("坐标 ({r}, {c}) 超出矩阵大小 {rows} × {cols}"),
                    ));
                }
                if v > 0 {
                    triplets.push(Triplet {
                        row: (r - 1) as u32,
                        col: (c - 1) as u32,
                        value: v,
                    });
                }
            }
            _ => {
                return Err(ScqcError::format(
                    path,
                    lineno,
                    "期望三个数：基因编号 barcode编号 计数（编号从 1 开始）",
                ))
            }
        }
    }
    if triplets.len() != nnz {
        tracing::warn!(
            declared = nnz,
            actual = triplets.len(),
            "matrix.mtx 声明的非零元数与实际行数不一致（零值行已忽略）"
        );
    }
    CountMatrix::from_triplets(rows, cols, triplets)
}

/// 读取 real/integer 的 MTX 为浮点三元组（供金标准比对非整数矩阵使用；不做维度校验）。
/// 返回 (基因数, barcode 数, (基因, barcode, 值))，索引从 0 起。
pub type RealEntries = Vec<(u32, u32, f32)>;

pub fn read_mtx_real(path: &Path) -> Result<(usize, usize, RealEntries)> {
    let mut reader = super::open_maybe_gz(path)?;
    let mut line = String::new();
    let mut lineno = 0usize;
    let mut dims: Option<(usize, usize)> = None;
    let mut out: Vec<(u32, u32, f32)> = Vec::new();
    loop {
        line.clear();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| ScqcError::io_read(path, &e))?;
        if n == 0 {
            break;
        }
        lineno += 1;
        let t = line.trim();
        if t.is_empty() || t.starts_with('%') {
            continue;
        }
        let mut it = t.split_ascii_whitespace();
        let a = it.next().and_then(|s| s.parse::<u64>().ok());
        let b = it.next().and_then(|s| s.parse::<u64>().ok());
        let c = it.next().and_then(|s| s.parse::<f64>().ok());
        match (dims, a, b, c) {
            (None, Some(r), Some(cc), Some(_)) => dims = Some((r as usize, cc as usize)),
            (Some(_), Some(r), Some(cc), Some(v)) if r >= 1 && cc >= 1 => {
                if v != 0.0 {
                    out.push(((r - 1) as u32, (cc - 1) as u32, v as f32));
                }
            }
            _ => {
                return Err(ScqcError::format(
                    path,
                    lineno,
                    "期望三个数：基因编号 barcode编号 数值",
                ))
            }
        }
    }
    let (r, c) = dims.ok_or_else(|| ScqcError::format(path, lineno, "缺少维度行"))?;
    Ok((r, c, out))
}

/// 解析计数：接受 "12"、"12.0"、"1.2e1"；非整数值报错（None）。
fn parse_count(s: &str) -> Option<u32> {
    if let Ok(v) = s.parse::<u32>() {
        return Some(v);
    }
    let f: f64 = s.parse().ok()?;
    if f < 0.0 || f.fract() != 0.0 || f > u32::MAX as f64 {
        return None;
    }
    Some(f as u32)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_count_variants() {
        assert_eq!(parse_count("12"), Some(12));
        assert_eq!(parse_count("12.0"), Some(12));
        assert_eq!(parse_count("1.2e1"), Some(12));
        assert_eq!(parse_count("1.5"), None);
        assert_eq!(parse_count("-1"), None);
    }
}
