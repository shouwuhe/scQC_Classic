# scQC

A desktop app that quality-controls single-cell RNA-seq data from 10x Cell Ranger output,
explains every cell it removes, and writes a report you can hand to a collaborator.

Everything runs on your own machine. No data, file paths or results are ever sent anywhere.
No Python or R to install — the analysis engine ships inside the app.

## What it does

Point it at a Cell Ranger output folder and it runs, in order:

1. **Cell calling** — separates cell-containing barcodes from empty droplets
   (knee point + EmptyDrops), recovering real cells Cell Ranger discarded where appropriate.
2. **Low-quality cell filtering** — thresholds are learned *within each cluster* rather than
   applied sample-wide, so genuinely small or mitochondria-rich cell types are not deleted
   wholesale. Fixed floors (mitochondrial fraction, gene count) apply on top.
3. **Doublet detection** — two independent estimators; barcodes called by both are removed,
   borderline calls are removed by default and reported separately.
4. **Ambient RNA** — always measured, with two (three for single-nucleus) independent estimators
   compared against published reference ranges. Removal is **your decision**: the run pauses and
   asks, and either answer continues the same run.

Assay differences are handled explicitly rather than by one-size-fits-all defaults: 3' / 5'
gene expression, Flex (probe-based), and single-nucleus each get their own metric set,
thresholds and caveats. The assay is detected from the input, never guessed silently.

## Output

One folder per run:

```
report.html       Self-contained report. Open in any browser.
matrix/           The filtered count matrix (Matrix Market), all feature types.
qc_metrics.csv    Per-cell metrics for every barcode called a cell.
run_params.json   Every parameter used, so the run can be reproduced.
details/          Per-step results and the barcode list behind every decision.
```

Input files are never modified, and results are never overwritten — each run writes a new folder.

## Install

Download from [Releases](../../releases):

- **macOS (Apple Silicon)** — `.dmg`. Unsigned, so the first launch needs
  right-click → Open → Open.
- **Windows (64-bit)** — `-portable.exe` runs with no install; `-setup.exe` installs for the
  current user. Both unsigned, so SmartScreen asks: More info → Run anyway.

## Build from source

Requires Rust (stable) and Node 20+.

```sh
npm install
npm run typecheck          # tsc + dictionary parity + input-scan self-check
cargo test                 # engine tests
node scripts/pack-mac.mjs arm64   # macOS build
node scripts/pack-win.mjs         # Windows build (needs mingw-w64 when building on macOS)
```

The engine can also be driven directly, without the app:

```sh
cargo build --release
./target/release/scqc-engine detect --raw <raw_dir> --filtered <filtered_dir>
./target/release/scqc-engine run --raw <raw_dir> --filtered <filtered_dir> --output <out_dir>
./target/release/scqc-engine verify     # golden-standard tests
```

`scqc-engine` writes NDJSON on stdout and nothing else, so it can be scripted.

## Layout

- `crates/scqc-core` — algorithms, no UI dependencies
- `crates/scqc-engine` — command-line engine
- `src`, `electron` — desktop app (React + Electron), three languages (EN / 中文 / 日本語)
- `tests` — synthetic fixtures and golden-standard comparisons

## Licence

MIT. Every algorithm is reimplemented from its published paper; no GPL-licensed code is used.
Papers and third-party licences are listed in the app's Credits page and in each report.
