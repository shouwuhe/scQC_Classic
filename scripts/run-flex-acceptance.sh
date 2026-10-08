#!/bin/bash
# 真实 Flex 数据验收（SPEC §15 阶段 0 验收的"一份 Flex 数据"）。
# 数据从 10x 官网下载到 ~/TEST，预处理后放在 ~/TEST/_scqc_prep：
#   kidney      —— GEM-X Flex 单重、人肾单核（Cell Ranger 9.0.0）
#   pbmc_merged —— Flex v2/Apex 4-plex 人 PBMC，4 个样本的 per-sample 矩阵已合并（Cell Ranger 10.0.0）
set -u
P="$HOME/TEST/_scqc_prep"
E=./target.nosync/release/scqc-engine

echo "==================== 1. 编译与自测 ===================="
cargo build --release --workspace 2>&1 | tail -3 || exit 1
cargo test --workspace 2>&1 | grep -E "^test result|FAILED|panicked" | head -6
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | grep -E "^(error|warning)" -A 8 | head -40

echo "==================== 2. Flex 单重（人肾单核）===================="
K="$P/kidney"
rm -rf /tmp/acc_flex_kidney
echo "--- 平台自动识别 ---"
$E detect --raw "$K/raw_feature_bc_matrix" --filtered "$K/sample_filtered_feature_bc_matrix" \
  --metrics "$HOME/TEST/Human_Kidney_4k_GEM-X_Flex/Human_Kidney_4k_GEM-X_Flex_Human_Kidney_4k_GEM-X_Flex_metrics_summary.csv" 2>&1 | grep '"type":"platform"' | cut -c1-800
echo "--- 全流程 ---"
/usr/bin/time -l $E run --raw "$K/raw_feature_bc_matrix" --filtered "$K/sample_filtered_feature_bc_matrix" \
  --metrics "$HOME/TEST/Human_Kidney_4k_GEM-X_Flex/Human_Kidney_4k_GEM-X_Flex_Human_Kidney_4k_GEM-X_Flex_metrics_summary.csv" \
  --platform 10x-Flex-singleplex --output /tmp/acc_flex_kidney 2>&1 \
  | grep -vE '"progress"' | grep -E "INFO|warning|real|maximum resident" | cut -c1-300

echo "==================== 3. Flex 4-plex（人 PBMC，4 样本合并）===================="
M="$P/pbmc_merged"
rm -rf /tmp/acc_flex_pbmc
echo "--- 平台自动识别 ---"
$E detect --raw "$M/raw" --filtered "$M/filtered" 2>&1 | grep '"type":"platform"' | cut -c1-800
echo "--- 全流程 ---"
/usr/bin/time -l $E run --raw "$M/raw" --filtered "$M/filtered" --output /tmp/acc_flex_pbmc 2>&1 \
  | grep -vE '"progress"' | grep -E "INFO|warning|real|maximum resident" | cut -c1-300

echo "==================== 4. 关键结论 ===================="
for d in /tmp/acc_flex_kidney /tmp/acc_flex_pbmc; do
  echo "---- $d ----"
  [ -f "$d/report_data.json" ] || { echo "（没有产出报告）"; continue; }
  python3 - "$d" <<'PY'
import json, sys
d = sys.argv[1]
r = json.load(open(f"{d}/report_data.json"))
print("平台:", r['sample']['platform_label'], "| 多重化:", r['sample']['multiplex_n'], "| 物种:", r['sample']['species'])
print("识别依据:", *["\n  - "+e for e in r['sample']['evidence']])
print("基因对齐:", [i['value'] for i in r['sample']['inputs'] if i['key']=='基因列表对齐'] or "（无需对齐）")
print("线粒体章节:", r['overview']['mito_title'])
print("核糖体:", "显示" if r['overview']['ribo_hist_svg'] else f"不显示 —— {r['overview']['ribo_hidden_reason']}")
print("环境 RNA:", [(e['name'][:30], e['value']) for e in r['ambient']['estimates']], "|", r['ambient']['verdict_zh'], r['ambient']['rating_zh'], r['ambient']['correction_zh'])
print("ddqc 维度:", r['ddqc']['dims_zh'])
print("ddqc:", [(k['key'], k['value']) for k in r['ddqc']['items'][:6]])
print("双细胞:", [(k['key'], k['value']) for k in r['doublet']['items']])
print("各样本:", [(p['gene'], p['a'], p['b'], p['c']) for p in r['doublet']['per_sample']])
print("瀑布:", [(k['key'], k['value']) for k in r['final_result']['waterfall']])
print("耗时(s):", r['sample']['elapsed_s'], "| 警示:", [w['key'] for w in r['warnings']])
PY
done
echo "==================== 5. 可复现性（4-plex 换线程数重跑）===================="
rm -rf /tmp/acc_flex_pbmc_t1
$E run --raw "$M/raw" --filtered "$M/filtered" --output /tmp/acc_flex_pbmc_t1 --threads 2 >/dev/null 2>&1
cmp /tmp/acc_flex_pbmc/qc_metrics.csv /tmp/acc_flex_pbmc_t1/qc_metrics.csv && echo "qc_metrics.csv 逐字节一致 ✅"
cp /tmp/acc_flex_kidney/report.html .agent/report_flex_kidney.html 2>/dev/null
cp /tmp/acc_flex_pbmc/report.html .agent/report_flex_pbmc.html 2>/dev/null
echo "报告已复制到 .agent/"
