#!/usr/bin/env python3
"""从 PBMC 3k 派生一份 **Flex 多重化样式** 的数据集，用于端到端验证 Flex 代码路径。

⚠️ 这是派生数据，不是真实 Flex 实验数据：它只用于验证"平台差异如何影响流程"
（24 nt barcode → 多重化识别、核糖体维度禁用、线粒体语义、按 Probe Barcode 分样本、
预期双细胞率按重数缩放、无环境 RNA 参考基准）。真实 Flex 数据的验收仍需负责人提供。

做法（SPEC §7.2）：
1. cell barcode = 16 nt GEM + 8 nt Probe Barcode（4 个 Probe Barcode = 4 个样本）；
2. 基因宇宙模拟 Flex 探针集：**删除全部核糖体蛋白基因**（RPS/RPL 等），
   线粒体基因计数按 0.3 倍降采样（探针覆盖被压低），并把基因数裁到约 18k；
3. raw 目录额外保留一批空液滴（同样带 Probe Barcode），供环境谱使用。

用法：python3 scripts/make_flexlike.py <pbmc3k/input> <输出目录> [每样本细胞数]
"""
import gzip
import os
import random
import re
import sys

RIBO = re.compile(r"^(RP[SL][0-9]|RPLP[0-9]|RPSA)", re.I)
PROBES = ["AACTTTAGG", "ACCTTTAGG", "AGCTTTAGG", "ATCTTTAGG"]  # 占位，取前 8 nt
PROBES = [p[:8] for p in PROBES]
N_EMPTY = 60000
MITO_KEEP = 0.3
TARGET_GENES = 18000


def op(path, mode="rt"):
    return gzip.open(path, mode) if str(path).endswith(".gz") else open(path, mode)


def find(d, names):
    for n in names:
        for suffix in ("", ".gz"):
            p = os.path.join(d, n + suffix)
            if os.path.exists(p):
                return p
    raise SystemExit(f"找不到 {names} in {d}")


def read_features(d):
    p = find(d, ["features.tsv", "genes.tsv"])
    ids, names = [], []
    with op(p) as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            ids.append(parts[0])
            names.append(parts[1] if len(parts) > 1 else parts[0])
    return ids, names


def read_barcodes(d):
    with op(find(d, ["barcodes.tsv"])) as f:
        return [l.strip() for l in f if l.strip()]


def main():
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    src, out = sys.argv[1], sys.argv[2]
    per_sample = int(sys.argv[3]) if len(sys.argv) > 3 else 700
    rng = random.Random(20260911)

    raw_dir, filt_dir = os.path.join(src, "raw"), os.path.join(src, "filtered")
    ids, names = read_features(raw_dir)
    raw_bcs = read_barcodes(raw_dir)
    cell_bcs = set(read_barcodes(filt_dir))
    n_genes = len(ids)

    # ---- 基因子集：删核糖体、保留其余高表达基因至 TARGET_GENES ----
    gene_total = [0] * n_genes
    mtx = find(raw_dir, ["matrix.mtx"])
    print("扫描 raw 矩阵…", flush=True)
    col_total = [0] * (len(raw_bcs) + 1)
    with op(mtx) as f:
        f.readline()
        line = f.readline()
        while line.startswith("%"):
            line = f.readline()
        for line in f:
            g, c, v = line.split()
            g, c, v = int(g), int(c), int(v)
            gene_total[g - 1] += v
            col_total[c] += v
    keep_mask = [False] * n_genes
    cand = [
        (gene_total[i], i)
        for i in range(n_genes)
        if not RIBO.match(names[i]) and gene_total[i] > 0
    ]
    cand.sort(reverse=True)
    for _, i in cand[:TARGET_GENES]:
        keep_mask[i] = True
    kept_genes = [i for i in range(n_genes) if keep_mask[i]]
    newg = {g: k + 1 for k, g in enumerate(kept_genes)}
    is_mito = [names[g].upper().startswith("MT-") for g in kept_genes]
    print(f"基因：{n_genes} → {len(kept_genes)}（核糖体基因已删除）", flush=True)

    # ---- 选细胞与空液滴 ----
    cell_cols = [i + 1 for i, b in enumerate(raw_bcs) if b in cell_bcs]
    rng.shuffle(cell_cols)
    cell_cols = sorted(cell_cols[: per_sample * len(PROBES)])
    empty_cols = [
        i + 1 for i, b in enumerate(raw_bcs) if 0 < col_total[i + 1] <= 100
    ]
    rng.shuffle(empty_cols)
    empty_cols = sorted(empty_cols[:N_EMPTY])
    sample_of = {}
    for k, c in enumerate(cell_cols):
        sample_of[c] = k % len(PROBES)
    for c in empty_cols:
        sample_of[c] = rng.randrange(len(PROBES))
    keep_cols = sorted(set(cell_cols) | set(empty_cols))
    newc = {c: k + 1 for k, c in enumerate(keep_cols)}
    print(f"细胞 {len(cell_cols)}（{len(PROBES)} 个样本），空液滴 {len(empty_cols)}", flush=True)

    def bc_of(c):
        # 真实 Flex 的 cell barcode = 16 nt GEM + 8 nt Probe；PBMC 3k 是 14 nt 的旧化学，
        # 这里补到 16 nt，保证解析出的总长为 24 nt（SPEC §7.2.3）。
        base = raw_bcs[c - 1].split("-")[0][:16].ljust(16, "A")
        return f"{base}{PROBES[sample_of[c]]}-1"

    # ---- 写出 ----
    os.makedirs(os.path.join(out, "raw"), exist_ok=True)
    os.makedirs(os.path.join(out, "filtered"), exist_ok=True)
    raw_rows, filt_rows = [], []
    cellset = set(cell_cols)
    filt_newc = {c: k + 1 for k, c in enumerate(sorted(cell_cols))}
    with op(mtx) as f:
        f.readline()
        line = f.readline()
        while line.startswith("%"):
            line = f.readline()
        for line in f:
            g, c, v = line.split()
            g, c, v = int(g), int(c), int(v)
            if c not in newc or not keep_mask[g - 1]:
                continue
            ng = newg[g - 1]
            if is_mito[ng - 1]:
                v = int(v * MITO_KEEP)
                if v <= 0:
                    continue
            raw_rows.append((ng, newc[c], v))
            if c in cellset:
                filt_rows.append((ng, filt_newc[c], v))

    def write_dir(d, rows, cols, n_cols):
        with open(os.path.join(d, "matrix.mtx"), "w") as f:
            f.write("%%MatrixMarket matrix coordinate integer general\n")
            f.write(f"{len(kept_genes)} {n_cols} {len(rows)}\n")
            rows.sort(key=lambda t: (t[1], t[0]))
            f.writelines(f"{g} {c} {v}\n" for g, c, v in rows)
        with open(os.path.join(d, "features.tsv"), "w") as f:
            f.writelines(
                f"{ids[g]}\t{names[g]}\tGene Expression\n" for g in kept_genes
            )
        with open(os.path.join(d, "barcodes.tsv"), "w") as f:
            f.writelines(f"{bc_of(c)}\n" for c in cols)

    write_dir(os.path.join(out, "raw"), raw_rows, keep_cols, len(keep_cols))
    write_dir(
        os.path.join(out, "filtered"), filt_rows, sorted(cell_cols), len(cell_cols)
    )
    print(
        f"完成：raw {len(keep_cols)} barcode / {len(raw_rows)} 非零，"
        f"filtered {len(cell_cols)} / {len(filt_rows)} → {out}"
    )


if __name__ == "__main__":
    main()
