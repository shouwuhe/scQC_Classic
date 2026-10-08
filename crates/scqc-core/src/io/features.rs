//! `features.tsv`（Cell Ranger ≥ 3）/ `genes.tsv`（Cell Ranger 2）解析。
//!
//! 列：id、name、feature_type（v3 起）、以及之后的任意列（Flex 探针集信息等），
//! 额外列原样保留供平台识别使用（DESIGN §3.2）。

use std::io::BufRead;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Result, ScqcError};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FeatureTable {
    pub id: Vec<String>,
    pub name: Vec<String>,
    /// 缺失（v2 genes.tsv）时全为 "Gene Expression"
    pub feature_type: Vec<String>,
    /// 第 4 列及之后：(列序号, 各行的值)。无表头，列名以 `col4`、`col5` 命名。
    pub extra_columns: Vec<(String, Vec<String>)>,
}

impl FeatureTable {
    pub fn len(&self) -> usize {
        self.id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.id.is_empty()
    }

    /// 按给定索引取子集（顺序按 `rows` 给定）。
    pub fn select(&self, rows: &[usize]) -> FeatureTable {
        let pick = |v: &Vec<String>| -> Vec<String> {
            rows.iter()
                .map(|&i| v.get(i).cloned().unwrap_or_default())
                .collect()
        };
        FeatureTable {
            id: pick(&self.id),
            name: pick(&self.name),
            feature_type: pick(&self.feature_type),
            extra_columns: self
                .extra_columns
                .iter()
                .map(|(k, v)| (k.clone(), pick(v)))
                .collect(),
        }
    }

    /// 仅 Gene Expression 类型的特征索引（排除 Antibody Capture 等）。
    pub fn gene_expression_indices(&self) -> Vec<usize> {
        self.feature_type
            .iter()
            .enumerate()
            .filter(|(_, t)| t.as_str() == "Gene Expression")
            .map(|(i, _)| i)
            .collect()
    }
}

pub fn read_features(path: &Path) -> Result<FeatureTable> {
    let reader = super::open_maybe_gz(path)?;
    let mut table = FeatureTable::default();
    let mut n_extra: Option<usize> = None;
    for (i, line) in reader.lines().enumerate() {
        let line = line.map_err(|e| ScqcError::io_read(path, &e))?;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 2 {
            return Err(ScqcError::format(
                path,
                i + 1,
                "至少需要两列：基因 ID 与基因名",
            ));
        }
        table.id.push(cols[0].to_string());
        table.name.push(cols[1].to_string());
        table.feature_type.push(
            cols.get(2)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "Gene Expression".to_string()),
        );
        let extras = if cols.len() > 3 { &cols[3..] } else { &[] };
        match n_extra {
            None => {
                n_extra = Some(extras.len());
                for (k, v) in extras.iter().enumerate() {
                    table
                        .extra_columns
                        .push((format!("col{}", k + 4), vec![v.to_string()]));
                }
            }
            Some(n) => {
                if extras.len() != n {
                    return Err(ScqcError::format(
                        path,
                        i + 1,
                        format!("列数与前面的行不一致（{} vs {}）", cols.len(), n + 3),
                    ));
                }
                for (k, v) in extras.iter().enumerate() {
                    table.extra_columns[k].1.push(v.to_string());
                }
            }
        }
    }
    if table.is_empty() {
        return Err(ScqcError::format(path, 1, "features 文件没有任何基因"));
    }
    Ok(table)
}
