import { useCallback, useEffect, useRef, useState } from 'react'
import {
  AlertTriangle,
  CheckCircle2,
  FileText,
  FolderOpen,
  HardDriveDownload,
  Loader2,
  Upload,
} from 'lucide-react'
import { Page, ActionBar } from '@/components/AppShell'
import { Advanced, Badge, Button, Card, Explain, Note, Select } from '@/components/ui/primitives'
import { useT, fill } from '@/i18n'
import { host } from '@/lib/api'
import type { CodedText, NeedInputEvent, PlatformEvent, SampleJob, ScannedSample } from '@/lib/types'
import { cn } from '@/lib/utils'
import { speciesLabel } from './Overview'

/** 只用来兜底（正常情况下主进程已经把 outputParent 给出来了） */
function dirname(p: string): string {
  const i = p.replace(/\/+$/, '').lastIndexOf('/')
  return i > 0 ? p.slice(0, i) : '/'
}

const PLATFORMS = [
  { value: '10x-scRNA', key: 'scRNA' },
  { value: '10x-snRNA', key: 'snRNA' },
  { value: '10x-Flex-singleplex', key: 'flexSingleplex' },
  { value: '10x-Flex-multiplex', key: 'flexMultiplex' },
] as const

/**
 * 第 1 屏：输入。
 *
 * 用户可以拖进来的东西没有形状限制——一个矩阵目录、一个 outs/、一整棵装着几十个样本的
 * 目录树，或者一个还没解压的压缩包。扫描在主进程里做（见 electron/main/scan.ts），
 * 这一屏只负责把结果呈现成"一组样本"，并在引擎拒绝猜测平台时（C9）拦住用户。
 */
export function ImportScreen({
  jobs,
  setJobs,
  baseOut,
  setBaseOut,
  setOutHint,
  onStart,
}: {
  jobs: SampleJob[]
  setJobs: (j: SampleJob[]) => void
  baseOut: string | null
  setBaseOut: (p: string | null) => void
  setOutHint: (h: { parent: string; label: string } | null) => void
  onStart: () => void
}) {
  const { t } = useT()
  const [scanning, setScanning] = useState(false)
  const [dragOver, setDragOver] = useState(false)
  const [nothingFound, setNothingFound] = useState(false)
  const [extractedTo, setExtractedTo] = useState<string | null>(null)
  const dropRef = useRef<HTMLDivElement>(null)

  const accept = useCallback(
    async (path: string) => {
      const h = host()
      setScanning(true)
      setNothingFound(false)
      setExtractedTo(null)
      const scan = await h.scanInput(path)
      if (!scan.ok || scan.samples.length === 0) {
        setNothingFound(true)
        setScanning(false)
        return
      }
      setExtractedTo(scan.extractedTo ?? null)
      const built: SampleJob[] = scan.samples.map((s: ScannedSample) => ({
        ...s,
        include: true,
        platform: null,
        species: null,
        multiplexN: null,
        needPlatform: false,
        evidence: [],
        speciesId: null,
        status: 'pending',
        outDir: null,
        report: null,
        error: null,
      }))
      setJobs(built)
      // 输出建在"用户拖进来的那个东西"旁边，不是第一个样本旁边：
      // 后者会把结果写进被扫描的目录树里（下次再扫就会把它当成新样本）。
      const first = built[0]
      if (first) {
        const hint = {
          parent: scan.outputParent ?? dirname(first.filteredDir),
          label: scan.label ?? first.name,
        }
        setOutHint(hint)
        setBaseOut(await h.suggestOutput(hint.parent, hint.label))
      }

      // 平台识别：样本不多就顺序跑，多了并发 3 个，避免一次 spawn 几十个进程
      const detected = [...built]
      const limit = 3
      let cursor = 0
      const workers = Array.from({ length: Math.min(limit, detected.length) }, async () => {
        for (;;) {
          const i = cursor++
          const job = detected[i]
          if (!job) return
          const d = await h.detect({
            filteredDir: job.filteredDir,
            ...(job.rawDir ? { rawDir: job.rawDir } : {}),
            ...(job.metricsCsv ? { metricsCsv: job.metricsCsv } : {}),
          })
          const evs = d.events ?? []
          const p = evs.find((e) => e.type === 'platform') as PlatformEvent | undefined
          const n = evs.find((e) => e.type === 'need_input') as NeedInputEvent | undefined
          detected[i] = {
            ...job,
            platform: p?.id ?? null,
            multiplexN: p?.multiplex_n ?? null,
            speciesId: p?.species_id ?? null,
            needPlatform: Boolean(n) && !p,
            evidence: (p?.evidence_codes ?? n?.evidence_codes ?? []) as CodedText[],
          }
          setJobs([...detected])
        }
      })
      await Promise.all(workers)
      setScanning(false)
    },
    [setJobs, setBaseOut, setOutHint],
  )

  useEffect(() => {
    const el = dropRef.current
    if (!el) return
    const over = (e: DragEvent) => {
      e.preventDefault()
      setDragOver(true)
    }
    const leave = () => setDragOver(false)
    const drop = (e: DragEvent) => {
      e.preventDefault()
      setDragOver(false)
      const f = e.dataTransfer?.files?.[0]
      if (!f) return
      const h = host()
      const p = h.pathForFile ? h.pathForFile(f) : ''
      if (p) void accept(p)
    }
    el.addEventListener('dragover', over)
    el.addEventListener('dragleave', leave)
    el.addEventListener('drop', drop)
    return () => {
      el.removeEventListener('dragover', over)
      el.removeEventListener('dragleave', leave)
      el.removeEventListener('drop', drop)
    }
  }, [accept])

  const included = jobs.filter((j) => j.include)
  const ready = included.length > 0 && included.every((j) => Boolean(j.platform)) && Boolean(baseOut)
  const single = jobs.length === 1 ? jobs[0] : null
  const patch = (id: string, p: Partial<SampleJob>) => setJobs(jobs.map((j) => (j.id === id ? { ...j, ...p } : j)))

  if (jobs.length === 0) {
    return (
      <Page title={t.import.title}>
        <div
          ref={dropRef}
          className={cn(
            'flex flex-col items-center justify-center rounded-2xl border-2 border-dashed px-6 py-20 text-center transition',
            dragOver
              ? 'border-[var(--color-accent)] bg-[var(--color-accent-soft)]'
              : 'border-[var(--color-line)] bg-[var(--color-surface)]',
          )}
        >
          <Upload
            className={cn('h-10 w-10', dragOver ? 'text-[var(--color-accent)]' : 'text-[var(--color-ink-faint)]')}
          />
          <p className="mt-4 text-[17px] font-medium">{t.import.dropHere}</p>
          <button
            data-smoke="browse"
            className="mt-3 text-[14px] text-[var(--color-accent)] underline-offset-4 hover:underline"
            onClick={async () => {
              const d = await host().chooseFolder(t.import.dropHere)
              if (d) void accept(d)
            }}
          >
            {t.import.orBrowse}
          </button>
          {scanning && (
            <p className="mt-6 flex items-center gap-2 text-[13px] text-[var(--color-ink-faint)]">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              {t.import.scanning}
            </p>
          )}
          {nothingFound && (
            <div className="mt-8 max-w-[56ch] text-left">
              <Note tone="warn" title={t.import.notAMatrixFolder}>
                {t.import.notAMatrixFolderBody}
              </Note>
            </div>
          )}
          <div className="mt-10 max-w-[60ch] text-left">
            <Advanced label={t.import.whatWeNeed}>
              <p className="text-[13px] leading-relaxed text-[var(--color-ink-soft)]">{t.import.whatWeNeedBody}</p>
            </Advanced>
          </div>
        </div>
      </Page>
    )
  }

  return (
    <Page wide={jobs.length > 1} title={t.import.title}>
      {jobs.length > 1 && (
        <div className="mb-4 flex items-center gap-3">
          <Badge tone="accent">{fill(t.import.samplesFound, { n: jobs.length })}</Badge>
          <button
            className="text-[12.5px] text-[var(--color-ink-faint)] hover:text-[var(--color-accent)]"
            onClick={() => setJobs(jobs.map((j) => ({ ...j, include: true })))}
          >
            {t.import.selectAll}
          </button>
          <button
            className="text-[12.5px] text-[var(--color-ink-faint)] hover:text-[var(--color-accent)]"
            onClick={() => setJobs(jobs.map((j) => ({ ...j, include: false })))}
          >
            {t.import.selectNone}
          </button>
          {scanning && (
            <span className="ml-auto flex items-center gap-2 text-[12.5px] text-[var(--color-ink-faint)]">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              {t.import.scanning}
            </span>
          )}
        </div>
      )}

      {extractedTo && (
        <div className="mb-4">
          <Note>{t.import.extracting.replace('…', '')} → <span className="font-mono text-[12px]">{extractedTo}</span></Note>
        </div>
      )}

      {single ? (
        <SingleSample job={single} patch={patch} />
      ) : (
        <SampleTable jobs={jobs} setJobs={setJobs} patch={patch} />
      )}

      <Card className="mt-4 p-5">
        <div className="flex items-start gap-3">
          <HardDriveDownload className="mt-0.5 h-4 w-4 shrink-0 text-[var(--color-ink-faint)]" />
          <div className="min-w-0 flex-1">
            <h2 className="text-[14px] font-semibold">{t.import.outputTitle}</h2>
            <p className="mt-2 truncate font-mono text-[12.5px] text-[var(--color-ink-faint)]" title={baseOut ?? ''}>
              {baseOut ?? '…'}
              {jobs.length > 1 && <span className="ml-1 opacity-70">/&lt;{t.import.sampleColumn}&gt;/</span>}
            </p>
          </div>
          <Button
            variant="ghost"
            size="sm"
            className="shrink-0"
            onClick={async () => {
              const d = await host().chooseFolder(t.import.outputChange)
              if (d) setBaseOut(d)
            }}
          >
            {t.app.change}
          </Button>
        </div>
      </Card>


      <ActionBar>
        <Button variant="ghost" onClick={() => setJobs([])}>
          {t.import.remove}
        </Button>
        <Button data-smoke="run" size="lg" disabled={!ready || scanning} onClick={onStart}>
          {included.length > 1 ? fill(t.import.runN, { n: included.length }) : t.import.start}
        </Button>
      </ActionBar>
    </Page>
  )
}

/* ---------------------------------------------------------------- */

function SingleSample({ job, patch }: { job: SampleJob; patch: (id: string, p: Partial<SampleJob>) => void }) {
  const { t } = useT()
  return (
    <div className="space-y-4">
      <FileRow label={t.import.filteredLabel} path={job.filteredDir} required mono />
      <FileRow label={t.import.rawLabel} path={job.rawDir} badge={t.import.recommended} mono />
      <FileRow label={t.import.metricsLabel} path={job.metricsCsv} badge={t.import.optional} icon="file" mono />

      {!job.rawDir && (
        <Note tone="warn" title={t.import.rawMissingTitle}>
          {t.import.rawMissingBody}
        </Note>
      )}

      <Card className="p-5">
        <div className="flex items-start justify-between gap-6">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h2 className="text-[15px] font-semibold">{t.import.platformTitle}</h2>
              <Explain>{t.import.platformWhyMattersBody}</Explain>
              {job.platform && !job.needPlatform && (
                <Badge tone="good">
                  <CheckCircle2 className="h-3 w-3" />
                  {t.import.platformDetected}
                </Badge>
              )}
              {job.needPlatform && (
                <Badge tone="warn">
                  <AlertTriangle className="h-3 w-3" />
                  {t.import.platformConfirm}
                </Badge>
              )}
            </div>
            {job.needPlatform && (
              <p className="mt-2 max-w-[64ch] text-[13.5px] text-[var(--color-ink-soft)]">
                {t.import.platformNeedInput}
              </p>
            )}
            <div className="mt-3 flex flex-wrap items-center gap-3">
              <PlatformSelect job={job} patch={patch} />
              {job.multiplexN != null && (
                <span className="text-[13px] text-[var(--color-ink-soft)]">
                  {t.import.multiplexN}: <span className="tnum font-medium">{job.multiplexN}</span>
                </span>
              )}
              {job.speciesId && (
                <span className="text-[13px] text-[var(--color-ink-soft)]">
                  {t.import.species}: <span className="font-medium">{speciesLabel(t, job.speciesId)}</span>
                </span>
              )}
            </div>
          </div>
          <dl className="shrink-0 text-right text-[13px]">
            <dt className="text-[12px] text-[var(--color-ink-faint)]">{t.exportScreen.sizeOnDisk}</dt>
            <dd className="tnum font-medium">{fmtBytes(job.bytes)}</dd>
          </dl>
        </div>
      </Card>
    </div>
  )
}

function SampleTable({
  jobs,
  setJobs,
  patch,
}: {
  jobs: SampleJob[]
  setJobs: (j: SampleJob[]) => void
  patch: (id: string, p: Partial<SampleJob>) => void
}) {
  const { t } = useT()
  const needing = jobs.filter((j) => j.needPlatform)
  return (
    <Card className="overflow-hidden">
      <table className="w-full text-[13px]">
        <thead>
          <tr className="border-b border-[var(--color-line)] text-left text-[12px] text-[var(--color-ink-faint)]">
            <th className="whitespace-nowrap px-4 py-2.5 font-medium">{t.import.includeColumn}</th>
            <th className="px-2 py-2.5 font-medium">{t.import.sampleColumn}</th>
            <th className="px-2 py-2.5 font-medium">{t.import.matrixColumn}</th>
            <th className="px-2 py-2.5 font-medium">{t.import.assayColumn}</th>
            <th className="px-4 py-2.5 text-right font-medium">{t.exportScreen.sizeOnDisk}</th>
          </tr>
        </thead>
        <tbody>
          {jobs.map((j) => (
            <tr key={j.id} className="border-b border-[var(--color-line)] last:border-b-0 align-middle">
              <td className="px-4 py-2.5">
                <input
                  type="checkbox"
                  checked={j.include}
                  onChange={(e) => patch(j.id, { include: e.target.checked })}
                  className="h-4 w-4 accent-[var(--color-accent)]"
                />
              </td>
              <td className="px-2 py-2.5">
                <div className="font-medium">{j.name}</div>
                {!j.rawDir && (
                  <div className="text-[11.5px] text-[var(--color-warn)]">{t.import.rawMissingTitle}</div>
                )}
              </td>
              <td className="px-2 py-2.5">
                <div className="truncate font-mono text-[11.5px] text-[var(--color-ink-faint)]" title={j.filteredDir}>
                  {shortPath(j.filteredDir)}
                </div>
              </td>
              <td className="px-2 py-2.5">
                {j.needPlatform ? (
                  <PlatformSelect job={j} patch={patch} compact />
                ) : j.platform ? (
                  <span className="text-[12.5px]">{platformName(t, j.platform)}</span>
                ) : (
                  <Loader2 className="h-3.5 w-3.5 animate-spin text-[var(--color-ink-faint)]" />
                )}
              </td>
              <td className="tnum px-4 py-2.5 text-right text-[12.5px] text-[var(--color-ink-faint)]">
                {fmtBytes(j.bytes)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {needing.length > 1 && (
        <div className="flex items-center gap-3 border-t border-[var(--color-line)] bg-[var(--color-canvas)] px-4 py-3">
          <AlertTriangle className="h-4 w-4 shrink-0 text-[var(--color-warn)]" />
          <span className="text-[12.5px] text-[var(--color-ink-soft)]">{t.import.platformNeedInput}</span>
          <span className="ml-auto shrink-0">
            <Select
              value=""
              onChange={(v) =>
                setJobs(jobs.map((j) => (j.needPlatform ? { ...j, platform: v, needPlatform: false } : j)))
              }
              options={[
                { value: '', label: t.import.applyToAll },
                ...PLATFORMS.map((p) => ({ value: p.value as string, label: t.import.platformLabels[p.key] })),
              ]}
              className="min-w-[280px]"
            />
          </span>
        </div>
      )}
    </Card>
  )
}

function PlatformSelect({
  job,
  patch,
  compact = false,
}: {
  job: SampleJob
  patch: (id: string, p: Partial<SampleJob>) => void
  compact?: boolean
}) {
  const { t } = useT()
  return (
    <Select
      value={job.platform ?? ''}
      onChange={(v) => patch(job.id, { platform: v, needPlatform: false })}
      options={PLATFORMS.map((p) => ({ value: p.value as string, label: t.import.platformLabels[p.key] }))}
      className={compact ? 'min-w-[230px]' : 'min-w-[280px]'}
    />
  )
}

function FileRow({
  label,
  path,
  required = false,
  badge,
  icon = 'folder',
  mono = false,
}: {
  label: string
  path: string | null
  required?: boolean
  badge?: string
  icon?: 'folder' | 'file'
  mono?: boolean
}) {
  const { t } = useT()
  return (
    <Card className={cn('flex items-center gap-3 px-5 py-4', !path && 'opacity-70')}>
      <span className="shrink-0 text-[var(--color-accent)]">
        {icon === 'folder' ? <FolderOpen className="h-4 w-4" /> : <FileText className="h-4 w-4" />}
      </span>
      <span className={cn('shrink-0 text-[13.5px]', mono && 'font-mono text-[13px]')}>{label}</span>
      {badge && !path && (
        <Badge tone="neutral" className="shrink-0">
          {badge}
        </Badge>
      )}
      {path ? (
        <span className="ml-auto min-w-0 truncate text-right font-mono text-[12px] text-[var(--color-ink-faint)]" title={path}>
          {shortPath(path)}
        </span>
      ) : (
        <span className="ml-auto shrink-0 text-[12.5px] text-[var(--color-ink-faint)]">
          {required ? '' : t.app.none}
        </span>
      )}
      {path && required && <CheckCircle2 className="h-4 w-4 shrink-0 text-[var(--color-good)]" />}
    </Card>
  )
}

/* ---------------------------------------------------------------- */

function platformName(t: { facts: { platform: Record<string, string> } }, id: string): string {
  return t.facts.platform[id] ?? id
}

/** 只留最后三段，避免长路径把表格撑开。 */
function shortPath(p: string): string {
  const parts = p.split('/').filter(Boolean)
  return (parts.length > 3 ? '…/' : '/') + parts.slice(-3).join('/')
}

function fmtBytes(b: number): string {
  if (!b) return '—'
  if (b < 1024) return `${b} B`
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(0)} KB`
  if (b < 1024 * 1024 * 1024) return `${(b / 1024 / 1024).toFixed(1)} MB`
  return `${(b / 1024 / 1024 / 1024).toFixed(2)} GB`
}

/** code + args → 本地化文案；未知 code 时退化成 code 本身（不会崩，也不会露出中文）。 */
export function evidenceText(dict: Record<string, string>, e: CodedText): string {
  const tpl = dict[e.code]
  if (!tpl) return e.code
  const vars: Record<string, string> = {}
  e.args.forEach((a, i) => (vars[String(i)] = a))
  return fill(tpl, vars)
}
