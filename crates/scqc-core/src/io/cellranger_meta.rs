//! Cell Ranger `metrics_summary.csv` 解析（只用于平台识别与报告，缺失不报错）。
//!
//! 两种布局：
//! - `cellranger count`（GEX）：单行宽表，表头即指标名；
//! - `cellranger multi`（Flex 等）：长表，列含 `Metric Name` / `Metric Value`，
//!   另有 `Library Type`、`Grouped By`、`Group Name` 等。
//!
//! 统一解析为 (指标名, 值) 列表；不假设任何指标一定存在。

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{ErrorCode, Result, ScqcError};

use super::InputFile;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CellRangerMetrics {
    pub file: InputFile,
    /// `wide` 或 `long`
    pub layout: String,
    /// 指标名 → 值（长表中同名指标取第一个非空值）
    pub metrics: BTreeMap<String, String>,
    /// 长表中的 Library Type 取值集合（如 "Gene Expression"）
    pub library_types: Vec<String>,
}

impl CellRangerMetrics {
    /// 按不区分大小写的关键词查找指标值。
    pub fn find(&self, key_contains: &str) -> Option<(&String, &String)> {
        let k = key_contains.to_ascii_lowercase();
        self.metrics
            .iter()
            .find(|(name, _)| name.to_ascii_lowercase().contains(&k))
    }
}

pub fn read_metrics_summary(path: &Path) -> Result<CellRangerMetrics> {
    let reader = super::open_maybe_gz(path)?;
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(reader);
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| {
            ScqcError::new(ErrorCode::FormatInvalid, "metrics_summary.csv 表头无法解析")
                .with_detail(e.to_string())
        })?
        .iter()
        .map(|s| s.trim().to_string())
        .collect();

    let mut metrics = BTreeMap::new();
    let mut library_types = Vec::new();
    let name_col = headers.iter().position(|h| h == "Metric Name");
    let value_col = headers.iter().position(|h| h == "Metric Value");
    let layout;
    if let (Some(nc), Some(vc)) = (name_col, value_col) {
        layout = "long";
        let lib_col = headers.iter().position(|h| h == "Library Type");
        for rec in rdr.records() {
            let rec = rec.map_err(|e| {
                ScqcError::new(ErrorCode::FormatInvalid, "metrics_summary.csv 行无法解析")
                    .with_detail(e.to_string())
            })?;
            let name = rec.get(nc).unwrap_or("").trim();
            let value = rec.get(vc).unwrap_or("").trim();
            if !name.is_empty() && !value.is_empty() {
                metrics
                    .entry(name.to_string())
                    .or_insert_with(|| value.to_string());
            }
            if let Some(lc) = lib_col {
                let lt = rec.get(lc).unwrap_or("").trim();
                if !lt.is_empty() && !library_types.iter().any(|x: &String| x == lt) {
                    library_types.push(lt.to_string());
                }
            }
        }
    } else {
        layout = "wide";
        if let Some(rec) = rdr.records().next() {
            let rec = rec.map_err(|e| {
                ScqcError::new(
                    ErrorCode::FormatInvalid,
                    "metrics_summary.csv 数据行无法解析",
                )
                .with_detail(e.to_string())
            })?;
            for (h, v) in headers.iter().zip(rec.iter()) {
                if !h.is_empty() {
                    metrics.insert(h.clone(), v.trim().to_string());
                }
            }
        }
    }
    Ok(CellRangerMetrics {
        file: super::describe_file(path)?,
        layout: layout.to_string(),
        metrics,
        library_types,
    })
}
