import { useMemo } from 'react'
import { ArrowLeft, Droplets, RotateCw, Sparkles, TriangleAlert, Users, Settings2 } from 'lucide-react'
import { Page, ActionBar } from '@/components/AppShell'
import { Advanced, Badge, Button, Card, Field, NumberInput, Toggle } from '@/components/ui/primitives'
import { useT, fill } from '@/i18n'
import { num, pct } from '@/lib/format'
import { DEFAULT_SETTINGS } from '@/lib/defaults'
import type { ReportData, RunSettings } from '@/lib/types'
import { correctionStateLabel, dimLabel } from './Overview'

/**
 * 第 4 屏：调整过滤。
 *
 * 每一步一张卡片，用同样的四个问题回答："删了多少 / 为什么 / 它们是什么 / 保留会怎样"。
 * 卡片上只有一个开关（真正会改变结论的那个），细参数全部收进"高级设置"，
 * 视觉上弱化成虚线框——新手可以一路只读不动，专家能改到底。
 */
export function TuningScreen({
  report,
  settings,
  baseline,
  setSettings,
  onBack,
  onRerun,
}: {
  report: ReportData
  settings: RunSettings
  /** 产生当前这份报告的那次运行所用的设置：用它判断用户到底改没改 */
  baseline: RunSettings
  setSettings: (s: RunSettings) => void
  onBack: () => void
  onRerun: () => void
}) {
  const { t, lang } = useT()
  const u = report.ui
  // 英文用逗号，中日文用顿号
  const sep = lang === 'en' ? ', ' : '、'
  const dirty = useMemo(() => JSON.stringify(settings) !== JSON.stringify(baseline), [settings, baseline])

  const set = <K extends keyof RunSettings>(k: K, v: RunSettings[K]) => setSettings({ ...settings, [k]: v })

  return (
    <Page wide title={t.tuning.title} subtitle={t.tuning.subtitle}>
      {/* 1. 真细胞 */}
      <Step
        icon={<Users className="h-4 w-4" />}
        title={t.tuning.cells.title}
        lead={t.tuning.cells.lead}
        skipped={u.cc_mode !== 'emptydrops' ? t.tuning.cells.skipped : null}
        answers={[
          [
            t.tuning.q.howMany,
            fill(t.tuning.cells.howMany, { n: num(u.cc_n_lost, lang), r: num(u.cc_n_rescued, lang) }),
          ],
          [t.tuning.q.why, t.tuning.cells.why],
          [
            t.tuning.q.what,
            fill(t.tuning.cells.what, {
              genes: num(Math.round(u.cc_rescued_median_genes), lang),
              top: u.cc_rescued_top_genes.slice(0, 4).join(' / ') || '—',
            }),
          ],
          [t.tuning.q.ifNot, t.tuning.cells.ifNot],
        ]}
        advanced={
          <>
            <Field label={t.tuning.cells.fdr} help={t.tuning.cells.fdrHelp}>
              <NumberInput
                value={settings.cellCalling.fdr}
                step={0.005}
                min={0.001}
                max={0.2}
                onChange={(v) => set('cellCalling', { ...settings.cellCalling, fdr: v })}
              />
            </Field>
            <Field label={t.tuning.cells.lower} help={t.tuning.cells.lowerHelp}>
              <NumberInput
                value={settings.cellCalling.lower ?? 0}
                step={10}
                min={0}
                onChange={(v) => set('cellCalling', { ...settings.cellCalling, lower: v > 0 ? v : null })}
              />
            </Field>
            <Field label={t.tuning.cells.niters} help={t.tuning.cells.niterHelp}>
              <NumberInput
                value={settings.cellCalling.niters}
                step={1000}
                min={1000}
                max={100000}
                onChange={(v) => set('cellCalling', { ...settings.cellCalling, niters: v })}
              />
            </Field>
          </>
        }
      />

      {/* 2. 低质量细胞 */}
      <Step
        icon={<Sparkles className="h-4 w-4" />}
        title={t.tuning.quality.title}
        lead={t.tuning.quality.lead}
        answers={[
          [
            t.tuning.q.howMany,
            fill(t.tuning.quality.howMany, {
              n: num(u.ddqc_n_removed_total, lang),
              total: num(u.ddqc_n_in, lang),
            }),
          ],
          [t.tuning.q.why, fill(t.tuning.quality.why, { mad: String(u.ddqc_mad_multiplier) })],
          [
            t.tuning.q.what,
            fill(t.tuning.quality.what, {
              dims:
                u.ddqc_removed_by_dim
                  .filter((d) => d.n > 0)
                  .map((d) => `${dimLabel(t, d.id)} ${num(d.n, lang)}`)
                  .join(sep) || '—',
            }),
          ],
          [t.tuning.q.ifNot, t.tuning.quality.ifNot],
        ]}
        toggle={{
          label: t.tuning.quality.strict,
          checked: settings.ddqc.fixedThresholdsAreHard,
          onChange: (v) => set('ddqc', { ...settings.ddqc, fixedThresholdsAreHard: v }),
          recommended: t.tuning.quality.strictOn,
          body: settings.ddqc.fixedThresholdsAreHard
            ? fill(t.tuning.quality.strictBody, {
                mito: pct(settings.ddqc.fixedMaxMito, 0),
                genes: num(settings.ddqc.fixedMinGenes, lang),
                n: num(u.ddqc_n_removed_by_fixed, lang),
              })
            : t.tuning.quality.strictOffBody,
          why: t.tuning.quality.strictWhy,
        }}
        advanced={
          <>
            <Field label={t.tuning.quality.mad} help={t.tuning.quality.madHelp}>
              <NumberInput
                value={settings.ddqc.madMultiplier}
                step={0.1}
                min={0.5}
                max={5}
                onChange={(v) => set('ddqc', { ...settings.ddqc, madMultiplier: v })}
              />
            </Field>
            <Field label={t.tuning.quality.minGenes}>
              <NumberInput
                value={settings.ddqc.fixedMinGenes}
                step={50}
                min={0}
                onChange={(v) => set('ddqc', { ...settings.ddqc, fixedMinGenes: v })}
              />
            </Field>
            <Field label={t.tuning.quality.maxMito}>
              <NumberInput
                value={settings.ddqc.fixedMaxMito}
                step={0.01}
                min={0.01}
                max={0.9}
                onChange={(v) => set('ddqc', { ...settings.ddqc, fixedMaxMito: v })}
              />
            </Field>
            <Field label={t.tuning.quality.guardrail} help={t.tuning.quality.guardrailHelp}>
              <NumberInput
                value={settings.mitoGuardrail ?? u.mito_guardrail ?? 0}
                step={0.01}
                min={0}
                max={0.9}
                onChange={(v) => set('mitoGuardrail', v > 0 ? v : null)}
              />
            </Field>
          </>
        }
      />

      {/* 3. 双细胞 */}
      <Step
        icon={<TriangleAlert className="h-4 w-4" />}
        title={t.tuning.doublet.title}
        lead={t.tuning.doublet.lead}
        answers={[
          [
            t.tuning.q.howMany,
            fill(t.tuning.doublet.howMany, {
              n: num(u.dbl_n_high_confidence, lang),
              b: num(u.dbl_n_borderline, lang),
            }),
          ],
          [
            t.tuning.q.why,
            fill(t.tuning.doublet.why, { loaded: num(u.dbl_n_loaded, lang), rate: pct(u.dbl_expected_rate) }),
          ],
          [t.tuning.q.what, fill(t.tuning.doublet.what, { jaccard: u.dbl_jaccard.toFixed(3) })],
          [t.tuning.q.ifNot, t.tuning.doublet.ifNot],
        ]}
        toggle={{
          label: t.tuning.doublet.removeBorderline,
          checked: settings.doublet.removeBorderline,
          onChange: (v) => set('doublet', { ...settings.doublet, removeBorderline: v }),
          recommended: t.tuning.doublet.removeBorderlineOn,
          body: settings.doublet.removeBorderline
            ? fill(t.tuning.doublet.removeBorderlineBody, { n: num(u.dbl_n_borderline, lang) })
            : t.tuning.doublet.removeBorderlineOffBody,
        }}
        advanced={
          <>
            <Field label={t.tuning.doublet.ratePer1000}>
              <NumberInput
                value={settings.doublet.ratePer1000 ?? u.dbl_rate_per_1000}
                step={0.1}
                min={0}
                max={50}
                onChange={(v) => set('doublet', { ...settings.doublet, ratePer1000: v })}
              />
            </Field>
            <Field label={t.tuning.doublet.nLoaded} help={t.tuning.doublet.nLoadedHelp}>
              <NumberInput
                width="w-36"
                value={settings.doublet.nLoaded ?? u.dbl_n_loaded}
                step={1000}
                min={0}
                onChange={(v) => set('doublet', { ...settings.doublet, nLoaded: v > 0 ? v : null })}
              />
            </Field>
          </>
        }
      />

      {/* 4. 环境 RNA 校正 */}
      <Step
        icon={<Droplets className="h-4 w-4" />}
        title={t.tuning.ambient.title}
        lead={t.tuning.ambient.lead}
        skipped={u.ambient_available ? null : t.warn.AMBIENT_SKIPPED}
        answers={[
          [t.tuning.q.howMany, fill(t.tuning.ambient.howMany, { rho: ambientRange(u.rho_soup, u.rho_decontx) })],
          [t.tuning.q.why, t.tuning.ambient.why],
          [
            t.tuning.q.what,
            fill(t.tuning.ambient.what, {
              top: u.ambient_top_genes.slice(0, 5).map((g) => g.gene).join(' / ') || '—',
            }),
          ],
          [t.tuning.q.ifNot, t.tuning.ambient.ifNot],
        ]}
        extra={u.ambient_available ? <CorrectionState report={report} /> : null}
      />

      {/* 全局运行参数 */}
      <Card className="mt-5 p-6">
        <div className="flex items-center gap-2 text-[var(--color-ink-soft)]">
          <Settings2 className="h-4 w-4" />
          <h3 className="text-[14px] font-semibold text-[var(--color-ink)]">{t.tuning.advancedGlobal.title}</h3>
        </div>
        <Advanced label={t.app.advanced}>
          <Field label={t.tuning.advancedGlobal.seed} help={t.tuning.advancedGlobal.seedHelp}>
            <NumberInput width="w-36" value={settings.seed} step={1} onChange={(v) => set('seed', v)} />
          </Field>
          <Field label={t.tuning.advancedGlobal.threads} help={t.tuning.advancedGlobal.threadsHelp}>
            <NumberInput value={settings.threads} step={1} min={0} max={128} onChange={(v) => set('threads', v)} />
          </Field>
          <div className="my-3 border-t border-dashed border-[var(--color-line)]" />
          <Field label={t.tuning.advancedGlobal.nHvg}>
            <NumberInput
              value={settings.embed.nHvg}
              step={100}
              min={200}
              max={10000}
              onChange={(v) => set('embed', { ...settings.embed, nHvg: v })}
            />
          </Field>
          <Field label={t.tuning.advancedGlobal.nPcs}>
            <NumberInput
              value={settings.embed.nPcs}
              step={5}
              min={5}
              max={200}
              onChange={(v) => set('embed', { ...settings.embed, nPcs: v })}
            />
          </Field>
          <Field label={t.tuning.advancedGlobal.k}>
            <NumberInput
              value={settings.embed.k}
              step={1}
              min={3}
              max={100}
              onChange={(v) => set('embed', { ...settings.embed, k: v })}
            />
          </Field>
          <Field label={t.tuning.advancedGlobal.resFine}>
            <NumberInput
              value={settings.embed.resFine}
              step={0.1}
              min={0.1}
              max={5}
              onChange={(v) => set('embed', { ...settings.embed, resFine: v })}
            />
          </Field>
          <Field label={t.tuning.advancedGlobal.resCoarse}>
            <NumberInput
              value={settings.embed.resCoarse}
              step={0.1}
              min={0.1}
              max={5}
              onChange={(v) => set('embed', { ...settings.embed, resCoarse: v })}
            />
          </Field>
          <div className="mt-4 flex justify-end">
            <Button variant="ghost" size="sm" onClick={() => setSettings(baseline)}>
              {t.app.reset}
            </Button>
          </div>
        </Advanced>
      </Card>

      <ActionBar>
        <Button data-smoke="back" variant="secondary" onClick={onBack}>
          <ArrowLeft className="h-4 w-4" />
          {t.app.back}
        </Button>
        <Button size="lg" disabled={!dirty} onClick={onRerun}>
          <RotateCw className="h-4 w-4" />
          {t.tuning.rerun}
        </Button>
      </ActionBar>
    </Page>
  )
}

/* ---------------------------------------------------------------- */

function Step({
  icon,
  title,
  lead,
  answers,
  toggle,
  advanced,
  extra,
  skipped,
}: {
  icon: React.ReactNode
  title: string
  lead: string
  answers: [string, string][]
  toggle?: {
    label: string
    checked: boolean
    onChange: (v: boolean) => void
    recommended?: string
    body: string
    why?: string
  }
  advanced?: React.ReactNode
  extra?: React.ReactNode
  skipped?: string | null
}) {
  const { t } = useT()
  return (
    <Card className="mt-5 p-6">
      <div className="flex items-center gap-2 text-[var(--color-ink-soft)]">
        <span className="text-[var(--color-accent)]">{icon}</span>
        <h3 className="text-[15px] font-semibold text-[var(--color-ink)]">{title}</h3>
        {skipped && <Badge tone="neutral" className="ml-auto">{skipped}</Badge>}
      </div>
      <p className="mt-2 max-w-[78ch] text-[13.5px] leading-relaxed text-[var(--color-ink-soft)]">{lead}</p>

      <dl className="mt-5 grid gap-x-8 gap-y-4 md:grid-cols-2">
        {answers.map(([q, a]) => (
          <div key={q}>
            <dt className="text-[12.5px] font-medium text-[var(--color-ink-faint)]">{q}</dt>
            <dd className="mt-1 text-[13.5px] leading-relaxed text-[var(--color-ink)]">{a}</dd>
          </div>
        ))}
      </dl>

      {toggle && (
        <div className="mt-5 rounded-xl bg-[var(--color-canvas)] px-4 py-3.5">
          <div className="flex items-center gap-3">
            <span className="text-[13.5px] font-medium">{toggle.label}</span>
            {toggle.checked && toggle.recommended && (
              <Badge tone="good" className="shrink-0">
                {toggle.recommended}
              </Badge>
            )}
            <span className="ml-auto">
              <Toggle checked={toggle.checked} onChange={toggle.onChange} />
            </span>
          </div>
          <p className="mt-2 max-w-[78ch] text-[13px] leading-relaxed text-[var(--color-ink-soft)]">{toggle.body}</p>
          {toggle.why && (
            <p className="mt-1.5 max-w-[78ch] text-[12.5px] leading-relaxed text-[var(--color-ink-faint)]">
              {toggle.why}
            </p>
          )}
        </div>
      )}

      {extra}

      {advanced && (
        <Advanced label={t.app.advanced}>
          {advanced}
        </Advanced>
      )}
    </Card>
  )
}

/** 环境 RNA 的决定在运行过程中问过一次（第 7 步之后），这里只显示这次的结果。 */
function CorrectionState({ report }: { report: ReportData }) {
  const { t } = useT()
  const u = report.ui
  return (
    <div className="mt-5 flex flex-wrap items-center gap-3 rounded-xl bg-[var(--color-canvas)] px-4 py-3.5">
      <span className="text-[13.5px] font-medium">{t.ambientScreen.decision}</span>
      <Badge tone={u.correction_applied ? 'good' : 'neutral'}>
        {u.correction_applied ? t.ambientScreen.applied : t.ambientScreen.notApplied}
      </Badge>
      <span className="text-[13px] text-[var(--color-ink-soft)]">{correctionStateLabel(t, u.ambient_correction_state)}</span>
    </div>
  )
}

/**
 * 从报告反推那一次运行的设置。
 * 只用于"界面没记住这次运行"的兜底（例如以后要做的'打开历史结果'）；
 * 正常流程里 App 直接把运行时用的 `RunSettings` 传进来。
 */
export function settingsFromReport(report: ReportData): RunSettings {
  const u = report.ui
  return {
    ...DEFAULT_SETTINGS,
    seed: u.seed,
    cellCalling: { ...DEFAULT_SETTINGS.cellCalling, fdr: u.cc_fdr, lower: u.cc_lower },
    ambient: { ...DEFAULT_SETTINGS.ambient },
    ddqc: {
      madMultiplier: u.ddqc_mad_multiplier,
      fixedThresholdsAreHard: u.ddqc_strict,
      fixedMinGenes: u.ddqc_fixed_min_genes,
      fixedMaxMito: u.ddqc_fixed_max_mito,
    },
    doublet: {
      ratePer1000: u.dbl_rate_per_1000,
      nLoaded: u.dbl_n_loaded,
      removeBorderline: u.dbl_remove_borderline,
    },
    correction: { method: u.correction_applied ?? 'none', force: u.correction_forced },
    mitoGuardrail: u.mito_guardrail,
  }
}

function ambientRange(a: number | null, b: number | null): string {
  const vals = [a, b].filter((v): v is number => v != null && Number.isFinite(v))
  if (vals.length === 0) return '—'
  const lo = Math.min(...vals)
  const hi = Math.max(...vals)
  return lo === hi ? pct(lo) : `${pct(lo)}–${pct(hi)}`
}
