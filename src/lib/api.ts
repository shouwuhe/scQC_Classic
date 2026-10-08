import type { EngineEvent, ReportData, ScanResult, StepId } from './types'

/**
 * 宿主 API。Electron 里由 preload 注入 `window.scqc`；
 * 在浏览器里（设计评审、截图）自动回落到 mock，读取打包进来的示例报告。
 */
export interface HostApi {
  platform: string
  pathForFile?(f: File): string
  chooseFolder(title?: string): Promise<string | null>
  chooseFile(title?: string, exts?: string[]): Promise<string | null>
  inspectFolder(dir: string): Promise<FolderInfo>
  scanInput(path: string): Promise<ScanResult>
  suggestOutput(parent: string, name: string, tag?: string): Promise<string>
  subOutput(base: string, name: string): Promise<string>
  listOutput(dir: string): Promise<OutputListing>
  readPlots(dir: string, names: string[]): Promise<PlotBundle>
  chooseSavePath(defaultPath: string, ext: string): Promise<string | null>
  detect(args: { rawDir?: string; filteredDir: string; metricsCsv?: string }): Promise<DetectResult>
  run(args: unknown): Promise<{ ok: boolean; code?: number; stderr?: string; error?: string }>
  cancel(): Promise<void>
  /** 回答引擎的 `await_correction`（第 1–7 步已跑完，只等这一个决定） */
  answerCorrection(method: 'none' | 'soup' | 'decontx', force: boolean, runId?: string): Promise<void>
  readReport(dir: string): Promise<ReportData>
  openPath(p: string): Promise<void>
  revealPath(p: string): Promise<void>
  exportPdf(htmlPath: string, pdfPath: string): Promise<void>
  engineVersion(): Promise<string>
  credits(lang: string): Promise<EngineCredits | null>
  onEvent(cb: (e: EngineEvent) => void): () => void
  /** macOS 应用菜单的"关于"；只有 Electron 宿主有 */
  onMenuCredits?(cb: () => void): () => void
}

export interface FolderInfo {
  ok: boolean
  reason?: string
  dir?: string
  files?: { matrix: string | null; features: string | null; barcodes: string | null }
  bytes?: number
  siblings?: string[]
}

export interface CreditMethod {
  key: string
  name: string
  citation: string
  license_note: string
}
export interface EngineCredits {
  lang: string
  version: string
  git_hash: string
  methods: CreditMethod[]
  limitations_global: string[]
}

export interface PlotEntry {
  name: string
  file: string
  shape: number[]
  columns: string[]
}
export interface PlotBundle {
  ok: boolean
  entries: PlotEntry[]
  buffers: Record<string, Uint8Array>
}

export interface OutputListing {
  ok: boolean
  entries: { path: string; bytes: number }[]
}

export interface DetectResult {
  ok: boolean
  code?: number
  events?: EngineEvent[]
  stderr?: string
  error?: string
}

declare global {
  interface Window {
    scqc?: HostApi
  }
}

export const isElectron = typeof window !== 'undefined' && Boolean(window.scqc)

export function host(): HostApi {
  if (typeof window !== 'undefined' && window.scqc) return window.scqc
  return mockHost
}

/* ------------------------------------------------------------------ */
/* 浏览器 mock：用真实运行产出的 report_data.json 驱动，便于设计评审与截图 */
/* ------------------------------------------------------------------ */

import demoReport from '@/assets/demo-report.json'
import demoCredits from '@/assets/demo-credits.json'
import demoPlots from '@/assets/demo-plots.json'

let listeners: ((e: EngineEvent) => void)[] = []

// 步骤名由界面词典给（`t.run.stepNames`），这里只需要 id 的顺序
/** 第 1–7 步；第 8 步（校正）由用户决定要不要跑，第 9 步在那之后 */
const STEPS: StepId[] = ['read', 'platform', 'cell_calling', 'cluster', 'ambient', 'ddqc', 'doublet']

let pendingCorrection: ((m: 'none' | 'soup' | 'decontx') => void)[] = []

const mockHost: HostApi = {
  platform: 'browser',
  pathForFile: (f: File) => `/Users/demo/${f.name}`,
  async chooseFolder() {
    const batch = typeof localStorage !== 'undefined' && localStorage.getItem('scqc.demo') === 'batch'
    return batch ? '/Users/demo/CellRangerRuns' : '/Users/demo/Human_Kidney_4k_Flex/outs/filtered_feature_bc_matrix'
  },
  async chooseFile() {
    return '/Users/demo/Human_Kidney_4k_Flex/outs/metrics_summary.csv'
  },
  async inspectFolder(dir: string) {
    return {
      ok: true,
      dir,
      files: { matrix: 'matrix.mtx.gz', features: 'features.tsv.gz', barcodes: 'barcodes.tsv.gz' },
      bytes: 8_400_000,
      siblings: ['metrics_summary.csv'],
    }
  },
  async scanInput(path: string): Promise<ScanResult> {
    const base = path.replace(/\/$/, '')
    const mk = (n: string) => ({
      id: `${base}/${n}`,
      name: n,
      filteredDir: `${base}/${n}/outs/filtered_feature_bc_matrix`,
      rawDir: `${base}/${n}/outs/raw_feature_bc_matrix`,
      metricsCsv: `${base}/${n}/outs/metrics_summary.csv`,
      bytes: 8_400_000,
    })
    // 设计评审用：demo=batch 时演示批量表格
    const batch = typeof localStorage !== 'undefined' && localStorage.getItem('scqc.demo') === 'batch'
    if (batch) {
      const names = ['Kidney_Flex_A1', 'Kidney_Flex_A2', 'Kidney_Flex_B1', 'Kidney_Flex_B2']
      return { ok: true, samples: names.map(mk), nMatrixDirs: names.length * 2, truncated: false }
    }
    return { ok: true, samples: [mk('Human_Kidney_4k_Flex')], nMatrixDirs: 2, truncated: false }
  },
  async suggestOutput(_parent: string, name: string, tag?: string) {
    return `/Users/demo/scqc-${name || 'sample'}-260912${tag ? `-${tag}` : ''}`
  },
  async subOutput(base: string, name: string) {
    return `${base}/${name}`
  },
  async listOutput() {
    // 真实一次运行（10x Flex 人肾 4k）写出的目录清单
    return {
      ok: true,
      entries: [
        { path: 'ambient_estimates.json', bytes: 5200 },
        { path: 'cell_calling.json', bytes: 436 },
        { path: 'cells_lost.csv', bytes: 33 },
        { path: 'cells_passed.txt', bytes: 79838 },
        { path: 'cells_removed.csv', bytes: 65008 },
        { path: 'cells_rescued.csv', bytes: 33 },
        { path: 'ddqc.json', bytes: 196584 },
        { path: 'detection.json', bytes: 1556 },
        { path: 'doublet.json', bytes: 6494 },
        { path: 'embedding.json', bytes: 2226 },
        { path: 'filtered/barcodes.tsv.gz', bytes: 26315 },
        { path: 'filtered/features.tsv.gz', bytes: 163094 },
        { path: 'filtered/matrix.mtx.gz', bytes: 37663311 },
        { path: 'memory_estimate.json', bytes: 606 },
        { path: 'platform_profile.json', bytes: 2113 },
        { path: 'plots/knee_curve.bin', bytes: 34136 },
        { path: 'plots/manifest.json', bytes: 1620 },
        { path: 'plots/pca.bin', bytes: 926600 },
        { path: 'plots/qc_metrics.bin', bytes: 111192 },
        { path: 'qc_metrics.csv', bytes: 577640 },
        { path: 'raw_input_summary.json', bytes: 1240 },
        { path: 'report.html', bytes: 167017 },
        { path: 'report_data.json', bytes: 89752 },
        { path: 'run_params.json', bytes: 1053 },
      ],
    }
  },
  async chooseSavePath(defaultPath: string) {
    return defaultPath
  },
  async readPlots(_dir: string, _names: string[]) {
    return mockPlots()
  },
  async detect() {
    const r = demoReport as unknown as ReportData
    return {
      ok: true,
      code: 0,
      events: [
        {
          type: 'platform',
          id: r.ui.platform_id,
          species_id: r.ui.species_id,
          multiplex_n: r.ui.multiplex_n,
          evidence_codes: r.ui.evidence,
          user_overrides: r.ui.user_overrides,
        },
      ],
    }
  },
  async run() {
    const t0 = Date.now()
    const r = demoReport as unknown as ReportData
    const march = async (step: StepId, i: number) => {
      for (const pct of [0, 55, 100]) {
        await sleep(90)
        emit({ type: 'progress', step, step_index: i + 1, total_steps: 9, pct, elapsed_s: (Date.now() - t0) / 1000 })
      }
    }
    for (let i = 0; i < STEPS.length; i++) await march(STEPS[i]!, i)

    // 第 1–7 步到此为止，和真引擎一样停下来问一次（见 Event::AwaitCorrection）
    const u = r.ui
    emit({
      type: 'await_correction',
      facts: {
        ambient_available: u.ambient_available,
        rho_soup: u.rho_soup,
        rho_soup_unreliable: u.rho_soup_unreliable,
        rho_decontx: u.rho_decontx,
        rho_mito: u.rho_mito,
        ambient_verdict: u.ambient_verdict,
        ambient_rating: u.ambient_rating,
        ambient_correction_state: u.ambient_correction_state,
        ambient_reference_mean: u.ambient_reference_mean,
        ambient_reference_grey_below: u.ambient_reference_grey_below,
        ambient_reference_discourage_above: u.ambient_reference_discourage_above,
        ambient_top_genes: u.ambient_top_genes.slice(0, 12),
      },
    })
    const answer = await new Promise<'none' | 'soup' | 'decontx'>((resolve) => {
      pendingCorrection.push(resolve)
    })
    if (answer !== 'none') await march('correction', 7)
    await march('output', 8)

    for (const w of r.warnings) emit({ type: 'warning', code: w.key })
    emit({ type: 'result', output_dir: '/Users/demo/scqc-Human_Kidney_4k_Flex-260912', summary: {} })
    return { ok: true, code: 0 }
  },
  async answerCorrection(method) {
    const waiting = pendingCorrection
    pendingCorrection = []
    for (const r of waiting) r(method)
  },
  async cancel() {
    for (const r of pendingCorrection) r('none')
    pendingCorrection = []
    emit({ type: 'error', code: 'CANCELLED' })
  },
  async readReport() {
    return demoReport as unknown as ReportData
  },
  async openPath() {},
  async revealPath() {},
  async exportPdf() {},
  async engineVersion() {
    return '0.1.0 (demo)'
  },
  async credits(lang: string): Promise<EngineCredits | null> {
    // 浏览器 mock：由 `scqc-engine credits` 三语输出预生成，内容与引擎一致
    const all = demoCredits as unknown as Record<string, EngineCredits>
    return all[lang] ?? all.en ?? null
  },
  onEvent(cb) {
    listeners.push(cb)
    return () => {
      listeners = listeners.filter((l) => l !== cb)
    }
  },
}

/**
 * 浏览器 mock 的绘图数据：直接内嵌一次真实运行（10x Flex 人肾 4k）写出的
 * `plots/knee_curve.bin` 与 `plots/qc_metrics.bin`，因此设计评审看到的曲线
 * 与真实结果一致。Electron 里读的仍然是引擎当次运行写出的文件。
 */
function mockPlots(): PlotBundle {
  const d = demoPlots as unknown as {
    entries: PlotEntry[]
    b64: Record<string, string>
  }
  const buffers: Record<string, Uint8Array> = {}
  for (const [k, v] of Object.entries(d.b64)) {
    const bin = atob(v)
    const u8 = new Uint8Array(bin.length)
    for (let i = 0; i < bin.length; i++) u8[i] = bin.charCodeAt(i)
    buffers[k] = u8
  }
  return { ok: true, entries: d.entries, buffers }
}

function emit(e: EngineEvent) {
  for (const l of listeners) l(e)
}
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))
