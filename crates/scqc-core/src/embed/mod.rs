//! 嵌入与聚类基础设施（SPEC §8.6）：归一化 → HVG → PCA → kNN → SNN → Leiden（两套分辨率）。
//!
//! 【硬约束】聚类在整个流程中只计算一次，产物放在 [`Embedding`] 中供各模块共用。
//! 本目录不出现任何平台名；所有参数来自 `EmbedParams`。

pub mod hvg;
pub mod knn;
pub mod leiden;
pub mod normalize;
pub mod pca;
pub mod snn;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::matrix::CountMatrix;
use crate::pipeline::cancel::CancelToken;

pub use knn::KnnGraph;
pub use leiden::LeidenResult;
pub use pca::PcaResult;
pub use snn::WeightedGraph;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbedParams {
    pub n_hvg: usize,
    pub n_pcs: usize,
    pub k: usize,
    pub res_fine: f64,
    pub res_coarse: f64,
    pub seed: u64,
    /// z-score 裁剪
    pub scale_clip: f32,
    /// SNN 剪枝阈值
    pub snn_prune: f32,
}

impl Default for EmbedParams {
    fn default() -> Self {
        Self {
            n_hvg: 2000,
            n_pcs: 50,
            k: 20,
            res_fine: 1.3,
            res_coarse: 0.3,
            seed: crate::rng::DEFAULT_SEED,
            scale_clip: 10.0,
            snn_prune: 1.0 / 15.0,
        }
    }
}

/// 嵌入产物（DESIGN §3.5 `SharedContext` 的聚类部分）。
pub struct Embedding {
    pub n_cells: usize,
    pub hvg: Vec<usize>,
    pub pca: PcaResult,
    pub knn: KnnGraph,
    pub snn: WeightedGraph,
    pub fine: LeidenResult,
    pub coarse: LeidenResult,
    pub timing: EmbedTiming,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EmbedTiming {
    pub stats_s: f64,
    pub scale_s: f64,
    pub pca_s: f64,
    pub knn_s: f64,
    pub snn_s: f64,
    pub leiden_s: f64,
}

/// 写入 JSON 的摘要。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingSummary {
    pub n_cells: usize,
    pub n_hvg: usize,
    pub n_pcs: usize,
    pub explained_variance_ratio: Vec<f64>,
    pub k: usize,
    pub snn_edges: usize,
    pub fine_resolution: f64,
    pub fine_n_clusters: usize,
    pub fine_modularity: f64,
    pub fine_sizes: Vec<usize>,
    pub coarse_resolution: f64,
    pub coarse_n_clusters: usize,
    pub coarse_modularity: f64,
    pub coarse_sizes: Vec<usize>,
    pub timing: EmbedTiming,
}

impl Embedding {
    pub fn summary(&self, params: &EmbedParams) -> EmbeddingSummary {
        let sizes = |r: &LeidenResult| {
            let mut s = vec![0usize; r.n_communities];
            for &c in &r.membership {
                s[c as usize] += 1;
            }
            s
        };
        EmbeddingSummary {
            n_cells: self.n_cells,
            n_hvg: self.hvg.len(),
            n_pcs: self.pca.k,
            explained_variance_ratio: self.pca.explained_variance_ratio.clone(),
            k: self.knn.k,
            snn_edges: self.snn.n_edges(),
            fine_resolution: params.res_fine,
            fine_n_clusters: self.fine.n_communities,
            fine_modularity: self.fine.modularity,
            fine_sizes: sizes(&self.fine),
            coarse_resolution: params.res_coarse,
            coarse_n_clusters: self.coarse.n_communities,
            coarse_modularity: self.coarse.modularity,
            coarse_sizes: sizes(&self.coarse),
            timing: self.timing.clone(),
        }
    }
}

/// 对细胞矩阵（列 = 细胞）构建嵌入与两套聚类。
/// `gene_candidates`：可作为 HVG 的基因索引（Gene Expression 类型）。
/// `on_progress(pct, message)` 用于进度回报。
pub fn build_embedding(
    matrix: &CountMatrix,
    gene_candidates: &[usize],
    params: &EmbedParams,
    cancel: &CancelToken,
    on_progress: &dyn Fn(u8, &str),
) -> Result<Embedding> {
    let n = matrix.n_barcodes();
    let mut timing = EmbedTiming::default();
    let t = std::time::Instant::now();

    on_progress(2, "归一化与基因统计");
    let totals = matrix.column_sums();
    let stats = normalize::gene_stats(matrix, &totals);
    let hvg = hvg::select_hvg(&stats, gene_candidates, params.n_hvg, 20);
    timing.stats_s = t.elapsed().as_secs_f64();
    cancel.checkpoint()?;

    on_progress(10, "构建缩放矩阵");
    let t = std::time::Instant::now();
    let dense = normalize::dense_scaled(matrix, &totals, &hvg, &stats, params.scale_clip);
    timing.scale_s = t.elapsed().as_secs_f64();
    cancel.checkpoint()?;

    on_progress(20, "PCA（随机化 SVD）");
    let t = std::time::Instant::now();
    let pca = pca::randomized_pca(
        &dense,
        n,
        hvg.len(),
        &pca::PcaParams {
            n_components: params.n_pcs,
            seed: params.seed,
            ..Default::default()
        },
    );
    drop(dense);
    timing.pca_s = t.elapsed().as_secs_f64();
    cancel.checkpoint()?;

    on_progress(50, "kNN（HNSW）");
    let t = std::time::Instant::now();
    let knn = knn::knn_graph(
        &pca.scores,
        n,
        pca.k,
        &knn::KnnParams {
            k: params.k,
            ..Default::default()
        },
    );
    timing.knn_s = t.elapsed().as_secs_f64();
    cancel.checkpoint()?;

    on_progress(75, "SNN 图");
    let t = std::time::Instant::now();
    let snn = snn::snn_graph(&knn, params.snn_prune);
    timing.snn_s = t.elapsed().as_secs_f64();
    cancel.checkpoint()?;

    on_progress(80, "Leiden 聚类（精细 + 粗）");
    let t = std::time::Instant::now();
    let fine = leiden::leiden(
        &snn,
        &leiden::LeidenParams {
            resolution: params.res_fine,
            seed: params.seed,
            ..Default::default()
        },
    );
    let coarse = leiden::leiden(
        &snn,
        &leiden::LeidenParams {
            resolution: params.res_coarse,
            seed: params.seed,
            ..Default::default()
        },
    );
    timing.leiden_s = t.elapsed().as_secs_f64();
    on_progress(100, "聚类完成");

    Ok(Embedding {
        n_cells: n,
        hvg,
        pca,
        knn,
        snn,
        fine,
        coarse,
        timing,
    })
}
