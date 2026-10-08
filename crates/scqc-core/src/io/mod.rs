//! 输入输出：10x MTX 目录、features / barcodes 表、Cell Ranger 元数据（DESIGN §5.1）。
//!
//! 所有路径以 `Path` 处理，测试覆盖中文目录名（SPEC §14.4）。
//! `.gz` 与未压缩文件均支持。

pub mod barcodes;
pub mod cellranger_meta;
pub mod features;
pub mod mtx;
pub mod writer;

use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use barcodes::BarcodeTable;
pub use cellranger_meta::CellRangerMetrics;
pub use features::FeatureTable;

use crate::error::{ErrorCode, Result, ScqcError};
use crate::matrix::{CountMatrix, MatrixSummary};

/// 一个 10x 矩阵目录读入后的完整内容。
#[derive(Debug)]
pub struct MatrixDir {
    pub dir: PathBuf,
    pub matrix: CountMatrix,
    pub features: FeatureTable,
    pub barcodes: BarcodeTable,
    pub files: Vec<InputFile>,
}

/// 输入文件记录（写入 `raw_input_summary.json`）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InputFile {
    /// 只记录文件名，不记录完整路径（C1：不把路径写进可能被分享的输出）
    pub name: String,
    pub size_bytes: u64,
    pub sha256: String,
}

/// raw + filtered 两个目录的组合，是流程的输入。
#[derive(Debug)]
pub struct Dataset {
    pub raw: Option<MatrixDir>,
    pub filtered: MatrixDir,
    pub metrics: Option<CellRangerMetrics>,
    /// raw 与 filtered 基因列表不一致时的对齐说明（一致时为 None）
    pub feature_alignment: Option<FeatureAlignment>,
    /// 非 Gene Expression 特征被排除的说明（没有这类特征时为 None）
    pub non_gex: Option<NonGexExclusion>,
}

/// `raw_input_summary.json` 的内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSummary {
    pub raw: Option<MatrixSummary>,
    pub filtered: MatrixSummary,
    pub raw_files: Vec<InputFile>,
    pub filtered_files: Vec<InputFile>,
    pub metrics_file: Option<InputFile>,
}

impl Dataset {
    pub fn summary(&self) -> InputSummary {
        InputSummary {
            raw: self.raw.as_ref().map(|r| r.matrix.summary()),
            filtered: self.filtered.matrix.summary(),
            raw_files: self
                .raw
                .as_ref()
                .map(|r| r.files.clone())
                .unwrap_or_default(),
            filtered_files: self.filtered.files.clone(),
            metrics_file: self.metrics.as_ref().map(|m| m.file.clone()),
        }
    }
}

/// 在目录中寻找 `base`、`base.gz` 之一；`alternatives` 为旧版文件名（如 genes.tsv）。
pub(crate) fn find_file(dir: &Path, base: &str, alternatives: &[&str]) -> Result<PathBuf> {
    let mut candidates = vec![base.to_string()];
    candidates.extend(alternatives.iter().map(|s| s.to_string()));
    for c in &candidates {
        for name in [c.clone(), format!("{c}.gz")] {
            let p = dir.join(&name);
            if p.is_file() {
                return Ok(p);
            }
        }
    }
    Err(ScqcError::new(
        ErrorCode::IoRead,
        format!(
            "目录 {} 中找不到 {}（或 {}.gz）",
            dir.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            base,
            base
        ),
    )
    .with_detail(dir.display().to_string()))
}

/// 打开文件，按扩展名自动解压 gzip。
pub(crate) fn open_maybe_gz(path: &Path) -> Result<Box<dyn BufRead>> {
    let f = File::open(path).map_err(|e| ScqcError::io_read(path, &e))?;
    let is_gz = path
        .extension()
        .map(|e| e.eq_ignore_ascii_case("gz"))
        .unwrap_or(false);
    let reader: Box<dyn Read> = if is_gz {
        Box::new(flate2::read::MultiGzDecoder::new(f))
    } else {
        Box::new(f)
    };
    Ok(Box::new(BufReader::with_capacity(1 << 20, reader)))
}

/// 文件大小与 SHA-256（用于 `raw_input_summary.json`，保证可复现性可追溯）。
pub(crate) fn describe_file(path: &Path) -> Result<InputFile> {
    use sha2::{Digest, Sha256};
    let mut f = File::open(path).map_err(|e| ScqcError::io_read(path, &e))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut size = 0u64;
    loop {
        let n = f.read(&mut buf).map_err(|e| ScqcError::io_read(path, &e))?;
        if n == 0 {
            break;
        }
        size += n as u64;
        hasher.update(&buf[..n]);
    }
    Ok(InputFile {
        name: path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        size_bytes: size,
        sha256: format!("{:x}", hasher.finalize()),
    })
}

/// 读取一个 10x MTX 目录（barcodes + features + matrix）。
pub fn read_matrix_dir(dir: &Path) -> Result<MatrixDir> {
    if !dir.is_dir() {
        return Err(ScqcError::new(
            ErrorCode::IoRead,
            format!(
                "矩阵目录不存在或不是目录：{}",
                dir.file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            ),
        )
        .with_detail(dir.display().to_string()));
    }
    let mtx_path = find_file(dir, "matrix.mtx", &[])?;
    let feat_path = find_file(dir, "features.tsv", &["genes.tsv"])?;
    let bc_path = find_file(dir, "barcodes.tsv", &[])?;

    let features = features::read_features(&feat_path)?;
    let barcodes = barcodes::read_barcodes(&bc_path)?;
    let matrix = mtx::read_mtx(&mtx_path, features.len(), barcodes.len())?;

    let files = vec![
        describe_file(&mtx_path)?,
        describe_file(&feat_path)?,
        describe_file(&bc_path)?,
    ];
    Ok(MatrixDir {
        dir: dir.to_path_buf(),
        matrix,
        features,
        barcodes,
        files,
    })
}

/// raw 与 filtered 基因列表不一致时的对齐结果（真实 Flex 数据的常态）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FeatureAlignment {
    pub n_raw: usize,
    pub n_filtered: usize,
    /// 对齐后保留的基因数
    pub n_common: usize,
    /// raw 矩阵要保留的行（按 filtered 的顺序）
    pub raw_rows: Vec<usize>,
    /// filtered 矩阵要保留的行（filtered ⊆ raw 时即全部）
    pub filtered_rows: Vec<usize>,
    /// raw 中被丢弃的 UMI 数与占比
    pub raw_dropped_umi: u64,
    pub raw_dropped_umi_fraction: f64,
    pub note_zh: String,
}

/// 非 Gene Expression 特征（Antibody Capture 等）被排除出 QC 的记录。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct NonGexExclusion {
    pub n_features_total: usize,
    pub n_features_dropped: usize,
    /// 被排除特征的类型 → 个数
    pub types: Vec<(String, usize)>,
    /// 这些特征在 filtered / raw 中占总 UMI 的比例
    pub umi_fraction_filtered: f64,
    pub umi_fraction_raw: f64,
    pub note_zh: String,
}

/// 按基因 ID 求交集；顺序以 filtered 为准（探针集顺序），保证结果可复现。
fn align_features(raw: &MatrixDir, filtered: &MatrixDir) -> Result<FeatureAlignment> {
    let raw_index: std::collections::HashMap<&str, usize> = raw
        .features
        .id
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let mut raw_rows = Vec::new();
    let mut filtered_rows = Vec::new();
    for (fi, id) in filtered.features.id.iter().enumerate() {
        if let Some(&ri) = raw_index.get(id.as_str()) {
            raw_rows.push(ri);
            filtered_rows.push(fi);
        }
    }
    let n_common = raw_rows.len();
    if n_common * 2 < filtered.features.len() || n_common == 0 {
        return Err(ScqcError::new(
            ErrorCode::InputMismatch,
            format!(
                "raw 与 filtered 的基因列表几乎没有交集（raw {} 个、filtered {} 个、共有 {} 个），                 两者很可能不属于同一次运行或用了不同的参考",
                raw.features.len(),
                filtered.features.len(),
                n_common
            ),
        ));
    }
    Ok(FeatureAlignment {
        n_raw: raw.features.len(),
        n_filtered: filtered.features.len(),
        n_common,
        raw_rows,
        filtered_rows,
        raw_dropped_umi: 0,
        raw_dropped_umi_fraction: 0.0,
        note_zh: String::new(),
    })
}

/// 读取整个数据集。`raw` 可缺失（此时真细胞鉴定与环境 RNA 诊断不可用，SPEC §6.1）。
///
/// 校验：filtered 的 barcode 必须是 raw 的子集；raw 与 filtered 的基因列表不一致时
/// 按基因 ID 取交集对齐（并在 `feature_alignment` 里记录，供报告说明）。
pub fn read_dataset(
    raw: Option<&Path>,
    filtered: &Path,
    metrics_csv: Option<&Path>,
) -> Result<Dataset> {
    let mut filtered_dir = read_matrix_dir(filtered)?;
    let raw_dir = match raw {
        Some(p) => Some(read_matrix_dir(p)?),
        None => None,
    };
    let mut raw_dir = raw_dir;
    let mut feature_alignment = None;
    if let Some(r) = &raw_dir {
        if r.features.id != filtered_dir.features.id {
            // 真实 Flex 数据里这是常态：raw 用整套参考（如 39,186 个基因），
            // per-sample filtered 只保留探针集内的基因（如 18,129 个）。
            // 不能直接报错，也不能按位置硬套——必须按基因 ID 对齐后取交集。
            let align = align_features(r, &filtered_dir)?;
            feature_alignment = Some(align);
        }
    }
    // 注意：这里必须先判断再 take —— 元组模式会无条件求值 `raw_dir.take()`，
    // 那样在"基因列表本来就一致"的常见路径上会把 raw 丢掉。
    let need_align = feature_alignment.is_some();
    if let (Some(align), Some(mut r)) = (
        &feature_alignment,
        if need_align { raw_dir.take() } else { None },
    ) {
        let (m, dropped) = r.matrix.select_rows(&align.raw_rows)?;
        r.matrix = m;
        r.features = r.features.select(&align.raw_rows);
        let total_before: u64 = dropped + r.matrix.column_sums().iter().sum::<u64>();
        let mut align2 = align.clone();
        align2.raw_dropped_umi = dropped;
        align2.raw_dropped_umi_fraction = if total_before > 0 {
            dropped as f64 / total_before as f64
        } else {
            0.0
        };
        if align.filtered_rows.len() != filtered_dir.features.len() {
            let (m, _) = filtered_dir.matrix.select_rows(&align.filtered_rows)?;
            filtered_dir.matrix = m;
            filtered_dir.features = filtered_dir.features.select(&align.filtered_rows);
        }
        align2.note_zh = format!(
            "raw 与 filtered 的基因列表不同（{} vs {} 个），已按基因 ID 取交集 {} 个后对齐；\
             raw 中落在交集之外的计数占 {:.2}%（多为探针集之外的脱靶信号）",
            align2.n_raw,
            align2.n_filtered,
            align2.raw_rows.len(),
            align2.raw_dropped_umi_fraction * 100.0
        );
        raw_dir = Some(r);
        feature_alignment = Some(align2);
    }
    if let Some(r) = &raw_dir {
        let raw_set: std::collections::HashSet<&str> =
            r.barcodes.barcode.iter().map(|s| s.as_str()).collect();
        let missing = filtered_dir
            .barcodes
            .barcode
            .iter()
            .filter(|b| !raw_set.contains(b.as_str()))
            .count();
        if missing > 0 {
            return Err(ScqcError::new(
                ErrorCode::InputMismatch,
                format!(
                    "filtered 矩阵中有 {missing} 个 barcode 不在 raw 矩阵中，raw 与 filtered 可能不属于同一个样本"
                ),
            ));
        }
    }
    // 【只用 Gene Expression 特征做 QC】
    // 真实数据教训：带 TotalSeq 抗体的 Flex 样本里，Antibody Capture 占了 42% 的细胞 UMI、
    // 46% 的空液滴 UMI。若把它们算进"总 UMI"，线粒体比例会被稀释近一半、ddqc 的计数维度、
    // 环境表达谱和污染度估计全都被抗体信号带偏。因此 QC 全程只用 Gene Expression 特征。
    let gex_rows = filtered_dir.features.gene_expression_indices();
    let mut non_gex = None;
    if gex_rows.len() < filtered_dir.features.len() {
        if gex_rows.is_empty() {
            return Err(ScqcError::new(
                ErrorCode::InputMismatch,
                "features 文件里没有 Gene Expression 类型的特征，无法做基因表达质控",
            ));
        }
        let mut types: std::collections::BTreeMap<String, usize> = Default::default();
        for (i, t) in filtered_dir.features.feature_type.iter().enumerate() {
            if !gex_rows.contains(&i) {
                *types.entry(t.clone()).or_insert(0) += 1;
            }
        }
        let (fm, f_dropped) = filtered_dir.matrix.select_rows(&gex_rows)?;
        let f_total = f_dropped + fm.column_sums().iter().sum::<u64>();
        filtered_dir.matrix = fm;
        let mut r_frac = 0.0;
        if let Some(r) = raw_dir.take() {
            let mut r = r;
            let (rm, r_dropped) = r.matrix.select_rows(&gex_rows)?;
            let r_total = r_dropped + rm.column_sums().iter().sum::<u64>();
            r_frac = if r_total > 0 {
                r_dropped as f64 / r_total as f64
            } else {
                0.0
            };
            r.matrix = rm;
            r.features = r.features.select(&gex_rows);
            raw_dir = Some(r);
        }
        let n_total = filtered_dir.features.len();
        filtered_dir.features = filtered_dir.features.select(&gex_rows);
        let f_frac = if f_total > 0 {
            f_dropped as f64 / f_total as f64
        } else {
            0.0
        };
        let types: Vec<(String, usize)> = types.into_iter().collect();
        non_gex = Some(NonGexExclusion {
            n_features_total: n_total,
            n_features_dropped: n_total - gex_rows.len(),
            note_zh: format!(
                "输入包含 {} 个非 Gene Expression 特征（{}），它们占细胞总 UMI 的 {:.1}%、空液滴总 UMI 的 {:.1}%。\
                 为避免稀释线粒体比例、带偏总 UMI 与环境 RNA 估计，**全部质控只使用 Gene Expression 特征**（硬约束 C10）；\
                 这些特征本身不丢——输出目录的 filtered_all_features/ 是同一批保留细胞的全特征矩阵",
                n_total - gex_rows.len(),
                types
                    .iter()
                    .map(|(t, n)| format!("{t} {n} 个"))
                    .collect::<Vec<_>>()
                    .join("、"),
                f_frac * 100.0,
                r_frac * 100.0
            ),
            types,
            umi_fraction_filtered: f_frac,
            umi_fraction_raw: r_frac,
        });
    }
    let metrics = match metrics_csv {
        Some(p) => Some(cellranger_meta::read_metrics_summary(p)?),
        None => None,
    };
    Ok(Dataset {
        raw: raw_dir,
        filtered: filtered_dir,
        metrics,
        feature_alignment,
        non_gex,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod alignment_tests {
    use super::*;
    use std::io::Write;

    /// 写一个最小 MTX 目录：`genes` 为 (id, name)，`entries` 为 (基因序号从1, barcode序号从1, 值)。
    fn write_dir(
        dir: &Path,
        genes: &[(&str, &str)],
        barcodes: &[&str],
        entries: &[(usize, usize, u32)],
    ) {
        std::fs::create_dir_all(dir).unwrap();
        let mut f = File::create(dir.join("features.tsv")).unwrap();
        for (id, name) in genes {
            writeln!(f, "{id}\t{name}\tGene Expression").unwrap();
        }
        let mut f = File::create(dir.join("barcodes.tsv")).unwrap();
        for b in barcodes {
            writeln!(f, "{b}").unwrap();
        }
        let mut f = File::create(dir.join("matrix.mtx")).unwrap();
        writeln!(f, "%%MatrixMarket matrix coordinate integer general").unwrap();
        writeln!(f, "{} {} {}", genes.len(), barcodes.len(), entries.len()).unwrap();
        for (g, c, v) in entries {
            writeln!(f, "{g} {c} {v}").unwrap();
        }
    }

    /// 真实 Flex 场景：raw 用整套参考，filtered 只含探针集内的基因 → 按 ID 取交集对齐。
    #[test]
    fn aligns_raw_superset_to_filtered_probe_set() {
        let tmp = tempfile::tempdir().unwrap();
        let raw = tmp.path().join("raw");
        let filt = tmp.path().join("filtered");
        write_dir(
            &raw,
            &[("G1", "A"), ("G2", "B"), ("G3", "C"), ("G4", "D")],
            &["BC1-1", "BC2-1", "BC3-1"],
            // G4（探针集外）在 raw 里有 10 个 UMI，应被丢弃并计入统计
            &[(1, 1, 5), (2, 1, 3), (4, 1, 10), (1, 2, 2), (3, 3, 7)],
        );
        write_dir(
            &filt,
            &[("G1", "A"), ("G2", "B"), ("G3", "C")],
            &["BC1-1"],
            &[(1, 1, 5), (2, 1, 3)],
        );
        let ds = read_dataset(Some(&raw), &filt, None).unwrap();
        let al = ds.feature_alignment.expect("应记录对齐信息");
        assert_eq!((al.n_raw, al.n_filtered, al.n_common), (4, 3, 3));
        assert_eq!(al.raw_dropped_umi, 10);
        let r = ds.raw.unwrap();
        assert_eq!(r.matrix.n_genes(), 3);
        assert_eq!(r.features.id, vec!["G1", "G2", "G3"]);
        // 对齐后 raw 的总 UMI = 27 - 10
        assert_eq!(r.matrix.column_sums().iter().sum::<u64>(), 17);
        assert!(al.note_zh.contains("交集"));
    }

    /// 交集过小 → 报错，不猜。
    #[test]
    fn rejects_when_gene_sets_barely_overlap() {
        let tmp = tempfile::tempdir().unwrap();
        let raw = tmp.path().join("raw");
        let filt = tmp.path().join("filtered");
        write_dir(&raw, &[("G1", "A"), ("G2", "B")], &["BC1-1"], &[(1, 1, 1)]);
        write_dir(
            &filt,
            &[("X1", "A"), ("X2", "B"), ("X3", "C"), ("G1", "A")],
            &["BC1-1"],
            &[(1, 1, 1)],
        );
        let e = read_dataset(Some(&raw), &filt, None).err().unwrap();
        assert_eq!(e.code, ErrorCode::InputMismatch);
    }
}
