import { useEffect, useState } from 'react'
import {
  ArrowLeft,
  CheckCircle2,
  FileText,
  FolderOpen,
  Loader2,
  Printer,
  RefreshCw,
  Table2,
  FileArchive,
} from 'lucide-react'
import { Page, ActionBar } from '@/components/AppShell'
import { Advanced, Badge, Button, Card, Note } from '@/components/ui/primitives'
import { useT } from '@/i18n'
import { num, pct, secs } from '@/lib/format'
import { host } from '@/lib/api'
import type { ReportData, SampleJob } from '@/lib/types'
import { cn } from '@/lib/utils'
import { platformLabel, speciesLabel } from './Overview'

type Group = 'matrix' | 'qcMetrics' | 'report' | 'reportPdf' | 'params' | 'details' | 'other'

/** 展示顺序 = 重要性顺序：先给下游分析要用的矩阵，再给可追溯的表，最后是诊断产物。 */
const GROUP_ORDER: Group[] = ['matrix', 'report', 'reportPdf', 'qcMetrics', 'params', 'details', 'other']

/**
 * 第 5 屏：导出。
 *
 * 引擎已经把所有文件写到输出目录了——这一屏不是"再导出一次"，而是
 * 告诉用户东西在哪、每个文件是什么、下一步该拿哪一个去做分析。
 */
export function ExportScreen({
  report,
  outDir,
  jobs,
  activeId,
  onSelect,
  onBack,
  onRestart,
}: {
  report: ReportData
  outDir: string
  jobs: SampleJob[]
  activeId: string
  onSelect: (id: string) => void
  onBack: () => void
  onRestart: () => void
}) {
  const { t, lang } = useT()
  const u = report.ui
  const [entries, setEntries] = useState<{ path: string; bytes: number }[]>([])
  const [pdf, setPdf] = useState<'idle' | 'busy' | 'done'>('idle')

  useEffect(() => {
    let alive = true
    void host()
      .listOutput(outDir)
      .then((r) => {
        if (alive) setEntries(r.entries)
      })
    return () => {
      alive = false
    }
  }, [outDir])

  const grouped = groupEntries(entries)

  const exportPdf = async () => {
    const h = host()
    setPdf('busy')
    try {
      const target = await h.chooseSavePath(`${outDir}/report.pdf`, 'pdf')
      if (!target) {
        setPdf('idle')
        return
      }
      await h.exportPdf(`${outDir}/report.html`, target)
      setPdf('done')
    } catch {
      setPdf('idle')
    }
  }

  const ran = jobs.filter((j) => j.status === 'done')
  return (
    <Page wide title={t.exportScreen.title}>
      {ran.length > 1 && (
        <Card className="mb-5 overflow-hidden">
          <div className="border-b border-[var(--color-line)] px-5 py-2.5 text-[12.5px] text-[var(--color-ink-faint)]">
            {t.exportScreen.batchAll}
          </div>
          <ul>
            {ran.map((j) => (
              <li
                key={j.id}
                className={cn(
                  'flex items-center gap-3 border-b border-[var(--color-line)] px-5 py-2.5 text-[13px] last:border-b-0',
                  j.id === activeId && 'bg-[var(--color-canvas)]',
                )}
              >
                <button className="min-w-0 flex-1 truncate text-left font-medium" onClick={() => onSelect(j.id)}>
                  {j.name}
                </button>
                <span className="tnum shrink-0 text-[12.5px] text-[var(--color-ink-faint)]">
                  {num(j.report?.ui.n_final ?? 0, lang)} {t.app.cells}
                </span>
                <Button variant="ghost" size="sm" onClick={() => void host().revealPath(j.outDir ?? '')}>
                  <FolderOpen className="h-3.5 w-3.5" />
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => void host().openPath(`${j.outDir ?? ''}/report.html`)}
                >
                  <FileText className="h-3.5 w-3.5" />
                </Button>
              </li>
            ))}
          </ul>
        </Card>
      )}

      {/* 结果在哪 */}
      <Card className="flex flex-wrap items-center gap-4 px-6 py-5">
        <CheckCircle2 className="h-6 w-6 shrink-0 text-[var(--color-good)]" />
        <div className="min-w-0 flex-1">
          <div className="text-[12.5px] text-[var(--color-ink-faint)]">{t.exportScreen.destination}</div>
          <div className="truncate font-mono text-[13px]" title={outDir}>
            {outDir}
          </div>
        </div>
        <div className="flex shrink-0 gap-2">
          <Button variant="secondary" onClick={() => void host().revealPath(outDir)}>
            <FolderOpen className="h-4 w-4" />
            {t.exportScreen.openFolder}
          </Button>
          <Button onClick={() => void host().openPath(`${outDir}/report.html`)}>
            <FileText className="h-4 w-4" />
            {t.exportScreen.openReport}
          </Button>
        </div>
      </Card>

      <Note tone="accent" title={t.exportScreen.whatNext}>
        {t.exportScreen.whatNextBody}
      </Note>

      {/* 写了哪些文件 */}
      <Card className="mt-5 p-6">
        <h2 className="text-[15px] font-semibold">{t.exportScreen.contents}</h2>
        <ul className="mt-4 divide-y divide-[var(--color-line)]">
          {GROUP_ORDER.filter((g) => (grouped[g]?.files.length ?? 0) > 0).map((g) => {
            const entry = grouped[g]!
            const first = entry.files[0]!
            const label =
              entry.files.length === 1
                ? first.path
                : first.path.includes('/')
                  ? `${first.path.split('/')[0]}/`
                  : `${first.path} +${entry.files.length - 1}`
            return (
              <li key={g} className="flex items-center gap-3 py-2.5">
                <span className="text-[var(--color-ink-faint)]">{iconFor(g)}</span>
                <span className="min-w-0 flex-1">
                  <span className="text-[13.5px]">{t.exportScreen.items[g]}</span>
                  <span className="ml-2 font-mono text-[12px] text-[var(--color-ink-faint)]">{label}</span>
                </span>
                <span className="tnum shrink-0 text-[12.5px] text-[var(--color-ink-faint)]">
                  {fmtBytes(entry.bytes)}
                </span>
              </li>
            )
          })}
        </ul>
        {entries.length === 0 && <p className="mt-3 text-[13px] text-[var(--color-ink-faint)]">{t.app.loading}</p>}

        <div className="mt-5 flex items-center gap-3">
          <Button variant="secondary" size="sm" disabled={pdf === 'busy'} onClick={() => void exportPdf()}>
            {pdf === 'busy' ? <Loader2 className="h-4 w-4 animate-spin" /> : <Printer className="h-4 w-4" />}
            {pdf === 'busy' ? t.exportScreen.exportingPdf : t.exportScreen.exportPdf}
          </Button>
          {pdf === 'done' && <Badge tone="good">{t.exportScreen.pdfDone}</Badge>}
        </div>
      </Card>

      {/* 运行摘要 */}
      <Card className="mt-5 p-6">
        <h2 className="text-[15px] font-semibold">{t.exportScreen.paramsSummary}</h2>
        <dl className="mt-3 grid gap-x-8 gap-y-2 sm:grid-cols-2">
          {(
            [
              [t.import.platformTitle, platformLabel(t, u.platform_id)],
              [t.import.species, speciesLabel(t, u.species_id)],
              [t.import.summaryCells, num(u.n_cellranger_cells, lang)],
              [t.app.kept, num(u.n_final, lang)],
              [t.overview.medianGenes, num(Math.round(u.median_genes), lang)],
              [t.overview.medianMito, pct(u.median_mito)],
              [t.tuning.quality.strict, u.ddqc_strict ? t.app.on : t.app.off],
              [t.tuning.doublet.removeBorderline, u.dbl_remove_borderline ? t.app.on : t.app.off],
              [t.overview.elapsed, secs(u.elapsed_s)],
              [t.tuning.advancedGlobal.seed, String(u.seed)],
              [t.overview.engineVersion, u.version],
            ] as [string, string][]
          ).map(([k, v], i) => (
            <div key={`${k}-${i}`} className="flex items-baseline justify-between gap-4 text-[13px]">
              <dt className="text-[var(--color-ink-soft)]">{k}</dt>
              <dd className="tnum shrink-0 font-medium">{v}</dd>
            </div>
          ))}
        </dl>
        <Advanced label={t.app.details}>
          <pre className="max-h-72 overflow-auto whitespace-pre-wrap break-all font-mono text-[11.5px] leading-relaxed text-[var(--color-ink-soft)]">
            {report.params_json}
          </pre>
        </Advanced>
      </Card>

      <ActionBar>
        <Button variant="secondary" onClick={onBack}>
          <ArrowLeft className="h-4 w-4" />
          {t.app.back}
        </Button>
        <Button variant="secondary" onClick={onRestart}>
          <RefreshCw className="h-4 w-4" />
          {t.exportScreen.reuse}
        </Button>
      </ActionBar>
    </Page>
  )
}

/* ---------------------------------------------------------------- */

function groupEntries(entries: { path: string; bytes: number }[]): Partial<
  Record<Group, { files: { path: string; bytes: number }[]; bytes: number }>
> {
  const out: Partial<Record<Group, { files: { path: string; bytes: number }[]; bytes: number }>> = {}
  const add = (g: Group, e: { path: string; bytes: number }) => {
    const cur = out[g] ?? { files: [], bytes: 0 }
    cur.files.push(e)
    cur.bytes += e.bytes
    out[g] = cur
  }
  for (const e of entries) {
    const p = e.path
    if (p.startsWith('matrix/')) add('matrix', e)
    else if (p === 'qc_metrics.csv') add('qcMetrics', e)
    else if (p === 'report.html') add('report', e)
    else if (p.endsWith('.pdf')) add('reportPdf', e)
    else if (p === 'run_params.json') add('params', e)
    else if (p.startsWith('details/')) add('details', e)
    else add('other', e)
  }
  return out
}

function iconFor(g: Group) {
  const cls = 'h-4 w-4'
  if (g === 'matrix') return <FileArchive className={cls} />
  if (g === 'report' || g === 'reportPdf') return <FileText className={cls} />
  return <Table2 className={cn(cls)} />
}

function fmtBytes(b: number): string {
  if (b < 1024) return `${b} B`
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(0)} KB`
  return `${(b / 1024 / 1024).toFixed(1)} MB`
}
