/**
 * 输入发现（`electron/main/scan.ts`）的自检：造一棵目录树，看扫描结果对不对。
 *
 * 为什么要有这个：`scanInput` 只能在 Electron 主进程里跑，平时验不到；
 * 而它错一次的后果很具体 ——
 *  1. 把 scQC 自己写出的输出目录当成新样本（输出里的 `filtered/` 完全符合矩阵目录的特征）；
 *  2. 把输出目录建在被扫描的那棵树里面，于是第 1 条必然发生；
 *  3. 拖压缩包时把输出建在系统临时目录里，用户过一阵就找不到结果。
 * 这三条都只在"真的跑一遍"时才暴露，所以在这里用假数据固定住。
 *
 * 跑法：node --experimental-strip-types scripts/check-scan.mts
 */
import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { scanInput } from '../electron/main/scan.ts'

let failed = 0
function check(label: string, cond: boolean, detail?: unknown) {
  if (cond) {
    console.log(`  ✓ ${label}`)
  } else {
    failed++
    console.error(`  ✗ ${label}`, detail ?? '')
  }
}

function matrixDir(dir: string) {
  mkdirSync(dir, { recursive: true })
  for (const f of ['matrix.mtx.gz', 'features.tsv.gz', 'barcodes.tsv.gz']) writeFileSync(join(dir, f), '')
}

const root = mkdtempSync(join(tmpdir(), 'scqc-scan-'))

// —— 一棵典型的树：两个样本 + 一份 scQC 自己的输出 ——
const tree = join(root, 'runs')
for (const s of ['sampleA', 'sampleB']) {
  matrixDir(join(tree, s, 'outs', 'filtered_feature_bc_matrix'))
  matrixDir(join(tree, s, 'outs', 'raw_feature_bc_matrix'))
  writeFileSync(join(tree, s, 'outs', 'metrics_summary.csv'), 'Chemistry\n')
}
const out = join(tree, 'scqc-sampleA-260912')
matrixDir(join(out, 'filtered'))
matrixDir(join(out, 'filtered_all_features'))
writeFileSync(join(out, 'run_params.json'), '{}')
writeFileSync(join(out, 'report.html'), '')

console.log('扫描一棵含两个样本 + 一份历史输出的树：')
const r = await scanInput(tree)
check('找到 2 个样本', r.samples.length === 2, r.samples.map((s) => s.name))
check(
  '没把自己的输出当成样本',
  r.samples.every((s) => !s.filteredDir.startsWith(out)),
  r.samples.map((s) => s.filteredDir),
)
check(
  '每个样本都配上了 raw',
  r.samples.every((s) => s.rawDir !== null),
)
check(
  '每个样本都找到了 metrics_summary.csv',
  r.samples.every((s) => s.metricsCsv !== null),
)
check('输出目录建在这棵树外面', r.outputParent === root, r.outputParent)
check('样本名取自样本目录而不是 outs', r.samples.map((s) => s.name).join(',') === 'sampleA,sampleB')

console.log('直接拖一个矩阵目录进来：')
const one = await scanInput(join(tree, 'sampleA', 'outs', 'filtered_feature_bc_matrix'))
check('找到 1 个样本', one.samples.length === 1, one.samples.length)
check('单样本时用样本名（而不是 filtered_feature_bc_matrix）命名输出', one.label === 'sampleA', one.label)
check('拖矩阵目录时，输出建在样本目录里而不是塞进 outs/', one.outputParent === join(tree, 'sampleA'), one.outputParent)

if (failed) {
  console.error(`\n❌ scan 自检失败 ${failed} 项`)
  process.exit(1)
}
console.log('\n✅ scan 自检通过')
