//! `barcodes.tsv` 解析与 barcode 结构分析。
//!
//! 10x barcode 形如 `ACGT…-1`：碱基序列 + `-` + GEM well 后缀。
//! 多重化 Flex 中 cell barcode = GEM barcode(16 nt) + Probe barcode，序列部分比 16 nt 长
//! （SPEC §7.2.3）。**Probe Barcode 的长度随化学版本变化**：Flex v1 为 8 nt（总长 24），
//! Flex v2 / Apex 为 10 nt（总长 26，2026-09 用真实数据实测）。因此这里不写死长度，
//! 只要序列长于 16 nt 就把 16 nt 之后的部分当作 Probe Barcode，并限制在合理范围内。
//! 它是**启发式**证据，识别逻辑在 `platform::detect` 中综合判断。

use std::collections::BTreeMap;
use std::io::BufRead;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Result, ScqcError};

/// GEM 条形码长度（10x 3'/5'/Flex 单重）
pub const GEM_BARCODE_LEN: usize = 16;
/// Probe Barcode 长度的合理范围（Flex v1 = 8，Flex v2 / Apex = 10）
pub const PROBE_BARCODE_LEN_RANGE: std::ops::RangeInclusive<usize> = 6..=12;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BarcodeTable {
    pub barcode: Vec<String>,
    /// 每个 barcode 的 Probe Barcode 序列（序列长于 16 nt 时解析），否则 None。
    pub probe_barcode: Vec<Option<String>>,
}

/// barcode 结构统计，供平台识别。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BarcodeStructure {
    /// 序列部分长度 → 出现次数
    pub seq_len_counts: BTreeMap<usize, usize>,
    /// 不同 Probe Barcode 的数量
    pub n_probe_barcodes: usize,
    /// 各 Probe Barcode 的 barcode 数
    pub probe_barcode_counts: BTreeMap<String, usize>,
}

impl BarcodeTable {
    pub fn len(&self) -> usize {
        self.barcode.len()
    }

    pub fn is_empty(&self) -> bool {
        self.barcode.is_empty()
    }

    pub fn structure(&self) -> BarcodeStructure {
        let mut seq_len_counts = BTreeMap::new();
        let mut probe_barcode_counts: BTreeMap<String, usize> = BTreeMap::new();
        for (bc, pb) in self.barcode.iter().zip(&self.probe_barcode) {
            let seq_len = bc.split('-').next().map(|s| s.len()).unwrap_or(0);
            *seq_len_counts.entry(seq_len).or_insert(0) += 1;
            if let Some(p) = pb {
                *probe_barcode_counts.entry(p.clone()).or_insert(0) += 1;
            }
        }
        BarcodeStructure {
            seq_len_counts,
            n_probe_barcodes: probe_barcode_counts.len(),
            probe_barcode_counts,
        }
    }

    /// 从 barcode 字符串解析 Probe Barcode：序列长于 16 nt 时取 16 nt 之后的部分
    /// （Flex v1 为 8 nt、Flex v2 / Apex 为 10 nt）。
    pub fn parse_probe_barcode(barcode: &str) -> Option<String> {
        let seq = barcode.split('-').next()?;
        if seq.len() <= GEM_BARCODE_LEN {
            return None;
        }
        let probe_len = seq.len() - GEM_BARCODE_LEN;
        if !PROBE_BARCODE_LEN_RANGE.contains(&probe_len) {
            return None;
        }
        if !seq
            .bytes()
            .all(|b| matches!(b, b'A' | b'C' | b'G' | b'T' | b'N'))
        {
            return None;
        }
        Some(seq[GEM_BARCODE_LEN..].to_string())
    }

    /// Probe Barcode 的长度（全部一致时返回该长度）。
    pub fn probe_barcode_len(&self) -> Option<usize> {
        let mut len: Option<usize> = None;
        for p in self.probe_barcode.iter().flatten() {
            match len {
                None => len = Some(p.len()),
                Some(l) if l == p.len() => {}
                Some(_) => return None,
            }
        }
        len
    }
}

pub fn read_barcodes(path: &Path) -> Result<BarcodeTable> {
    let reader = super::open_maybe_gz(path)?;
    let mut table = BarcodeTable::default();
    for (i, line) in reader.lines().enumerate() {
        let line = line.map_err(|e| ScqcError::io_read(path, &e))?;
        let bc = line.trim();
        if bc.is_empty() {
            continue;
        }
        // 允许有额外制表符分隔列（某些工具会写），只取第一列
        let bc = bc.split('\t').next().unwrap_or(bc);
        if bc.contains(' ') {
            return Err(ScqcError::format(path, i + 1, "barcode 中不应包含空格"));
        }
        table
            .probe_barcode
            .push(BarcodeTable::parse_probe_barcode(bc));
        table.barcode.push(bc.to_string());
    }
    if table.is_empty() {
        return Err(ScqcError::format(path, 1, "barcodes 文件没有任何 barcode"));
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_barcode_parsing() {
        assert_eq!(
            BarcodeTable::parse_probe_barcode("AAACAAGCAAACAAGAACTTTAGG-1"),
            Some("ACTTTAGG".to_string())
        );
        assert_eq!(
            BarcodeTable::parse_probe_barcode("AAACCTGAGAAACCAT-1"),
            None
        );
    }
}
