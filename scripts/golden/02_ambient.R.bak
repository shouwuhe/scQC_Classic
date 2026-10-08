#!/usr/bin/env Rscript
# 环境 RNA 对照结果：SoupX::autoEstCont 与 celda::decontX(background=空液滴)。
# 用法：Rscript scripts/golden/02_ambient.R tests/golden/pbmc3k
# 聚类标签：优先读取 <root>/scqc_clusters.csv（由本工具 run 输出的 qc_metrics.csv 抽取，
# 列 barcode,cluster），这样比对隔离的是"估计器本身"的一致性（DESIGN §13.2）。
suppressPackageStartupMessages({ library(Matrix); library(jsonlite); library(DropletUtils); library(SoupX); library(celda); library(SingleCellExperiment) })
args <- commandArgs(trailingOnly = TRUE)
root <- if (length(args) >= 1) args[1] else "tests/golden/pbmc3k"
set.seed(20260911)
read10x <- function(d) { x <- DropletUtils::read10xCounts(d); m <- SingleCellExperiment::counts(x); colnames(m) <- x$Barcode; rownames(m) <- rowData(x)$Symbol; m }
raw <- read10x(file.path(root, "input", "raw"))
cl <- read.csv(file.path(root, "scqc_clusters.csv"), stringsAsFactors = FALSE)
cells <- raw[, cl$barcode]                      # 本工具判定的细胞集合（含 rescued）
clusters <- setNames(as.character(cl$cluster), cl$barcode)

# ---- SoupX ----
sc <- SoupChannel(raw, cells)                    # estimateSoup 默认 soupRange = c(0, 100)
sc <- setClusters(sc, clusters[colnames(cells)])
sc <- autoEstCont(sc, doPlot = FALSE, forceAccept = TRUE)
d <- file.path(root, "soupx"); dir.create(d, showWarnings = FALSE, recursive = TRUE)
write_json(list(rho_global = sc$fit$rhoEst, rho_low = sc$fit$rhoFWHM[1], rho_high = sc$fit$rhoFWHM[2],
                n_markers = nrow(sc$fit$markersUsed), package = "SoupX", version = as.character(packageVersion("SoupX")),
                params = list(soup_umi_range = c(0, 100), tfidfMin = 1, soupQuantile = 0.9, priorRho = 0.05, priorRhoStdDev = 0.1)),
           file.path(d, "rho.json"), auto_unbox = TRUE, pretty = TRUE, digits = NA)
message("SoupX rho = ", sc$fit$rhoEst)

# ---- DecontX background ----
empty <- raw[, Matrix::colSums(raw) > 0 & Matrix::colSums(raw) <= 100]
sce <- SingleCellExperiment(assays = list(counts = cells))
sce <- decontX(sce, z = clusters[colnames(cells)], background = empty, seed = 20260911)
d <- file.path(root, "decontx"); dir.create(d, showWarnings = FALSE, recursive = TRUE)
write.csv(data.frame(barcode = colnames(sce), contamination = sce$decontX_contamination), file.path(d, "rho.csv"), row.names = FALSE)
Matrix::writeMM(as(decontXcounts(sce), "dgCMatrix"), file.path(d, "decontx_counts.mtx"))
write_json(list(background_umi_upper = 100, seed = 20260911, package = "celda", version = as.character(packageVersion("celda")),
                rho_mean = mean(sce$decontX_contamination), rho_median = median(sce$decontX_contamination)),
           file.path(d, "params.json"), auto_unbox = TRUE, pretty = TRUE, digits = NA)
message("DecontX mean rho = ", mean(sce$decontX_contamination), "  median = ", median(sce$decontX_contamination))
