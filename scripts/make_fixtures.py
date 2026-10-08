#!/usr/bin/env python3
"""生成 tests/fixtures 下的合成小数据集（仅开发用；产品本身不含 Python，见 SPEC C3）。

数据集：
- scrna_mini      ：人 GEX 风格。30 基因（含 6 个 MT-、4 个 RPS/RPL），raw 60 barcode（20 细胞 + 40 空液滴），
                    filtered 20 barcode。附 metrics_summary.csv（宽表，Chemistry = Single Cell 3' v3）。
- flex_multiplex  ：人 Flex 风格。20 基因（含 MT-，无 RPS/RPL），24 nt barcode，3 种 Probe Barcode。
                    附 metrics_summary.csv（长表，Chemistry = Flex）。
- snrna_mini      ：鼠 snRNA 风格。大多数核线粒体计数为 0。
- 中文路径/scrna_mini_gz ：scrna_mini 的 gzip 版本，放在中文目录下（SPEC §14.4）。

所有随机数固定种子，重复运行输出完全一致。
"""
import csv
import gzip
import os
import random
import shutil

ROOT = os.path.join(os.path.dirname(__file__), "..", "tests", "fixtures")
BASES = "ACGT"


def rand_seq(rng, n):
    return "".join(rng.choice(BASES) for _ in range(n))


def write_dir(path, genes, barcodes, counts, gz=False, extra_feature_cols=None):
    os.makedirs(path, exist_ok=True)
    opener = (lambda p: gzip.open(p + ".gz", "wt", newline="\n")) if gz else (lambda p: open(p, "w", newline="\n"))
    with opener(os.path.join(path, "features.tsv")) as f:
        for i, (gid, name) in enumerate(genes):
            row = [gid, name, "Gene Expression"]
            if extra_feature_cols:
                row += [c[i] for c in extra_feature_cols]
            f.write("\t".join(row) + "\n")
    with opener(os.path.join(path, "barcodes.tsv")) as f:
        for b in barcodes:
            f.write(b + "\n")
    nnz = sum(1 for (_, _, v) in counts if v > 0)
    with opener(os.path.join(path, "matrix.mtx")) as f:
        f.write("%%MatrixMarket matrix coordinate integer general\n")
        f.write("%metadata_json: {\"software_version\": \"fixture\"}\n")
        f.write(f"{len(genes)} {len(barcodes)} {nnz}\n")
        for (g, b, v) in counts:
            if v > 0:
                f.write(f"{g + 1} {b + 1} {v}\n")


def synth(rng, genes, n_cells, n_empty, cell_umi, empty_umi, mito_idx, mito_frac, bc_len=16, probe_bcs=None):
    """返回 barcodes, counts(三元组), n_cells。前 n_cells 个 barcode 是细胞。"""
    n_bc = n_cells + n_empty
    barcodes = []
    for i in range(n_bc):
        seq = rand_seq(rng, 16)
        if probe_bcs:
            seq += probe_bcs[i % len(probe_bcs)]
        barcodes.append(seq + "-1")
    counts = []
    n_genes = len(genes)
    other = [g for g in range(n_genes) if g not in mito_idx]
    for b in range(n_bc):
        total = cell_umi if b < n_cells else empty_umi
        total = max(1, int(rng.gauss(total, total * 0.2)))
        mf = mito_frac if b < n_cells else mito_frac * 0.5
        per_gene = {}
        for _ in range(total):
            if mito_idx and rng.random() < mf:
                g = rng.choice(mito_idx)
            else:
                g = rng.choice(other)
            per_gene[g] = per_gene.get(g, 0) + 1
        for g, v in per_gene.items():
            counts.append((g, b, v))
    return barcodes, counts


def scrna(rng):
    names = [f"MT-{n}" for n in ["ND1", "ND2", "CO1", "CO2", "ATP6", "CYB"]]
    names += ["RPS6", "RPL13A", "RPS27", "RPL3"]
    names += [f"GENE{i}" for i in range(20)]
    genes = [(f"ENSG{i:011d}", n) for i, n in enumerate(names)]
    mito = list(range(6))
    barcodes, counts = synth(rng, genes, 20, 40, 400, 15, mito, 0.08)
    d = os.path.join(ROOT, "scrna_mini")
    write_dir(os.path.join(d, "raw"), genes, barcodes, counts)
    cells = set(range(20))
    fcounts = [(g, b, v) for (g, b, v) in counts if b in cells]
    write_dir(os.path.join(d, "filtered"), genes, barcodes[:20], fcounts)
    with open(os.path.join(d, "metrics_summary.csv"), "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["Estimated Number of Cells", "Chemistry", "Median Genes per Cell"])
        w.writerow(["20", "Single Cell 3' v3", "18"])
    # 中文路径 + gzip
    dz = os.path.join(ROOT, "中文路径", "scrna_mini_gz")
    write_dir(os.path.join(dz, "raw"), genes, barcodes, counts, gz=True)
    write_dir(os.path.join(dz, "filtered"), genes, barcodes[:20], fcounts, gz=True)
    shutil.copy(os.path.join(d, "metrics_summary.csv"), os.path.join(dz, "metrics_summary.csv"))


def flex(rng):
    names = [f"MT-{n}" for n in ["ND1", "ND2", "CO1", "CO2", "ATP6", "CYB"]]
    names += [f"GENE{i}" for i in range(14)]
    genes = [(f"ENSG{i:011d}", n) for i, n in enumerate(names)]
    mito = list(range(6))
    probes = ["ACTTTAGG", "CCGGTTAA", "GGTTCCAA"]
    barcodes, counts = synth(rng, genes, 18, 30, 300, 10, mito, 0.02, probe_bcs=probes)
    d = os.path.join(ROOT, "flex_multiplex")
    write_dir(os.path.join(d, "raw"), genes, barcodes, counts)
    cells = set(range(18))
    fcounts = [(g, b, v) for (g, b, v) in counts if b in cells]
    write_dir(os.path.join(d, "filtered"), genes, barcodes[:18], fcounts)
    with open(os.path.join(d, "metrics_summary.csv"), "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["Category", "Library Type", "Grouped By", "Group Name", "Metric Name", "Metric Value"])
        w.writerow(["Library", "Gene Expression", "", "", "Chemistry", "Flex"])
        w.writerow(["Library", "Gene Expression", "", "", "Probe set name", "Chromium_Human_Transcriptome_Probe_Set_v1.0.1_GRCh38-2020-A"])
        w.writerow(["Cells", "Gene Expression", "Sample", "S1", "Cells", "6"])


def snrna(rng):
    names = [f"mt-{n}" for n in ["Nd1", "Nd2", "Co1", "Co2", "Atp6", "Cytb"]]
    names += ["Rps6", "Rpl13a", "Rps27", "Rpl3"]
    names += [f"Gene{i}" for i in range(20)]
    genes = [(f"ENSMUSG{i:011d}", n) for i, n in enumerate(names)]
    mito = list(range(6))
    barcodes, counts = synth(rng, genes, 20, 40, 300, 12, mito, 0.003)
    d = os.path.join(ROOT, "snrna_mini")
    write_dir(os.path.join(d, "raw"), genes, barcodes, counts)
    cells = set(range(20))
    fcounts = [(g, b, v) for (g, b, v) in counts if b in cells]
    write_dir(os.path.join(d, "filtered"), genes, barcodes[:20], fcounts)


if __name__ == "__main__":
    if os.path.isdir(ROOT):
        shutil.rmtree(ROOT)
    scrna(random.Random(1))
    flex(random.Random(2))
    snrna(random.Random(3))
    print("fixtures written to", os.path.abspath(ROOT))
