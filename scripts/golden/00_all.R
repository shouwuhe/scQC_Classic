#!/usr/bin/env Rscript
# 生成 PBMC 3k 的全部金标准对照结果。用法：Rscript scripts/golden/00_all.R tests/golden/pbmc3k
# 每个包只作为"黑盒"调用；参数全部固定并记录到 params.json（SPEC §14.1）。
suppressPackageStartupMessages({
  library(Matrix); library(jsonlite); library(DropletUtils); library(Seurat)
  library(SoupX); library(celda); library(scds); library(SingleCellExperiment); library(DoubletFinder)
})
args <- commandArgs(trailingOnly = TRUE)
if (length(args) < 1) stop("用法：Rscript 00_all.R <tests/golden/pbmc3k>")
root <- args[1]
set.seed(20260911)
SEED <- 20260911

read10x <- function(d) { m <- Seurat::Read10X(d); if (is.list(m)) m <- m[["Gene Expression"]]; m }
raw <- read10x(file.path(root, "input", "raw"))
filt <- read10x(file.path(root, "input", "filtered"))
dir.create(root, showWarnings = FALSE, recursive = TRUE)
mk <- function(sub) { d <- file.path(root, sub); dir.create(d, showWarnings = FALSE); d }
wjson <- function(x, f) write_json(x, f, auto_unbox = TRUE, pretty = TRUE, digits = NA)

# ---------- 1. EmptyDrops（DropletUtils）----------
d <- mk("emptydrops")
ed <- emptyDrops(raw, lower = 100, niters = 10000, test.ambient = FALSE)
cells <- colnames(raw)[which(ed$FDR <= 0.01)]
writeLines(cells, file.path(d, "cells.txt"))
wjson(list(lower = 100, niters = 10000, fdr = 0.01, seed = SEED, package = "DropletUtils",
           version = as.character(packageVersion("DropletUtils"))), file.path(d, "params.json"))
message("EmptyDrops 细胞数：", length(cells))

# ---------- 2. 粗聚类（Seurat，供 SoupX / DecontX / ddqc 共用）----------
so <- CreateSeuratObject(filt)
so <- NormalizeData(so, verbose = FALSE) |> FindVariableFeatures(nfeatures = 2000, verbose = FALSE) |>
  ScaleData(verbose = FALSE) |> RunPCA(npcs = 50, verbose = FALSE) |>
  FindNeighbors(dims = 1:50, k.param = 20, verbose = FALSE) |>
  FindClusters(resolution = 0.3, algorithm = 4, random.seed = SEED, verbose = FALSE)
clusters <- setNames(as.character(Idents(so)), colnames(so))
write.csv(data.frame(barcode = names(clusters), cluster = clusters), file.path(root, "clusters.csv"), row.names = FALSE)

# ---------- 3. SoupX ----------
d <- mk("soupx")
sc <- SoupChannel(raw, filt)
sc <- setClusters(sc, clusters[colnames(filt)])
sc <- autoEstCont(sc, doPlot = FALSE)
wjson(list(rho_global = sc$fit$rhoEst, rho_low = sc$fit$rhoFWHM[1], rho_high = sc$fit$rhoFWHM[2],
           rho_per_cell = setNames(as.list(sc$metaData$rho), rownames(sc$metaData)),
           params = list(soup_umi_range = c(0, 10), package = "SoupX",
                         version = as.character(packageVersion("SoupX")))),
      file.path(d, "rho.json"))
message("SoupX ρ：", sc$fit$rhoEst)

# ---------- 4. DecontX（background 模式）----------
d <- mk("decontx")
empty <- raw[, Matrix::colSums(raw) <= 100 & Matrix::colSums(raw) > 0]
sce <- SingleCellExperiment(assays = list(counts = filt))
sce <- decontX(sce, z = clusters[colnames(filt)], background = empty, seed = SEED)
write.csv(data.frame(barcode = colnames(sce), contamination = sce$decontX_contamination),
          file.path(d, "rho.csv"), row.names = FALSE)
Matrix::writeMM(as(decontXcounts(sce), "dgCMatrix"), file.path(d, "decontx_counts.mtx"))
wjson(list(background_umi_upper = 100, seed = SEED, package = "celda",
           version = as.character(packageVersion("celda"))), file.path(d, "params.json"))
message("DecontX 平均污染度：", mean(sce$decontX_contamination))

# ---------- 5. cxds（scds）----------
d <- mk("cxds")
sce2 <- SingleCellExperiment(assays = list(counts = filt))
sce2 <- cxds(sce2, ntop = 500, binThresh = 0)
write.csv(data.frame(barcode = colnames(sce2), score = sce2$cxds_score), file.path(d, "scores.csv"), row.names = FALSE)
wjson(list(ntop = 500, binThresh = 0, package = "scds", version = as.character(packageVersion("scds"))),
      file.path(d, "params.json"))

# ---------- 6. DoubletFinder ----------
d <- mk("doubletfinder")
so2 <- so |> RunUMAP(dims = 1:50, verbose = FALSE)
sweep <- paramSweep(so2, PCs = 1:50, sct = FALSE)
stats <- summarizeSweep(sweep, GT = FALSE)
bcmvn <- find.pK(stats)
pK <- as.numeric(as.character(bcmvn$pK[which.max(bcmvn$BCmetric)]))
rate <- 0.008 * ncol(so2) / 1000
nExp <- round(rate * ncol(so2))
so2 <- doubletFinder(so2, PCs = 1:50, pN = 0.25, pK = pK, nExp = nExp, sct = FALSE)
col <- grep("^DF.classifications", colnames(so2@meta.data), value = TRUE)[1]
dbl <- colnames(so2)[so2@meta.data[[col]] == "Doublet"]
writeLines(dbl, file.path(d, "doublets.txt"))
wjson(list(pN = 0.25, pK = pK, expected_rate = rate, nExp = nExp, seed = SEED,
           package = "DoubletFinder", version = as.character(packageVersion("DoubletFinder"))),
      file.path(d, "params.json"))
message("DoubletFinder 双细胞数：", length(dbl), "，pK=", pK)

# ---------- 7. ddqc ----------
d <- mk("ddqc")
if (requireNamespace("ddqcR", quietly = TRUE)) {
  library(ddqcR)
  so3 <- CreateSeuratObject(filt)
  so3 <- initialQC(so3)
  df <- ddqc.metrics(so3, res = 1.3, threshold = 2, do.counts = FALSE, do.genes = TRUE,
                     do.mito = TRUE, do.ribo = TRUE, n.genes.lower.bound = 200, percent.mito.upper.bound = 10)
  so3 <- filterData(so3, df)
  writeLines(colnames(so3), file.path(d, "kept.txt"))
  wjson(list(res = 1.3, mad = 2, dims = c("genes", "mito", "ribo"), n_genes_lower_bound = 200,
             percent_mito_upper_bound = 10, package = "ddqcR",
             version = as.character(packageVersion("ddqcR"))), file.path(d, "params.json"))
  message("ddqc 保留：", ncol(so3))
} else message("未安装 ddqcR，跳过 ddqc（remotes::install_github('ayshwaryas/ddqc_R')）")

wjson(list(R = R.version.string, Seurat = as.character(packageVersion("Seurat"))), file.path(root, "versions.json"))
message("全部完成：", root)
