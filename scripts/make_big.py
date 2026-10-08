#!/usr/bin/env python3
"""由 PBMC 3k 派生一份 **5 万细胞** 的数据集，用于 SPEC §13 / §14.4 的性能验收。

做法：对每个合成细胞，取一个真实细胞与它在表达空间中的一个近邻，按随机权重混合后做泊松抽样。
这样既保留真实的基因共表达结构，又不会退化成"同一个细胞复制 18 份"（那会让聚类产生上千个
微簇，与真实数据的规模特征不符）。另保留一批真实空液滴做环境谱。
输出到指定目录（建议 /tmp，不要放进 iCloud 目录）。

用法：python3 scripts/make_big.py <pbmc3k/input> <输出目录> [细胞数=50000] [空液滴数=200000]
"""
import gzip
import os
import random
import sys

try:
    import numpy as np
except ImportError:
    np = None


def op(path, mode="rt"):
    return gzip.open(path, mode) if str(path).endswith(".gz") else open(path, mode)


def find(d, names):
    for n in names:
        for suffix in ("", ".gz"):
            p = os.path.join(d, n + suffix)
            if os.path.exists(p):
                return p
    raise SystemExit(f"找不到 {names} in {d}")


def read_lines(p):
    with op(p) as f:
        return [l.rstrip("\n") for l in f if l.strip()]


def main():
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    src, out = sys.argv[1], sys.argv[2]
    n_cells = int(sys.argv[3]) if len(sys.argv) > 3 else 50000
    n_empty = int(sys.argv[4]) if len(sys.argv) > 4 else 200000
    rng = random.Random(20260911)
    if np is not None:
        nprng = np.random.default_rng(20260911)

    raw_dir, filt_dir = os.path.join(src, "raw"), os.path.join(src, "filtered")
    feats = read_lines(find(raw_dir, ["features.tsv", "genes.tsv"]))
    raw_bcs = read_lines(find(raw_dir, ["barcodes.tsv"]))
    cell_bcs = set(read_lines(find(filt_dir, ["barcodes.tsv"])))
    n_genes = len(feats)

    print("读取 raw 矩阵…", flush=True)
    cols = {}  # col -> list[(gene, count)]
    totals = {}
    with op(find(raw_dir, ["matrix.mtx"])) as f:
        f.readline()
        line = f.readline()
        while line.startswith("%"):
            line = f.readline()
        for line in f:
            g, c, v = line.split()
            c = int(c)
            v = int(v)
            totals[c] = totals.get(c, 0) + v
            cols.setdefault(c, []).append((int(g), v))
    cell_cols = [i + 1 for i, b in enumerate(raw_bcs) if b in cell_bcs]

    # ---- 近邻表：log1p(CP10k) → 方差前 2000 基因 → SVD 30 维 → 每个细胞的 10 个近邻 ----
    neighbors = None
    if np is not None:
        m = len(cell_cols)
        dense = np.zeros((m, n_genes), dtype=np.float32)
        for j, c in enumerate(cell_cols):
            for g, v in cols[c]:
                dense[j, g - 1] = v
        tot = dense.sum(axis=1, keepdims=True)
        tot[tot == 0] = 1
        dense = np.log1p(dense / tot * 1e4)
        var = dense.var(axis=0)
        hv = np.argsort(var)[-2000:]
        x = dense[:, hv]
        x = (x - x.mean(axis=0)) / (x.std(axis=0) + 1e-8)
        u, sv, _ = np.linalg.svd(x, full_matrices=False)
        pcs = u[:, :30] * sv[:30]
        d2 = (
            (pcs**2).sum(axis=1)[:, None]
            + (pcs**2).sum(axis=1)[None, :]
            - 2 * pcs @ pcs.T
        )
        neighbors = np.argsort(d2, axis=1)[:, 1:11]
        del dense, x, d2
        print("近邻表就绪（每个细胞 10 个近邻）", flush=True)

    empty_cols = [c for c, t in totals.items() if 0 < t <= 100]
    rng.shuffle(empty_cols)
    empty_cols = empty_cols[:n_empty]
    print(f"源：细胞 {len(cell_cols)}，空液滴 {len(empty_cols)}", flush=True)

    letters = "ACGT"

    def rand_bc(i):
        s = "".join(letters[(i >> (2 * k)) & 3] for k in range(8))
        return s + "".join(rng.choice(letters) for _ in range(8)) + "-1"

    os.makedirs(os.path.join(out, "raw"), exist_ok=True)
    os.makedirs(os.path.join(out, "filtered"), exist_ok=True)
    raw_mtx = open(os.path.join(out, "raw", "matrix.mtx"), "w")
    filt_mtx = open(os.path.join(out, "filtered", "matrix.mtx"), "w")
    raw_bc_f = open(os.path.join(out, "raw", "barcodes.tsv"), "w")
    filt_bc_f = open(os.path.join(out, "filtered", "barcodes.tsv"), "w")
    header = "%%MatrixMarket matrix coordinate integer general\n"
    # 先占位维度行，最后回填
    raw_mtx.write(header)
    filt_mtx.write(header)
    dim_pos_raw, dim_pos_filt = raw_mtx.tell(), filt_mtx.tell()
    pad = " " * 40 + "\n"
    raw_mtx.write(pad)
    filt_mtx.write(pad)

    n_raw_nnz = n_filt_nnz = 0
    col_raw = col_filt = 0
    print(f"生成 {n_cells} 个细胞…", flush=True)
    for i in range(n_cells):
        src_col = cell_cols[i % len(cell_cols)]
        entries = cols[src_col]
        col_raw += 1
        col_filt += 1
        bc = rand_bc(i)
        raw_bc_f.write(bc + "\n")
        filt_bc_f.write(bc + "\n")
        if np is not None:
            src_idx = i % len(cell_cols)
            nb = int(neighbors[src_idx][nprng.integers(0, neighbors.shape[1])])
            w = float(nprng.uniform(0.3, 0.7))
            acc = {}
            for gg, vv in entries:
                acc[gg] = acc.get(gg, 0.0) + w * vv
            for gg, vv in cols[cell_cols[nb]]:
                acc[gg] = acc.get(gg, 0.0) + (1.0 - w) * vv
            g = np.fromiter(acc.keys(), dtype=np.int32, count=len(acc))
            lam = np.fromiter(acc.values(), dtype=np.float64, count=len(acc))
            order = np.argsort(g)
            g, lam = g[order], lam[order]
            v = nprng.poisson(lam).astype(np.int32)
            nz = v > 0
            g, v = g[nz], v[nz]
            raw_mtx.writelines(f"{gg} {col_raw} {vv}\n" for gg, vv in zip(g, v))
            filt_mtx.writelines(f"{gg} {col_filt} {vv}\n" for gg, vv in zip(g, v))
            n_raw_nnz += len(g)
            n_filt_nnz += len(g)
        else:
            for gg, vv in entries:
                raw_mtx.write(f"{gg} {col_raw} {vv}\n")
                filt_mtx.write(f"{gg} {col_filt} {vv}\n")
                n_raw_nnz += 1
                n_filt_nnz += 1
        if (i + 1) % 10000 == 0:
            print(f"  {i + 1} 个细胞", flush=True)

    print(f"生成 {len(empty_cols)} 个空液滴…", flush=True)
    for j, src_col in enumerate(empty_cols):
        col_raw += 1
        raw_bc_f.write(rand_bc(n_cells + j) + "\n")
        for gg, vv in cols[src_col]:
            raw_mtx.write(f"{gg} {col_raw} {vv}\n")
            n_raw_nnz += 1

    for f, pos, ncol, nnz in (
        (raw_mtx, dim_pos_raw, col_raw, n_raw_nnz),
        (filt_mtx, dim_pos_filt, col_filt, n_filt_nnz),
    ):
        f.seek(pos)
        f.write(f"{n_genes} {ncol} {nnz}")
        f.close()
    raw_bc_f.close()
    filt_bc_f.close()
    for d in ("raw", "filtered"):
        with open(os.path.join(out, d, "features.tsv"), "w") as f:
            f.write("\n".join(feats) + "\n")
    print(
        f"完成：raw {col_raw} barcode / {n_raw_nnz} 非零；filtered {col_filt} / {n_filt_nnz} → {out}"
    )


if __name__ == "__main__":
    main()
