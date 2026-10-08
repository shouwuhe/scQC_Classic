#!/usr/bin/env Rscript
# 只生成 EmptyDrops 的对照结果（依赖少：DropletUtils、Matrix、jsonlite）。
# 用法：Rscript scripts/golden/01_emptydrops.R tests/golden/pbmc3k
# 安装：install.packages(c("BiocManager","jsonlite")); BiocManager::install("DropletUtils")
suppressPackageStartupMessages({ library(Matrix); library(jsonlite); library(DropletUtils) })
args <- commandArgs(trailingOnly = TRUE)
root <- if (length(args) >= 1) args[1] else "tests/golden/pbmc3k"
set.seed(20260911)
raw <- DropletUtils::read10xCounts(file.path(root, "input", "raw"))
m <- SingleCellExperiment::counts(raw)
colnames(m) <- raw$Barcode
ed <- emptyDrops(m, lower = 100, niters = 10000, test.ambient = FALSE)
cells <- colnames(m)[which(ed$FDR <= 0.01)]
d <- file.path(root, "emptydrops"); dir.create(d, showWarnings = FALSE, recursive = TRUE)
writeLines(cells, file.path(d, "cells.txt"))
write.csv(data.frame(barcode = colnames(m), total = ed$Total, logprob = ed$LogProb, pvalue = ed$PValue, fdr = ed$FDR)[!is.na(ed$PValue), ],
          file.path(d, "table.csv"), row.names = FALSE)
write_json(list(lower = 100, niters = 10000, fdr = 0.01, seed = 20260911, package = "DropletUtils",
                version = as.character(packageVersion("DropletUtils")), knee = metadata(ed)$retain),
           file.path(d, "params.json"), auto_unbox = TRUE, pretty = TRUE)
message("EmptyDrops 细胞数：", length(cells), "；retain=", metadata(ed)$retain, "；结果已写入 ", d)
