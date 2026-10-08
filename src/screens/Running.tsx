import { useEffect, useState } from 'react'
import { Check, Loader2, X, AlertTriangle, CircleAlert, Circle, Droplets, TriangleAlert } from 'lucide-react'
import { Page } from '@/components/AppShell'
import { Badge, Button, Card, Note, Progress, Select } from '@/components/ui/primitives'
import { useT, fill } from '@/i18n'
import { pct, secs } from '@/lib/format'
import type { AmbientFacts, ProgressEvent, SampleJob, WarningEvent } from '@/lib/types'
import { pipelineFor } from '@/lib/pipeline'
import { cn } from '@/lib/utils'
import { ambientHeadline, errText, warnText } from './Overview'
import { ConfirmDialog, recLabel, recTone, ReferenceBar, runClass, runVariant, whyText } from './Ambient'

/**
 * 第 2 屏：运行。给初学者一条"正在发生什么"的叙事线，而不是一条百分比。
 * 每一步一行，附一句人话解释；当前步骤展开，已完成的收起。
 */
export function RunningScreen({
  progress,
  warnings,
  error,
  correct,
  awaiting,
  onAnswerCorrection,
  jobs,
  current,
  onCancel,
}: {
  progress: ProgressEvent | null
  warnings: WarningEvent[]
  error: string | null
  /** 这次跑不跑第 8 步；`null` = 还没问用户 */
  correct: boolean | null
  /** 引擎跑完第 1–7 步后停下来等答复时，带过来的环境 RNA 事实 */
  awaiting: AmbientFacts | null
  onAnswerCorrection: (method: 'none' | 'soup' | 'decontx', force: boolean) => void
  /** 本次要跑的样本队列（批量时显示进度与逐项状态） */
  jobs: SampleJob[]
  current: number
  onCancel: () => void
}) {
  const { t } = useT()
  const [confirm, setConfirm] = useState(false)
  const [elapsed, setElapsed] = useState(0)
  useEffect(() => {
    const id = setInterval(() => setElapsed((e) => e + 1), 1000)
    return () => clearInterval(id)
  }, [])

  const steps = pipelineFor(correct)
  const activeIdx = progress ? steps.indexOf(progress.step) : 0

  const batch = jobs.length > 1
  return (
    <Page title={t.run.title}>
      {batch && (
        <div className="mb-4">
          <div className="flex items-baseline justify-between">
            <span className="text-[13.5px] font-medium">
              {fill(t.run.sampleProgress, { i: Math.min(current + 1, jobs.length), n: jobs.length })}
            </span>
            <span className="truncate pl-4 text-[13px] text-[var(--color-ink-soft)]">{jobs[current]?.name}</span>
          </div>
          <Progress className="mt-2" value={(current / jobs.length) * 100} />
        </div>
      )}
      {awaiting && <CorrectionPause facts={awaiting} nSamples={jobs.length} onAnswer={onAnswerCorrection} />}

      <Card className="overflow-hidden">
        {steps.map((s, i) => {
          const state = i < activeIdx ? 'done' : i === activeIdx ? 'active' : 'todo'
          return (
            <div
              key={s}
              className={cn(
                'flex gap-4 border-b border-[var(--color-line)] px-5 py-3.5 last:border-b-0',
                state === 'todo' && 'opacity-45',
              )}
            >
              <div className="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center">
                {state === 'done' && <Check className="h-4 w-4 text-[var(--color-good)]" />}
                {state === 'active' && <Loader2 className="h-4 w-4 animate-spin text-[var(--color-accent)]" />}
                {state === 'todo' && <div className="h-1.5 w-1.5 rounded-full bg-[var(--color-line)]" />}
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex items-baseline justify-between gap-4">
                  <span className={cn('text-[14px]', state === 'active' && 'font-medium')}>{t.run.stepNames[s]}</span>
                  {state === 'active' && progress && (
                    <span className="tnum shrink-0 text-[12px] text-[var(--color-ink-faint)]">{progress.pct}%</span>
                  )}
                </div>
                {state === 'active' && (
                  <>
                    <p className="mt-1 text-[13px] text-[var(--color-ink-soft)]">{t.run.stepHelp[s]}</p>
                    <Progress className="mt-2.5" value={progress?.pct ?? 0} />
                  </>
                )}
              </div>
            </div>
          )
        })}
      </Card>

      {batch && (
        <Card className="mt-4 overflow-hidden">
          <div className="border-b border-[var(--color-line)] px-5 py-2.5 text-[12.5px] text-[var(--color-ink-faint)]">
            {t.run.queue}
          </div>
          <ul className="max-h-56 overflow-y-auto">
            {jobs.map((j, i) => (
              <li
                key={j.id}
                className="flex items-center gap-3 border-b border-[var(--color-line)] px-5 py-2 text-[13px] last:border-b-0"
              >
                <span className="shrink-0">
                  {j.status === 'done' && <Check className="h-3.5 w-3.5 text-[var(--color-good)]" />}
                  {j.status === 'error' && <CircleAlert className="h-3.5 w-3.5 text-[var(--color-bad)]" />}
                  {j.status === 'running' && (
                    <Loader2 className="h-3.5 w-3.5 animate-spin text-[var(--color-accent)]" />
                  )}
                  {(j.status === 'pending' || j.status === 'cancelled') && (
                    <Circle className="h-3 w-3 text-[var(--color-line)]" />
                  )}
                </span>
                <span className={cn('min-w-0 flex-1 truncate', i === current && 'font-medium')}>{j.name}</span>
                {j.status === 'error' && (
                  <Badge tone="bad" className="shrink-0">
                    {t.overview.batchFailed}
                  </Badge>
                )}
              </li>
            ))}
          </ul>
        </Card>
      )}

      <div className="mt-5 flex items-center justify-between">
        <span className="tnum text-[13px] text-[var(--color-ink-faint)]">
          {t.run.elapsed} {secs(progress?.elapsed_s ?? elapsed)}
        </span>
        <Button variant="ghost" onClick={() => setConfirm(true)}>
          <X className="h-4 w-4" />
          {t.app.cancel}
        </Button>
      </div>

      {warnings.length > 0 && (
        <div className="mt-6 space-y-2">
          {warnings.slice(-4).map((w, i) => (
            <div key={i} className="flex gap-2.5 rounded-xl bg-[var(--color-warn-soft)] px-4 py-3 text-[13px]">
              <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-[var(--color-warn)]" />
              <span className="text-[var(--color-ink-soft)]">{warnText(t, w.code) ?? w.code}</span>
            </div>
          ))}
        </div>
      )}

      {error && (
        <div className="mt-6">
          <Note tone="bad" title={t.errors.runFailed}>
            {/* error 是引擎给的错误 code；文案一律来自界面词典（SPEC §10.5） */}
            {errText(t, error)}
          </Note>
        </div>
      )}

      {confirm && (
        <div className="fixed inset-0 z-50 grid place-items-center bg-black/25 px-6">
          <Card className="w-full max-w-md p-6 shadow-xl">
            <h3 className="text-[16px] font-semibold">{t.run.cancelConfirm}</h3>
            <p className="mt-2 text-[13.5px] text-[var(--color-ink-soft)]">{t.run.cancelConfirmBody}</p>
            <div className="mt-6 flex justify-end gap-2">
              <Button variant="secondary" onClick={() => setConfirm(false)}>
                {t.run.keepRunning}
              </Button>
              <Button
                variant="danger"
                onClick={() => {
                  setConfirm(false)
                  onCancel()
                }}
              >
                {t.run.stop}
              </Button>
            </div>
          </Card>
        </div>
      )}
    </Page>
  )
}

/**
 * 第 1–7 步跑完、引擎停在那儿等答复时出现的决策卡（SPEC §11.3）。
 *
 * 为什么放在运行页而不是等跑完再问：前七步是确定性的，为了改"要不要校正"
 * 这一个决定而重跑一遍，跑出来必然是同样的结果。所以引擎停在第 7 步后面等，
 * 界面就地问一次，答完继续跑第 8、9 步——一次运行，一个目录，一份报告。
 */
function CorrectionPause({
  facts,
  nSamples,
  onAnswer,
}: {
  facts: AmbientFacts
  /** 批量时这里给的是整批的平均水平，标出来免得被当成某一个样本的值 */
  nSamples: number
  onAnswer: (method: 'none' | 'soup' | 'decontx', force: boolean) => void
}) {
  const { t } = useT()
  const [method, setMethod] = useState<'soup' | 'decontx'>('soup')
  const [confirm, setConfirm] = useState(false)
  const state = facts.ambient_correction_state
  const discouraged = state === 'greyed' || state === 'discouraged'

  if (!facts.ambient_available) return null

  return (
    <Card className="mb-4 p-6">
      <div className="flex flex-wrap items-center gap-2 text-[var(--color-ink-soft)]">
        <Droplets className="h-4 w-4 text-[var(--color-accent)]" />
        <h3 className="text-[14px] font-semibold text-[var(--color-ink)]">{t.ambientScreen.title}</h3>
        <span className="tnum pl-1 text-[20px] font-semibold text-[var(--color-ink)]">{ambientHeadline(facts)}</span>
        <Badge tone={recTone(state)}>{recLabel(t, state)}</Badge>
        {nSamples > 1 && <Badge tone="neutral">{fill(t.ambientScreen.batchAverage, { n: nSamples })}</Badge>}
      </div>
      <ReferenceBar u={facts} />
      <div className="mt-4 grid gap-x-10 gap-y-1 md:grid-cols-2">
        {(
          [
            [t.overview.ambientMethodA, facts.rho_soup],
            [t.overview.ambientMethodB, facts.rho_decontx],
            [t.overview.ambientMethodC, facts.rho_mito],
            [t.ambientScreen.typical, facts.ambient_reference_mean],
          ] as [string, number | null][]
        ).map(([k, v]) => (
          <div
            key={k}
            className="flex items-baseline justify-between gap-6 border-b border-[var(--color-line)] py-1.5 text-[13px] last:border-b-0"
          >
            <span className="text-[var(--color-ink-soft)]">{k}</span>
            <span className="tnum shrink-0 font-medium">{v != null ? pct(v) : t.app.notAvailable}</span>
          </div>
        ))}
      </div>
      {facts.ambient_top_genes.length > 0 && (
        <div className="mt-4">
          <div className="mb-1.5 text-[12.5px] font-medium text-[var(--color-ink-soft)]">
            {t.ambientScreen.topGenes}
          </div>
          <div className="flex flex-wrap gap-1.5">
            {facts.ambient_top_genes.slice(0, 12).map((g) => (
              <Badge key={g.gene} tone="neutral" className="font-mono">
                {g.gene} <span className="tnum opacity-70">{pct(g.fraction, 0)}</span>
              </Badge>
            ))}
          </div>
        </div>
      )}
      <p className="mt-4 max-w-[80ch] text-[13.5px] leading-relaxed text-[var(--color-ink-soft)]">
        {whyText(t, state)}
      </p>
      {/* 一行到底，全部垂直居中：Field 自带 py-2，跟按钮混排会差 8px */}
      <div className="mt-4 flex flex-wrap items-center gap-3">
        <span className="text-[13px]">{t.ambientScreen.method}</span>
        <Select<'soup' | 'decontx'>
          className="w-[300px]"
          value={method}
          onChange={setMethod}
          options={[
            { value: 'soup', label: t.ambientScreen.methodSoup },
            { value: 'decontx', label: t.ambientScreen.methodDecontx },
          ]}
        />
        <Button
          data-smoke="ambient-run"
          variant={runVariant(state, null, method)}
          className={runClass(state, null, method)}
          onClick={() => (discouraged ? setConfirm(true) : onAnswer(method, false))}
        >
          {discouraged ? <TriangleAlert className="h-4 w-4" /> : <Droplets className="h-4 w-4" />}
          {discouraged ? t.ambientScreen.runAnyway : t.ambientScreen.run}
        </Button>
        <Button data-smoke="ambient-skip" variant="secondary" onClick={() => onAnswer('none', false)}>
          {t.ambientScreen.skip}
        </Button>
      </div>
      {confirm && (
        <ConfirmDialog
          reason={whyText(t, state)}
          onCancel={() => setConfirm(false)}
          onGo={() => onAnswer(method, true)}
        />
      )}
    </Card>
  )
}
