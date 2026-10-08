//! 绘图数据文件（SPEC §10.3【硬约束】：大数组不走 JSON，写 `plots/*.bin` + manifest）。
//!
//! 每个 `.bin` 为小端 `f32` 原始字节；`plots/manifest.json` 描述名称、形状、列名。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, ScqcError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlotEntry {
    pub name: String,
    pub file: String,
    pub dtype: String,
    pub shape: Vec<usize>,
    pub byte_order: String,
    pub columns: Vec<String>,
    pub description_zh: String,
}

pub struct PlotWriter {
    dir: PathBuf,
    entries: Vec<PlotEntry>,
}

impl PlotWriter {
    pub fn new(output_dir: &Path) -> Result<Self> {
        let dir = output_dir.join("plots");
        fs::create_dir_all(&dir).map_err(|e| ScqcError::io_write(&dir, &e))?;
        Ok(Self {
            dir,
            entries: Vec::new(),
        })
    }

    /// 写一个 n × k 的 f32 矩阵（按行主序）。
    pub fn write_f32(
        &mut self,
        name: &str,
        columns: &[&str],
        rows: usize,
        data: &[f32],
        description_zh: &str,
    ) -> Result<()> {
        let k = columns.len();
        if data.len() != rows * k {
            return Err(ScqcError::internal(format!(
                "绘图数据 {name} 尺寸不符：{} != {rows}×{k}",
                data.len()
            )));
        }
        let file = format!("{name}.bin");
        let path = self.dir.join(&file);
        let mut bytes = Vec::with_capacity(data.len() * 4);
        for v in data {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        let mut f = fs::File::create(&path).map_err(|e| ScqcError::io_write(&path, &e))?;
        f.write_all(&bytes)
            .map_err(|e| ScqcError::io_write(&path, &e))?;
        self.entries.push(PlotEntry {
            name: name.to_string(),
            file,
            dtype: "f32".into(),
            shape: vec![rows, k],
            byte_order: "le".into(),
            columns: columns.iter().map(|s| s.to_string()).collect(),
            description_zh: description_zh.to_string(),
        });
        Ok(())
    }

    pub fn finish(self) -> Result<()> {
        crate::io::writer::write_json(&self.dir.join("manifest.json"), &self.entries)
    }
}
