#!/usr/bin/env Rscript
# ddqc 对照：ddqcR（BSD-3）在本工具判定的细胞集合上运行。
# 用法：Rscript scripts/golden/03_ddqc.R tests/golden/pbmc3k
# 注意：ddqcR 内部用 Seurat 聚类（res 1.3），与本工具的 Leiden 精细聚类不同，Jaccard 反映端到端一致性。
suppressPackageStartupMessages({ library(Matrix); library(jsonlite); library(DropletUtils); library(Seurat); library(ddqcR); library(SingleCellExperiment) })
args <- commandArgs(trailingOnly = TRUE)
root <- if (length(args) >= 1) args[1] else "tests/golden/pbmc3k"
set.seed(20260911)
x <- DropletUtils::read10xCounts(file.path(root, "input", "raw"))
m <- SingleCellExperiment::counts(x); colnames(m) <- x$Barcode; rownames(m) <- make.unique(rowData(x)$Symbol)
cl <- read.csv(file.path(root, "scqc_clusters.csv"), stringsAsFactors = FALSE)
cells <- m[, cl$barcode]
so <- CreateSeuratObject(cells)
so <- initialQC(so)                                     # 基因 ≥ 100 且线粒体 ≤ 80%
df <- ddqc.metrics(so, res = 1.3, threshold = 2, do.counts = TRUE, do.genes = TRUE, do.mito = TRUE, do.ribo = TRUE,
                   n.genes.lower.bound = 200, percent.mito.upper.bound = 10)
so2 <- filterData(so, df)
d <- file.path(root, "ddqc"); dir.create(d, showWarnings = FALSE, recursive = TRUE)
writeLines(colnames(so2), file.path(d, "kept.txt"))
# ddqcR 使用的 Seurat 簇标签（供 verify 的"同簇标签"比对，隔离 MAD 逻辑本身）
write.csv(data.frame(barcode = rownames(df), cluster = as.integer(as.character(df$cluster_labels)), passed = df$passed.qc),
          file.path(d, "clusters.csv"), row.names = FALSE)
write_json(list(res = 1.3, mad = 2, dims = c("counts", "genes", "mito", "ribo"), n_genes_lower_bound = 200,
                percent_mito_upper_bound = 10, n_input = ncol(cells), n_after_initial = ncol(so), n_kept = ncol(so2),
                package = "ddqcR", version = as.character(packageVersion("ddqcR"))),
           file.path(d, "params.json"), auto_unbox = TRUE, pretty = TRUE)
message("ddqcR：输入 ", ncol(cells), "，粗过滤后 ", ncol(so), "，保留 ", ncol(so2))
