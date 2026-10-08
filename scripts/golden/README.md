# 金标准对照数据生成脚本（在 R 中运行）

> 这些脚本**只调用**对照包（DropletUtils、SoupX、celda、scds、DoubletFinder、ddqcR）来生成
> 期望输出，供 `scqc-engine verify` 比对。使用软件包与阅读其源代码是两回事，
> 不违反 SPEC §9.2 的许可证约束（代理不会打开 GPL 包的源码）。

## 一次性准备

1. 安装 R（≥ 4.3）与 RStudio（可选）。
2. 在 R 中安装依赖：

```r
install.packages(c("BiocManager", "remotes", "Seurat", "Matrix", "jsonlite"))
BiocManager::install(c("DropletUtils", "celda", "scds", "SingleCellExperiment"))
install.packages("SoupX")
remotes::install_github("chris-mcginnis-ucsf/DoubletFinder")
remotes::install_github("ayshwaryas/ddqc_R")
```

3. 下载 10x 公开数据 **PBMC 3k**（需要 raw 与 filtered 两个 MTX 目录），放到
   `tests/golden/pbmc3k/input/raw/` 与 `tests/golden/pbmc3k/input/filtered/`。
   下载页：10x Genomics → Datasets → "PBMC from a Healthy Donor (v1 chemistry, 3k)"
   文件名形如 `pbmc3k_raw_gene_bc_matrices.tar.gz` / `pbmc3k_filtered_gene_bc_matrices.tar.gz`。

## 运行

在仓库根目录：

```sh
Rscript scripts/golden/00_all.R tests/golden/pbmc3k
```

约 10–20 分钟。运行结束后 `tests/golden/pbmc3k/` 下会出现：

| 文件 | 内容 | 用于 |
|---|---|---|
| `clusters.csv` | barcode, cluster（Seurat 粗聚类，供 SoupX/DecontX/ddqc 对齐） | 隔离估计器本身的一致性 |
| `emptydrops/cells.txt` | DropletUtils::emptyDrops 判为细胞的 barcode | Jaccard ≥ 0.95 |
| `emptydrops/params.json` | lower、niters、FDR、种子 | — |
| `ddqc/kept.txt` + `params.json` | ddqcR 保留的 barcode | Jaccard ≥ 0.95 |
| `cxds/scores.csv` | barcode, score | Spearman ≥ 0.95 |
| `doubletfinder/doublets.txt` + `params.json` | 双细胞 barcode；pN、pK、预期率 | Jaccard ≥ 0.90 |
| `soupx/rho.json` | 全局 ρ 与簇级 ρ | |ρ 差| ≤ 0.02 |
| `decontx/rho.csv` | barcode, contamination（background 模式） | Pearson ≥ 0.90 |
| `decontx/decontx_counts.mtx` | 校正矩阵 | 逐基因 Spearman ≥ 0.95 |
| `versions.json` | 各包版本 | 记录 |

矩阵文件较大，`.gitignore` 已排除 `tests/golden/**/*.mtx*`；其余小文件请一起提交。
