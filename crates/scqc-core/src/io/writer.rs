//! 写出 10x MTX 目录与 JSON 文件。
//!
//! C7：只写新目录，绝不覆盖输入。若目标目录已存在且非空，报错而非覆盖。

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use serde::Serialize;

use crate::error::{ErrorCode, Result, ScqcError};
use crate::io::{BarcodeTable, FeatureTable};
use crate::matrix::CountMatrix;

/// 创建输出目录；已存在且非空 → 报错（C7）。
pub fn create_output_dir(dir: &Path) -> Result<()> {
    if dir.exists() {
        let mut entries = fs::read_dir(dir).map_err(|e| ScqcError::io_read(dir, &e))?;
        if entries.next().is_some() {
            return Err(ScqcError::new(
                ErrorCode::IoWrite,
                format!(
                    "输出目录 {} 已存在且不为空。为保护已有数据，请指定一个新目录",
                    dir.file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default()
                ),
            )
            .with_detail(dir.display().to_string()));
        }
    }
    fs::create_dir_all(dir).map_err(|e| ScqcError::io_write(dir, &e))
}

/// 写出一个 MTX 目录（gzip 可选）。
pub fn write_matrix_dir(
    dir: &Path,
    matrix: &CountMatrix,
    features: &FeatureTable,
    barcodes: &BarcodeTable,
    gzip: bool,
) -> Result<()> {
    if features.len() != matrix.n_genes() || barcodes.len() != matrix.n_barcodes() {
        return Err(ScqcError::internal(format!(
            "写出矩阵时维度不一致：{}×{} vs features {} / barcodes {}",
            matrix.n_genes(),
            matrix.n_barcodes(),
            features.len(),
            barcodes.len()
        )));
    }
    fs::create_dir_all(dir).map_err(|e| ScqcError::io_write(dir, &e))?;
    let ext = if gzip { ".gz" } else { "" };

    // matrix.mtx
    {
        let p = dir.join(format!("matrix.mtx{ext}"));
        let mut w = open_writer(&p, gzip)?;
        writeln!(w, "%%MatrixMarket matrix coordinate integer general")
            .and_then(|_| {
                writeln!(
                    w,
                    "%metadata_json: {{\"software_version\": \"scqc-{}\"}}",
                    crate::VERSION
                )
            })
            .and_then(|_| {
                writeln!(
                    w,
                    "{} {} {}",
                    matrix.n_genes(),
                    matrix.n_barcodes(),
                    matrix.nnz()
                )
            })
            .map_err(|e| ScqcError::io_write(&p, &e))?;
        for t in matrix.iter_triplets() {
            writeln!(w, "{} {} {}", t.row + 1, t.col + 1, t.value)
                .map_err(|e| ScqcError::io_write(&p, &e))?;
        }
        w.flush().map_err(|e| ScqcError::io_write(&p, &e))?;
    }
    write_features_barcodes(dir, features, barcodes, gzip)
}

/// 写出一个非整数（real）MTX 目录：`entries` 为 (基因, barcode, 值)，按列升序、列内按基因升序。
/// `precision` 为小数位数（写 README 说明非整数来源）。
#[allow(clippy::too_many_arguments)]
pub fn write_real_matrix_dir(
    dir: &Path,
    n_genes: usize,
    n_barcodes: usize,
    entries: &[(u32, u32, f32)],
    features: &FeatureTable,
    barcodes: &BarcodeTable,
    gzip: bool,
    precision: usize,
) -> Result<()> {
    if features.len() != n_genes || barcodes.len() != n_barcodes {
        return Err(ScqcError::internal(format!(
            "写出矩阵时维度不一致：{n_genes}×{n_barcodes} vs features {} / barcodes {}",
            features.len(),
            barcodes.len()
        )));
    }
    fs::create_dir_all(dir).map_err(|e| ScqcError::io_write(dir, &e))?;
    let ext = if gzip { ".gz" } else { "" };
    {
        let p = dir.join(format!("matrix.mtx{ext}"));
        let mut w = open_writer(&p, gzip)?;
        writeln!(w, "%%MatrixMarket matrix coordinate real general")
            .and_then(|_| {
                writeln!(
                    w,
                    "%metadata_json: {{\"software_version\": \"scqc-{}\"}}",
                    crate::VERSION
                )
            })
            .and_then(|_| writeln!(w, "{} {} {}", n_genes, n_barcodes, entries.len()))
            .map_err(|e| ScqcError::io_write(&p, &e))?;
        for &(g, c, v) in entries {
            writeln!(w, "{} {} {:.prec$}", g + 1, c + 1, v, prec = precision)
                .map_err(|e| ScqcError::io_write(&p, &e))?;
        }
        w.flush().map_err(|e| ScqcError::io_write(&p, &e))?;
    }
    write_features_barcodes(dir, features, barcodes, gzip)
}

fn write_features_barcodes(
    dir: &Path,
    features: &FeatureTable,
    barcodes: &BarcodeTable,
    gzip: bool,
) -> Result<()> {
    let ext = if gzip { ".gz" } else { "" };
    // features.tsv
    {
        let p = dir.join(format!("features.tsv{ext}"));
        let mut w = open_writer(&p, gzip)?;
        for i in 0..features.len() {
            let mut line = format!(
                "{}\t{}\t{}",
                features.id[i], features.name[i], features.feature_type[i]
            );
            for (_, col) in &features.extra_columns {
                line.push('\t');
                line.push_str(&col[i]);
            }
            writeln!(w, "{line}").map_err(|e| ScqcError::io_write(&p, &e))?;
        }
        w.flush().map_err(|e| ScqcError::io_write(&p, &e))?;
    }
    // barcodes.tsv
    {
        let p = dir.join(format!("barcodes.tsv{ext}"));
        let mut w = open_writer(&p, gzip)?;
        for b in &barcodes.barcode {
            writeln!(w, "{b}").map_err(|e| ScqcError::io_write(&p, &e))?;
        }
        w.flush().map_err(|e| ScqcError::io_write(&p, &e))?;
    }
    Ok(())
}

fn open_writer(path: &Path, gzip: bool) -> Result<Box<dyn Write>> {
    let f = File::create(path).map_err(|e| ScqcError::io_write(path, &e))?;
    let w: Box<dyn Write> = if gzip {
        Box::new(BufWriter::with_capacity(
            1 << 20,
            flate2::write::GzEncoder::new(f, flate2::Compression::fast()),
        ))
    } else {
        Box::new(BufWriter::with_capacity(1 << 20, f))
    };
    Ok(w)
}

/// 写 JSON（pretty，UTF-8，键顺序由结构体定义决定 → 可复现）。
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let s = serde_json::to_string_pretty(value)
        .map_err(|e| ScqcError::internal("JSON 序列化失败").with_detail(e.to_string()))?;
    fs::write(path, s).map_err(|e| ScqcError::io_write(path, &e))
}

/// 写纯文本行列表（如 cells_passed.txt）。
pub fn write_lines(path: &Path, lines: &[String]) -> Result<()> {
    let f = File::create(path).map_err(|e| ScqcError::io_write(path, &e))?;
    let mut w = BufWriter::new(f);
    for l in lines {
        writeln!(w, "{l}").map_err(|e| ScqcError::io_write(path, &e))?;
    }
    w.flush().map_err(|e| ScqcError::io_write(path, &e))
}
