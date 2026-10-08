//! 稀疏计数矩阵（DESIGN §3.1）。
//!
//! 约定：**行 = 基因，列 = barcode**，按列（CSC）存储，逐细胞操作最快。
//! 计数类型 `u32`；归一化后的值放在单独的浮点矩阵里，**永不覆盖原始计数**（C7）。

use serde::{Deserialize, Serialize};

use crate::error::{ErrorCode, Result, ScqcError};

/// 基因 × barcode 的 CSC 计数矩阵。
///
/// 自持 CSC 三数组，结构在构建时校验；索引用 `u32`（nnz 与维度均 < 2^32）。
#[derive(Debug, Clone)]
pub struct CountMatrix {
    n_genes: usize,
    n_barcodes: usize,
    indptr: Vec<u32>,
    indices: Vec<u32>,
    data: Vec<u32>,
}

/// 三元组（行、列、值），用于从 MTX 构建矩阵。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Triplet {
    pub row: u32,
    pub col: u32,
    pub value: u32,
}

/// 矩阵的基本统计，写入 `raw_input_summary.json`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MatrixSummary {
    pub n_genes: usize,
    pub n_barcodes: usize,
    pub nnz: usize,
    pub total_umi: u64,
}

impl CountMatrix {
    /// 由（可能无序、可能含重复）三元组构建 CSC 矩阵。重复坐标的值相加。
    ///
    /// 内存：三元组排序在原地进行，峰值 ≈ 三元组 12 B × nnz + CSC 8 B × nnz。
    pub fn from_triplets(
        n_genes: usize,
        n_barcodes: usize,
        mut triplets: Vec<Triplet>,
    ) -> Result<Self> {
        for t in &triplets {
            if t.row as usize >= n_genes || t.col as usize >= n_barcodes {
                return Err(ScqcError::new(
                    ErrorCode::FormatInvalid,
                    format!(
                        "矩阵坐标越界：基因 {} / barcode {}，而矩阵大小为 {} × {}",
                        t.row + 1,
                        t.col + 1,
                        n_genes,
                        n_barcodes
                    ),
                ));
            }
        }
        // 按 (列, 行) 排序 —— 排序是确定性的，与线程无关。
        triplets.sort_unstable_by_key(|t| (t.col, t.row));

        let mut indptr: Vec<u32> = Vec::with_capacity(n_barcodes + 1);
        let mut indices: Vec<u32> = Vec::with_capacity(triplets.len());
        let mut data: Vec<u32> = Vec::with_capacity(triplets.len());
        indptr.push(0);
        let mut current_col: u32 = 0;
        for t in &triplets {
            while current_col < t.col {
                indptr.push(indices.len() as u32);
                current_col += 1;
            }
            let col_start = indptr[current_col as usize] as usize;
            let dup = indices.len() > col_start && indices[indices.len() - 1] == t.row;
            if dup {
                // 重复坐标：累加（MTX 规范允许）
                if let Some(last) = data.last_mut() {
                    *last = last.saturating_add(t.value);
                }
            } else {
                indices.push(t.row);
                data.push(t.value);
            }
        }
        while indptr.len() < n_barcodes + 1 {
            indptr.push(indices.len() as u32);
        }
        drop(triplets);

        if indptr.len() != n_barcodes + 1 || indices.len() != data.len() {
            return Err(ScqcError::internal("稀疏矩阵结构校验失败（这不应发生）"));
        }
        Ok(Self {
            n_genes,
            n_barcodes,
            indptr,
            indices,
            data,
        })
    }

    pub fn n_genes(&self) -> usize {
        self.n_genes
    }

    pub fn n_barcodes(&self) -> usize {
        self.n_barcodes
    }

    pub fn nnz(&self) -> usize {
        self.indices.len()
    }

    /// 第 `col` 列（一个 barcode）的非零元：(基因索引, 计数)。越界返回空切片。
    pub fn column(&self, col: usize) -> (&[u32], &[u32]) {
        if col >= self.n_barcodes {
            return (&[], &[]);
        }
        let start = self.indptr[col] as usize;
        let end = self.indptr[col + 1] as usize;
        (&self.indices[start..end], &self.data[start..end])
    }

    /// 每个 barcode 的总 UMI。
    pub fn column_sums(&self) -> Vec<u64> {
        (0..self.n_barcodes())
            .map(|j| self.column(j).1.iter().map(|&v| v as u64).sum())
            .collect()
    }

    /// 每个 barcode 的非零基因数。
    pub fn column_nnz(&self) -> Vec<u32> {
        (0..self.n_barcodes())
            .map(|j| self.column(j).0.len() as u32)
            .collect()
    }

    /// 每个基因在所有 barcode 上的计数和。
    pub fn row_sums(&self) -> Vec<u64> {
        let mut sums = vec![0u64; self.n_genes()];
        for j in 0..self.n_barcodes() {
            let (idx, val) = self.column(j);
            for (&g, &v) in idx.iter().zip(val) {
                sums[g as usize] += v as u64;
            }
        }
        sums
    }

    /// 选取一部分列（barcode）组成新矩阵；列顺序按 `cols` 给定。
    pub fn select_columns(&self, cols: &[usize]) -> Result<Self> {
        let mut triplets = Vec::new();
        for (new_j, &j) in cols.iter().enumerate() {
            if j >= self.n_barcodes() {
                return Err(ScqcError::internal(format!(
                    "选取的 barcode 列 {j} 超出范围 {}",
                    self.n_barcodes()
                )));
            }
            let (idx, val) = self.column(j);
            for (&g, &v) in idx.iter().zip(val) {
                triplets.push(Triplet {
                    row: g,
                    col: new_j as u32,
                    value: v,
                });
            }
        }
        Self::from_triplets(self.n_genes(), cols.len(), triplets)
    }

    /// 选取一部分行（基因）组成新矩阵；行顺序按 `rows` 给定。
    /// 返回 (新矩阵, 被丢弃的 UMI 总数)——后者用于向用户说明"丢了多少信号"。
    pub fn select_rows(&self, rows: &[usize]) -> Result<(Self, u64)> {
        let mut new_index = vec![u32::MAX; self.n_genes()];
        for (new_g, &g) in rows.iter().enumerate() {
            if g >= self.n_genes() {
                return Err(ScqcError::internal(format!(
                    "选取的基因行 {g} 超出范围 {}",
                    self.n_genes()
                )));
            }
            new_index[g] = new_g as u32;
        }
        let mut triplets = Vec::with_capacity(self.nnz());
        let mut dropped_umi = 0u64;
        for j in 0..self.n_barcodes() {
            let (idx, val) = self.column(j);
            for (&g, &v) in idx.iter().zip(val) {
                let ng = new_index[g as usize];
                if ng == u32::MAX {
                    dropped_umi += v as u64;
                } else {
                    triplets.push(Triplet {
                        row: ng,
                        col: j as u32,
                        value: v,
                    });
                }
            }
        }
        Ok((
            Self::from_triplets(rows.len(), self.n_barcodes(), triplets)?,
            dropped_umi,
        ))
    }

    pub fn summary(&self) -> MatrixSummary {
        MatrixSummary {
            n_genes: self.n_genes(),
            n_barcodes: self.n_barcodes(),
            nnz: self.nnz(),
            total_umi: self.column_sums().iter().sum(),
        }
    }

    /// 以 (列, 行, 值) 顺序遍历全部非零元，用于写出 MTX。
    pub fn iter_triplets(&self) -> impl Iterator<Item = Triplet> + '_ {
        (0..self.n_barcodes()).flat_map(move |j| {
            let (idx, val) = self.column(j);
            idx.iter().zip(val).map(move |(&g, &v)| Triplet {
                row: g,
                col: j as u32,
                value: v,
            })
        })
    }

    /// CSC 原始三数组 (indptr, indices, data)，供后续线代模块构造 sprs 视图。
    pub fn csc_parts(&self) -> (&[u32], &[u32], &[u32]) {
        (&self.indptr, &self.indices, &self.data)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn tiny() -> CountMatrix {
        // 3 基因 × 2 barcode
        let t = vec![
            Triplet {
                row: 2,
                col: 1,
                value: 5,
            },
            Triplet {
                row: 0,
                col: 0,
                value: 1,
            },
            Triplet {
                row: 1,
                col: 0,
                value: 2,
            },
            Triplet {
                row: 0,
                col: 0,
                value: 3,
            }, // 重复坐标，应累加为 4
        ];
        CountMatrix::from_triplets(3, 2, t).unwrap()
    }

    #[test]
    fn builds_and_sums() {
        let m = tiny();
        assert_eq!(m.n_genes(), 3);
        assert_eq!(m.n_barcodes(), 2);
        assert_eq!(m.nnz(), 3);
        assert_eq!(m.column_sums(), vec![6, 5]);
        assert_eq!(m.column_nnz(), vec![2, 1]);
        assert_eq!(m.row_sums(), vec![4, 2, 5]);
        assert_eq!(m.column(0), (&[0u32, 1][..], &[4u32, 2][..]));
    }

    #[test]
    fn empty_trailing_columns() {
        let m = CountMatrix::from_triplets(
            2,
            3,
            vec![Triplet {
                row: 0,
                col: 0,
                value: 1,
            }],
        )
        .unwrap();
        assert_eq!(m.column_sums(), vec![1, 0, 0]);
    }

    #[test]
    fn out_of_range_is_error() {
        let e = CountMatrix::from_triplets(
            2,
            2,
            vec![Triplet {
                row: 5,
                col: 0,
                value: 1,
            }],
        );
        assert!(e.is_err());
    }

    #[test]
    fn select_rows_subsets_and_counts_dropped() {
        let m = tiny();
        let (sub, dropped) = m.select_rows(&[0]).unwrap();
        assert_eq!(sub.n_genes(), 1);
        assert_eq!(sub.n_barcodes(), m.n_barcodes());
        let kept: u64 = sub.column_sums().iter().sum();
        let all: u64 = m.column_sums().iter().sum();
        assert_eq!(kept + dropped, all);
        assert!(m.select_rows(&[99]).is_err());
    }

    #[test]
    fn select_columns_reorders() {
        let m = tiny().select_columns(&[1, 0]).unwrap();
        assert_eq!(m.column_sums(), vec![5, 6]);
    }
}
