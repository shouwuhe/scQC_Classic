#!/usr/bin/env Rscript
# 双细胞对照：scds::cxds（MIT）与 DoubletFinder（CC0）在同一细胞集合上运行。
# 细胞集合 = ddqc/kept.txt（ddqcR 保留的细胞；与 SPEC §8.5.5"过滤后再检测"一致，且不依赖本工具结果）。
# 用法：Rscript scripts/golden/04_doublet.R tests/golden/pbmc3k
suppressPackageStartupMessages({
  library(Matrix); library(jsonlite); library(DropletUtils); library(Seurat)
  library(scds); library(SingleCellExperiment); library(DoubletFinder)
})
args <- commandArgs(trailingOnly = TRUE)
root <- if (length(args) >= 1) args[1] else "tests/golden/pbmc3k"
SEED <- 20260911
set.seed(SEED)
wjson <- function(x, f) write_json(x, f, auto_unbox = TRUE, pretty = TRUE, digits = NA)

x <- DropletUtils::read10xCounts(file.path(root, "input", "raw"))
m <- SingleCellExperiment::counts(x); colnames(m) <- x$Barcode; rownames(m) <- make.unique(rowData(x)$Symbol)
kept <- readLines(file.path(root, "ddqc", "kept.txt"))
cells <- m[, kept]
message("细胞数：", ncol(cells))

# ---------- cxds ----------
d <- file.path(root, "cxds"); dir.create(d, showWarnings = FALSE, recursive = TRUE)
sce <- SingleCellExperiment(assays = list(counts = cells))
sce <- cxds(sce, ntop = 500, binThresh = 0)
write.csv(data.frame(barcode = colnames(sce), score = sce$cxds_score), file.path(d, "scores.csv"), row.names = FALSE)
wjson(list(ntop = 500, binThresh = 0, n_cells = ncol(cells), package = "scds",
           version = as.character(packageVersion("scds"))), file.path(d, "params.json"))

# ---------- DoubletFinder ----------
d <- file.path(root, "doubletfinder"); dir.create(d, showWarnings = FALSE, recursive = TRUE)
so <- CreateSeuratObject(cells)
so <- NormalizeData(so, verbose = FALSE) |> FindVariableFeatures(nfeatures = 2000, verbose = FALSE) |>
  ScaleData(verbose = FALSE) |> RunPCA(npcs = 50, verbose = FALSE)
sweep <- paramSweep(so, PCs = 1:50, sct = FALSE)
stats <- summarizeSweep(sweep, GT = FALSE)
bcmvn <- find.pK(stats)
pK <- as.numeric(as.character(bcmvn$pK[which.max(bcmvn$BCmetric)]))
rate <- 0.008 * ncol(so) / 1000
nExp <- round(rate * ncol(so))
run_df <- function(obj, seed, tag) {
  set.seed(seed)
  obj <- doubletFinder(obj, PCs = 1:50, pN = 0.25, pK = pK, nExp = nExp, sct = FALSE)
  col <- grep("^DF.classifications", colnames(obj@meta.data), value = TRUE)[1]
  pcol <- grep("^pANN", colnames(obj@meta.data), value = TRUE)[1]
  dbl <- colnames(obj)[obj@meta.data[[col]] == "Doublet"]
  writeLines(dbl, file.path(d, paste0("doublets", tag, ".txt")))
  pann <- obj@meta.data[[pcol]]
  write.csv(data.frame(barcode = colnames(obj), pann = pann), file.path(d, paste0("pann", tag, ".csv")), row.names = FALSE)
  list(dbl = dbl, pann = pann)
}
r1 <- run_df(so, SEED, "")
# 自身随机性上限：同一 pK/pN，换随机种子重新生成人工双细胞（人工双细胞配对是随机的）。
# 任何实现与 DoubletFinder 的一致性都不可能超过它与自己的一致性，verify 以此作为门槛依据。
r2 <- run_df(so, SEED + 1, "_seed2")
r3 <- run_df(so, SEED + 2, "_seed3")
dbl <- r1$dbl
jac <- function(a, b) length(intersect(a, b)) / length(union(a, b))
self_j <- c(jac(r1$dbl, r2$dbl), jac(r1$dbl, r3$dbl), jac(r2$dbl, r3$dbl))
self_rho <- c(cor(r1$pann, r2$pann, method = "spearman"), cor(r1$pann, r3$pann, method = "spearman"),
              cor(r2$pann, r3$pann, method = "spearman"))
message("DoubletFinder 换种子自身 Jaccard：", paste(round(self_j, 3), collapse = " / "),
        "；pANN Spearman：", paste(round(self_rho, 3), collapse = " / "))
write.csv(data.frame(pK = as.numeric(as.character(bcmvn$pK)), bcmvn = bcmvn$BCmetric), file.path(d, "sweep.csv"), row.names = FALSE)
wjson(list(pN = 0.25, pK = pK, expected_rate = rate, nExp = nExp, n_cells = ncol(so), seed = SEED,
           self_jaccard_across_seeds = self_j, self_pann_spearman_across_seeds = self_rho,
           package = "DoubletFinder", version = as.character(packageVersion("DoubletFinder"))),
      file.path(d, "params.json"))
message("DoubletFinder 双细胞数：", length(dbl), "，pK=", pK)
