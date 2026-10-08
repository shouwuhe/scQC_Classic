import { execFile } from 'node:child_process'
import { existsSync, statSync } from 'node:fs'
import { mkdtemp, readdir, stat } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { basename, dirname, join, sep } from 'node:path'
import { promisify } from 'node:util'

const run = promisify(execFile)

/**
 * 输入发现：用户拖进来的可能是一个矩阵目录、一个 `outs/`、一整个 pipestance 目录、
 * 一棵装着几十个样本的目录树，或者一个还没解压的压缩包。这里把这些情况统一成
 * "一组样本"，每个样本 = 一个 filtered 矩阵目录 + （尽量配上的）raw 目录 + metrics_summary.csv。
 *
 * 不做的事：不解析 .h5（阶段 2 才支持），不跟随符号链接，不写用户目录
 * （压缩包解到系统临时目录，永远不动输入，硬约束 C7）。
 */

export interface ScannedSample {
  id: string
  name: string
  filteredDir: string
  rawDir: string | null
  metricsCsv: string | null
  bytes: number
}

export interface ScanResult {
  ok: boolean
  samples: ScannedSample[]
  /** 压缩包解压到的临时目录（界面据此提示用户结果不在原处） */
  extractedTo?: string
  /** 扫到的矩阵目录总数，含被判为 raw 的那些 */
  nMatrixDirs: number
  truncated: boolean
  reason?: string
  /**
   * 输出目录该建在哪个目录**下面**（与用户拖进来的东西同级）。
   *
   * 为什么不从样本目录往上推：拖一棵树进来时，从第一个样本往上推会把结果写进这棵树里面，
   * 于是下次再拖同一棵树，我们自己写出的 `filtered/` 会被当成新样本扫出来；
   * 拖压缩包时更糟——会写进系统临时目录，用户过一阵就找不到结果了。
   */
  outputParent?: string
  /** 给这次拖入起的名字（输出目录名用它） */
  label?: string
}

const MAX_DEPTH = 8
const MAX_ENTRIES = 40_000
/** 目录名对样本命名没有信息量，往上再找一层 */
const GENERIC = new Set([
  'outs',
  'count',
  'multi',
  'per_sample_outs',
  'filtered_feature_bc_matrix',
  'sample_filtered_feature_bc_matrix',
  'raw_feature_bc_matrix',
  'sample_raw_feature_bc_matrix',
  'raw_probe_bc_matrix',
  'filtered_gene_bc_matrices',
  'raw_gene_bc_matrices',
  'filtered',
  'raw',
  'matrix',
  'matrices',
])
const ARCHIVE_RE = /\.(zip|tar|tar\.gz|tgz|tar\.bz2|tbz|tar\.xz|txz)$/i

function isArchive(p: string): boolean {
  return ARCHIVE_RE.test(p)
}

/**
 * 压缩包 → 系统临时目录。用系统自带的解压工具，不引入依赖。
 *
 * macOS 用 `/usr/bin/unzip`（zip）与 `/usr/bin/tar`（其余）。
 * Windows 10 1803 起自带 `tar.exe`（bsdtar），zip 和 tar 都能解，但**没有** unzip，
 * 所以那边一律走 tar。
 */
export async function extractArchive(archive: string): Promise<string> {
  const dest = await mkdtemp(join(tmpdir(), 'scqc-extract-'))
  const win = process.platform === 'win32'
  const opts = { maxBuffer: 1 << 24, timeout: 600_000 }
  if (/\.zip$/i.test(archive) && !win) {
    await run('/usr/bin/unzip', ['-q', '-o', archive, '-d', dest], opts)
  } else {
    await run(win ? 'tar' : '/usr/bin/tar', ['-xf', archive, '-C', dest], opts)
  }
  return dest
}

async function matrixFilesIn(dir: string, names: string[]): Promise<{ ok: boolean; bytes: number }> {
  const has = (base: string) => names.find((n) => n === base || n === `${base}.gz`) ?? null
  const matrix = has('matrix.mtx')
  const features = has('features.tsv') ?? has('genes.tsv')
  const barcodes = has('barcodes.tsv')
  if (!matrix || !features || !barcodes) return { ok: false, bytes: 0 }
  let bytes = 0
  for (const n of [matrix, features, barcodes]) {
    try {
      bytes += (await stat(join(dir, n))).size
    } catch {
      /* 读不到大小不影响判定 */
    }
  }
  return { ok: true, bytes }
}

interface FoundDir {
  dir: string
  bytes: number
  isRaw: boolean
}

async function walk(root: string): Promise<{ found: FoundDir[]; truncated: boolean }> {
  const found: FoundDir[] = []
  let seen = 0
  let truncated = false
  const queue: { dir: string; depth: number }[] = [{ dir: root, depth: 0 }]
  while (queue.length) {
    const { dir, depth } = queue.shift()!
    let entries: string[]
    try {
      entries = await readdir(dir)
    } catch {
      continue
    }
    seen += entries.length
    if (seen > MAX_ENTRIES) {
      truncated = true
      break
    }
    // scQC 自己的输出目录整棵跳过：里面的 filtered/ 完全符合"矩阵目录"的特征，
    // 不跳的话用户第二次拖同一棵树，会把上次的结果当成新样本扫出来。
    if (entries.includes('run_params.json') && entries.includes('report.html')) continue

    const m = await matrixFilesIn(dir, entries)
    if (m.ok) {
      const base = basename(dir).toLowerCase()
      found.push({ dir, bytes: m.bytes, isRaw: /(^|_)raw(_|$)|raw_feature|raw_gene|raw_probe/.test(base) })
      continue // 矩阵目录内部不必再往下走
    }
    if (depth >= MAX_DEPTH) continue
    for (const name of entries) {
      if (name.startsWith('.') || name === 'node_modules') continue
      const p = join(dir, name)
      try {
        if (!statSync(p).isDirectory()) continue
      } catch {
        continue
      }
      queue.push({ dir: p, depth: depth + 1 })
    }
  }
  return { found, truncated }
}

/** 共同前缀的段数：用来给 filtered 配最近的那个 raw。 */
function commonDepth(a: string, b: string): number {
  const x = a.split(sep)
  const y = b.split(sep)
  let i = 0
  while (i < x.length && i < y.length && x[i] === y[i]) i++
  return i
}

function sampleName(filteredDir: string, root: string): string {
  let d = dirname(filteredDir)
  const rootParent = dirname(root)
  for (let i = 0; i < 5; i++) {
    const b = basename(d)
    if (!GENERIC.has(b.toLowerCase()) && b && d !== rootParent) return b
    const up = dirname(d)
    if (up === d) break
    d = up
  }
  const b = basename(filteredDir)
  return GENERIC.has(b.toLowerCase()) ? basename(dirname(filteredDir)) : b
}

async function findMetrics(filteredDir: string): Promise<string | null> {
  let d = dirname(filteredDir)
  for (let i = 0; i < 4; i++) {
    try {
      const names = await readdir(d)
      const hit =
        names.find((n) => n === 'metrics_summary.csv') ??
        names.find((n) => n.endsWith('metrics_summary.csv')) ??
        names.find((n) => n === 'summary.csv')
      if (hit) return join(d, hit)
    } catch {
      /* 读不到就往上找 */
    }
    const up = dirname(d)
    if (up === d) break
    d = up
  }
  return null
}

export async function scanInput(input: string): Promise<ScanResult> {
  let root = input
  let extractedTo: string | undefined
  if (!existsSync(input)) return { ok: false, samples: [], nMatrixDirs: 0, truncated: false, reason: 'not_found' }
  if (!statSync(input).isDirectory()) {
    if (!isArchive(input)) return { ok: false, samples: [], nMatrixDirs: 0, truncated: false, reason: 'not_a_directory' }
    try {
      root = await extractArchive(input)
      extractedTo = root
    } catch (e) {
      return { ok: false, samples: [], nMatrixDirs: 0, truncated: false, reason: `extract_failed: ${String(e)}` }
    }
  }

  const { found, truncated } = await walk(root)
  const filtered = found.filter((f) => !f.isRaw)
  const raws = found.filter((f) => f.isRaw)

  // raw 目录有时是唯一的（multi 流程：一个 raw 对应多个 per-sample filtered），
  // 所以按"共同路径最长"配对，而不是要求同一个父目录。
  const samples: ScannedSample[] = []
  const usedNames = new Map<string, number>()
  for (const f of filtered) {
    let best: FoundDir | null = null
    let bestDepth = -1
    for (const r of raws) {
      const d = commonDepth(f.dir, r.dir)
      if (d > bestDepth) {
        bestDepth = d
        best = r
      }
    }
    let name = sampleName(f.dir, root)
    const n = (usedNames.get(name) ?? 0) + 1
    usedNames.set(name, n)
    if (n > 1) name = `${name} (${n})`
    samples.push({
      id: f.dir,
      name,
      filteredDir: f.dir,
      rawDir: best?.dir ?? null,
      metricsCsv: await findMetrics(f.dir),
      bytes: f.bytes + (best?.bytes ?? 0),
    })
  }

  // 只有 raw、没有 filtered：把 raw 当作唯一输入也比什么都不给强
  if (samples.length === 0 && raws.length > 0) {
    for (const r of raws) {
      samples.push({
        id: r.dir,
        name: sampleName(r.dir, root),
        filteredDir: r.dir,
        rawDir: null,
        metricsCsv: await findMetrics(r.dir),
        bytes: r.bytes,
      })
    }
  }

  samples.sort((a, b) => a.name.localeCompare(b.name))
  const label =
    samples.length === 1
      ? samples[0]!.name
      : basename(input).replace(/\.(zip|tgz|tar|tar\.gz|tar\.bz2|tar\.xz)$/i, '') || 'samples'

  // 输出建在"被拖进来的那个东西"旁边。但如果那个东西本身是矩阵目录或 outs/ 这类
  // 没有信息量的目录，就再往上一层——没人愿意在 Cell Ranger 的 outs/ 里多出一个结果目录。
  let outputParent = dirname(input)
  for (let i = 0; i < 3 && GENERIC.has(basename(outputParent).toLowerCase()); i++) {
    const up = dirname(outputParent)
    if (up === outputParent) break
    outputParent = up
  }
  return {
    ok: samples.length > 0,
    samples,
    ...(extractedTo ? { extractedTo } : {}),
    nMatrixDirs: found.length,
    truncated,
    outputParent,
    label,
  }
}
