/** 与 scqc-engine 的 NDJSON 协议（SPEC §10.2）对应的类型。 */

export type StepId =
  | 'read'
  | 'platform'
  | 'cell_calling'
  | 'cluster'
  | 'ambient'
  | 'ddqc'
  | 'doublet'
  | 'correction'
  | 'output'

export interface CodedText {
  code: string
  args: string[]
}
/*
 * 下面这些事件类型**故意不声明引擎的散文字段**（`message` / `label_zh` /
 * `evidence`（中文原句））。
 *
 * 引擎为了命令行和调试会把这些串写成中文；界面是三语的。一旦有人顺手把它渲染出来，
 * 英文界面里就会冒出中文 —— 这事已经发生过一次（运行页的进度说明与告警文字）。
 * 不声明 = TypeScript 直接拦住，不必靠人记得。界面只认稳定的 `code`（+ `args`），
 * 用自己的词典渲染（SPEC §10.5）；要看引擎原文去 stderr 日志。
 */
export interface PlatformEvent {
  type: 'platform'
  id: string
  species_id: string
  multiplex_n: number | null
  evidence_codes: CodedText[]
  user_overrides: CodedText[]
}
export interface ProgressEvent {
  type: 'progress'
  step: StepId
  step_index: number
  total_steps: number
  pct: number
  elapsed_s: number
}
export interface WarningEvent {
  type: 'warning'
  code: string
}
export interface NeedInputEvent {
  type: 'need_input'
  field: string
  options: { value: string }[]
  evidence_codes: CodedText[]
}
/**
 * 引擎的"环境 RNA 部分事实"。第 1–7 步跑完、还没写报告时，引擎把它随
 * `await_correction` 事件发过来，界面据此当场问用户要不要校正。
 * 字段名与 `report_data.json.ui` 里的同名字段完全一致，两个时刻说的是同一件事。
 */
export interface AmbientFacts {
  ambient_available: boolean
  rho_soup: number | null
  rho_soup_unreliable: boolean
  rho_decontx: number | null
  rho_mito: number | null
  ambient_verdict: string
  ambient_rating: string
  ambient_correction_state: 'greyed' | 'available' | 'discouraged' | 'unrated'
  ambient_reference_mean: number | null
  ambient_reference_grey_below: number | null
  ambient_reference_discourage_above: number | null
  ambient_top_genes: UiGene[]
}
export interface AwaitCorrectionEvent {
  type: 'await_correction'
  facts: AmbientFacts
}
export interface ResultEvent {
  type: 'result'
  output_dir: string
  summary: Record<string, unknown>
}
export interface ErrorEvent {
  type: 'error'
  code: string
}
export type EngineEvent =
  | PlatformEvent
  | ProgressEvent
  | WarningEvent
  | NeedInputEvent
  | AwaitCorrectionEvent
  | ResultEvent
  | ErrorEvent


/** 引擎的语言中立事实块（`report_data.json` 的 `ui` 字段）。界面只读这里的数字和 id。 */
export interface UiCount {
  id: string
  n: number
}
export interface UiGene {
  gene: string
  fraction: number
}
/** 校正前后对比的一行（SPEC §8.3.4.1）：计数是这一批细胞在该基因上的合计。 */
export interface UiCorrectedGene {
  gene: string
  before: number
  after: number
  removed_fraction: number
}
export interface UiFacts {
  schema: number
  platform_id: string
  species_id: string
  multiplex_n: number | null
  n_raw_barcodes: number | null
  n_cellranger_cells: number
  n_genes: number
  n_non_gex_excluded: number
  feature_aligned_n_common: number | null
  mito_semantics: 'viability' | 'stripping_quality' | 'dampened'
  show_ribo: boolean
  evidence: CodedText[]
  user_overrides: CodedText[]
  seed: number
  version: string
  elapsed_s: number

  median_umi: number
  median_genes: number
  median_mito: number
  median_ribo: number | null

  cc_mode: 'emptydrops' | 'trust_upstream' | 'raw_missing'
  cc_n_called: number
  cc_n_rescued: number
  cc_n_lost: number
  cc_knee: number
  cc_inflection: number
  cc_lower: number | null
  cc_n_tested: number
  cc_fdr: number
  cc_rescued_median_genes: number
  cc_rescued_top_genes: string[]

  ambient_available: boolean
  rho_soup: number | null
  rho_soup_low: number | null
  rho_soup_high: number | null
  rho_soup_unreliable: boolean
  rho_decontx: number | null
  decontx_converged: boolean | null
  decontx_n_iter: number | null
  rho_mito: number | null
  ambient_relative_diff: number | null
  ambient_verdict: 'agree' | 'disagree' | 'single' | 'unavailable'
  ambient_rating: 'normal' | 'elevated' | 'severe' | 'unrated' | 'unavailable'
  ambient_correction_state: 'greyed' | 'available' | 'discouraged' | 'unrated'
  ambient_reference_mean: number | null
  ambient_reference_grey_below: number | null
  ambient_reference_discourage_above: number | null
  ambient_top_genes: UiGene[]
  correction_applied: 'soup' | 'decontx' | null
  correction_removed_fraction: number | null
  correction_forced: boolean
  correction_top_genes: UiCorrectedGene[]

  ddqc_n_in: number
  ddqc_n_removed_coarse: number
  ddqc_n_removed_total: number
  ddqc_removed_by_dim: UiCount[]
  ddqc_dims: string[]
  ddqc_n_removed_by_fixed: number
  ddqc_fixed_n_removed: number
  ddqc_n_extra_kept: number
  ddqc_extra_kept_median_genes: number
  ddqc_strict: boolean
  ddqc_mad_multiplier: number
  ddqc_fixed_min_genes: number
  ddqc_fixed_max_mito: number
  ddqc_n_clusters: number
  mito_guardrail: number | null

  dbl_n_in: number
  dbl_n_samples: number
  dbl_expected_rate: number
  dbl_rate_per_1000: number
  dbl_n_loaded: number
  dbl_n_high_confidence: number
  dbl_n_borderline: number
  dbl_cxds_n: number
  dbl_df_n: number
  dbl_jaccard: number
  dbl_remove_borderline: boolean
  dbl_n_removed: number

  waterfall: UiCount[]
  removed_by_step: UiCount[]
  n_final: number
  warning_codes: string[]
}

/** `report_data.json` 的结构（引擎 `scqc_core::report::ReportData`）。 */
export interface KeyValue {
  key: string
  value: string
}
export interface GeneRow {
  gene: string
  a: string
  b: string
  c: string
}
export interface EstimateRow {
  name: string
  value: string
  detail: string
}
export interface ReportData {
  title: string
  sample: {
    platform_id: string
    platform_label: string
    evidence: string[]
    user_overrides: string[]
    species: string
    multiplex_n: number | null
    inputs: KeyValue[]
    started_at_utc: string
    version: string
    git_hash: string
    n_raw_barcodes: number | null
    n_cellranger_cells: number
    n_genes: number
    seed: number
    elapsed_s: string
    caveats_platform: string[]
  }
  overview: {
    knee_svg: string
    umi_hist_svg: string
    genes_hist_svg: string
    mito_hist_svg: string
    ribo_hist_svg: string | null
    stats: KeyValue[]
    mito_title: string
    mito_note: string
    ribo_hidden_reason: string | null
  }
  cell_calling: {
    mode_zh: string
    items: KeyValue[]
    n_rescued: number
    rescued_median_genes: string
    rescued_top_genes: string[]
    n_lost: number
    note_zh: string
  }
  ambient: {
    available: boolean
    skipped_reason: string
    estimates: EstimateRow[]
    verdict_zh: string
    rating_zh: string
    correction_zh: string
    reason_zh: string
    relative_diff: string
    interval: string
    reference: KeyValue[]
    reference_note: string
    disagree: boolean
    disagreement_reasons: string[]
    severe: boolean
    severe_text: string[]
    top_genes: GeneRow[]
    correction: unknown | null
  }
  ddqc: {
    items: KeyValue[]
    removed_by_dim: KeyValue[]
    dims_zh: string[]
    notes: string[]
    thresholds: GeneRow[]
  }
  doublet: {
    items: KeyValue[]
    formula: string
    assumption_zh: string
    per_sample: GeneRow[]
    borderline_action: string
  }
  final_result: {
    waterfall: KeyValue[]
    waterfall_svg: string
    items: KeyValue[]
    removed_by_step: KeyValue[]
  }
  methods: {
    key: string
    name: string
    citation: string
    license_note: string
    description: string
    params: [string, string][]
    limitations: string[]
  }[]
  limitations_global: string[]
  params_json: string
  warnings: KeyValue[]
  ui: UiFacts
}

/** 界面自己维护的运行配置（会序列化成引擎的 --params 文件）。 */
export interface RunSettings {
  seed: number
  threads: number
  cellCalling: { niters: number; fdr: number; lower: number | null }
  embed: { nHvg: number; nPcs: number; k: number; resFine: number; resCoarse: number }
  ambient: { decontxMaxIter: number; soupExclFactor: number }
  ddqc: {
    madMultiplier: number
    fixedThresholdsAreHard: boolean
    fixedMinGenes: number
    fixedMaxMito: number
  }
  doublet: { ratePer1000: number | null; nLoaded: number | null; removeBorderline: boolean }
  correction: { method: 'none' | 'soup' | 'decontx'; force: boolean }
  mitoGuardrail: number | null
}

/** 扫描一次输入得到的一个可分析样本。 */
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
  extractedTo?: string
  nMatrixDirs: number
  truncated: boolean
  reason?: string
  /** 输出目录该建在哪个目录下面（= 用户拖进来的东西的同级目录） */
  outputParent?: string
  /** 给这次拖入起的名字 */
  label?: string
}

export type JobStatus = 'pending' | 'running' | 'done' | 'error' | 'cancelled'

/** 扫描结果 + 用户的选择 + 运行状态。批量与单样本走同一套结构。 */
export interface SampleJob extends ScannedSample {
  include: boolean
  platform: string | null
  species: string | null
  multiplexN: number | null
  /** 引擎要求用户确认平台（C9） */
  needPlatform: boolean
  evidence: CodedText[]
  speciesId: string | null
  status: JobStatus
  outDir: string | null
  report: ReportData | null
  error: string | null
}

export interface DatasetInput {
  rawDir: string | null
  filteredDir: string | null
  metricsCsv: string | null
  platform: string | null
  species: string | null
  multiplexN: number | null
  /** 结果写到哪里；C7 要求绝不覆盖输入，界面负责选一个不存在的新目录 */
  outDir: string | null
}

export interface RunOutput {
  outputDir: string
  report: ReportData
  summary: Record<string, unknown>
  warnings: WarningEvent[]
}
