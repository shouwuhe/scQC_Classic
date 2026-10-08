import { useMemo } from 'react'
import {
  CheckCircle2,
  CircleAlert,
  Droplets,
  Users,
  TriangleAlert,
  Sparkles,
  Info,
} from 'lucide-react'
import { Page, ActionBar } from '@/components/AppShell'
import { Advanced, Badge, Button, Card, Explain, Note } from '@/components/ui/primitives'
import { Plot, ACCENT, MUTED, GOOD } from '@/components/Plot'
import { KneePlot, DistributionPlots, usePlots } from '@/components/Charts'
import { useT, fill, type Dict } from '@/i18n'
import { num, pct } from '@/lib/format'
import type { AmbientFacts, ReportData, SampleJob } from '@/lib/types'
import { cn } from '@/lib/utils'

/**
 * 第 3 屏：结果总览。
 *
 * 只读 `report.ui`（语言中立事实块），文案一律来自词典 —— 报告里的中文串只服务 HTML 报告。
 * 顶部一句结论 + 四张卡片（真细胞 / 环境 RNA / 低质量 / 双细胞），
 * 每张卡片的细节折在"看看这是怎么判定的"后面：新手看四个数就够了。
 */
export function OverviewScreen({
  report,
  outDir,
  jobs,
  activeId,
  onSelect,
  onTune,
  onNext,
}: {
  report: ReportData
  outDir: string | null
  jobs: SampleJob[]
  activeId: string
  onSelect: (id: string) => void
  onTune: () => void
  /** 下一屏是"环境 RNA"（SPEC §11.3），不是输出页 */
  onNext: () => void
}) {
  const { t, lang } = useT()
  const u = report.ui
  const plots = usePlots(outDir)
  const total = u.n_cellranger_cells
  const kept = u.n_final
  const keptRatio = total > 0 ? kept / total : 1
  const disagree = u.ambient_verdict === 'disagree'
  const verdict = u.ambient_rating === 'severe' ? 'problem' : disagree || keptRatio < 0.7 ? 'check' : 'good'

  const waterfall = useMemo(
    () => ({
      labels: u.waterfall.map((w) => stepLabel(t, w.id)),
      values: u.waterfall.map((w) => w.n),
    }),
    [u.waterfall, t],
  )

  const rhoText = ambientHeadline(u)

  const ran = jobs.filter((j) => j.status === 'done' || j.status === 'error')
  return (
    <Page wide title={t.overview.title}>
      {ran.length > 1 && (
        <SampleSwitcher jobs={ran} activeId={activeId} onSelect={onSelect} />
      )}

      {/* 结论条 */}
      <Card
        className={cn(
          'flex items-center gap-4 px-6 py-5',
          verdict === 'good' && 'bg-[var(--color-good-soft)] border-transparent',
          verdict === 'check' && 'bg-[var(--color-warn-soft)] border-transparent',
          verdict === 'problem' && 'bg-[var(--color-bad-soft)] border-transparent',
        )}
      >
        {verdict === 'good' ? (
          <CheckCircle2 className="h-7 w-7 shrink-0 text-[var(--color-good)]" />
        ) : (
          <CircleAlert
            className={cn(
              'h-7 w-7 shrink-0',
              verdict === 'check' ? 'text-[var(--color-warn)]' : 'text-[var(--color-bad)]',
            )}
          />
        )}
        <div className="min-w-0">
          <div className="text-[18px] font-semibold">
            {verdict === 'good'
              ? t.overview.verdictGood
              : verdict === 'check'
                ? t.overview.verdictCheck
                : t.overview.verdictProblem}
          </div>
          <div className="text-[13.5px] text-[var(--color-ink-soft)]">
            {fill(t.overview.keptOf, { kept: num(kept, lang), total: num(total, lang) })} ·{' '}
            {platformLabel(t, u.platform_id)} · {speciesLabel(t, u.species_id)}
            {u.multiplex_n ? ` · ${t.import.multiplexN} ${u.multiplex_n}` : ''}
          </div>
        </div>
        <div className="ml-auto hidden shrink-0 items-baseline gap-1 sm:flex">
          <span className="tnum text-[32px] font-semibold leading-none">{Math.round(keptRatio * 100)}</span>
          <span className="text-[15px] text-[var(--color-ink-soft)]">%</span>
        </div>
      </Card>

      {/* 四张卡片 */}
      <div className="mt-5 grid gap-4 md:grid-cols-2">
        <MetricCard
          icon={<Users className="h-4 w-4" />}
          title={t.overview.cards.cells}
          sub={t.overview.cards.cellsSub}
          value={num(u.cc_n_called, lang)}
          foot={
            u.cc_n_rescued > 0
              ? `+${num(u.cc_n_rescued, lang)} · ${u.cc_rescued_top_genes.slice(0, 4).join(' / ')}`
              : ''
          }
          details={
            <Rows
              rows={[
                [t.import.summaryCells, num(u.n_cellranger_cells, lang)],
                [t.overview.rescued, num(u.cc_n_rescued, lang)],
                [t.overview.lost, num(u.cc_n_lost, lang)],
                [t.overview.knee, num(u.cc_knee, lang)],
                [t.overview.inflection, num(u.cc_inflection, lang)],
              ]}
            />
          }
        />
        <MetricCard
          icon={<Droplets className="h-4 w-4" />}
          title={t.overview.cards.ambient}
          sub={t.overview.cards.ambientSub}
          value={rhoText}
          tone={disagree ? 'warn' : 'neutral'}
          badge={
            u.ambient_available ? (
              <Badge tone={disagree ? 'warn' : 'good'}>
                {disagree ? t.overview.ambientDisagree : t.overview.ambientAgree}
              </Badge>
            ) : undefined
          }
          foot={
            u.ambient_available
              ? [
                  u.rho_soup != null ? `A ${pct(u.rho_soup)}` : null,
                  u.rho_decontx != null ? `B ${pct(u.rho_decontx)}` : null,
                  u.rho_mito != null ? `C ${pct(u.rho_mito)}` : null,
                ]
                  .filter(Boolean)
                  .join('   ·   ')
              : t.warn.AMBIENT_SKIPPED
          }
          details={
            <div className="space-y-3">
              <Rows
                rows={[
                  [t.overview.ambientMethodA, u.rho_soup != null ? pct(u.rho_soup) : t.app.notAvailable],
                  [t.overview.ambientMethodB, u.rho_decontx != null ? pct(u.rho_decontx) : t.app.notAvailable],
                  [t.overview.ambientMethodC, u.rho_mito != null ? pct(u.rho_mito) : t.app.notAvailable],
                  [t.overview.ambientInterval, rhoText],
                  [
                    t.overview.ambientReferenceMean,
                    u.ambient_reference_mean != null ? pct(u.ambient_reference_mean) : t.app.none,
                  ],
                  [t.overview.ambientLevel, ratingLabel(t, u.ambient_rating)],
                  [
                    t.ambientScreen.decision,
                    u.correction_applied ? t.ambientScreen.applied : t.ambientScreen.notApplied,
                  ],
                ]}
              />
              {u.rho_soup_unreliable && <Note tone="warn">{t.overview.ambientUnreliable}</Note>}
              {disagree && <Note tone="warn">{t.overview.ambientDisagreeBody}</Note>}
              {u.correction_applied && (
                <Rows
                  rows={[
                    [
                      t.ambientScreen.usedMethod,
                      u.correction_applied === 'decontx'
                        ? t.ambientScreen.methodDecontx
                        : t.ambientScreen.methodSoup,
                    ],
                    [t.ambientScreen.removedTotal, pct(u.correction_removed_fraction)],
                    ...(u.correction_top_genes ?? []).map(
                      (g) =>
                        [`${g.gene} ${t.ambientScreen.colRemoved}`, pct(g.removed_fraction, 0)] as [string, string],
                    ),
                  ]}
                />
              )}
              {u.ambient_top_genes.length > 0 && (
                <div>
                  <div className="mb-1.5 text-[12.5px] font-medium text-[var(--color-ink-soft)]">
                    {t.overview.ambientTopGenes}
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {u.ambient_top_genes.slice(0, 10).map((g) => (
                      <Badge key={g.gene} tone="neutral" className="font-mono">
                        {g.gene} <span className="tnum opacity-70">{pct(g.fraction, 0)}</span>
                      </Badge>
                    ))}
                  </div>
                </div>
              )}
            </div>
          }
        />
        <MetricCard
          icon={<Sparkles className="h-4 w-4" />}
          title={t.overview.cards.quality}
          sub={t.overview.cards.qualitySub}
          value={num(u.ddqc_n_removed_total, lang)}
          foot={u.ddqc_removed_by_dim
            .filter((d) => d.n > 0)
            .map((d) => `${dimLabel(t, d.id)} ${num(d.n, lang)}`)
            .join('   ·   ')}
          details={
            <div className="space-y-3">
              <Rows
                rows={[
                  [t.overview.removedTotal, `${num(u.ddqc_n_removed_total, lang)} / ${num(u.ddqc_n_in, lang)}`],
                  ...u.ddqc_removed_by_dim.map(
                    (d) => [dimLabel(t, d.id), num(d.n, lang)] as [string, string],
                  ),
                  [t.tuning.quality.strict, u.ddqc_strict ? t.app.on : t.app.off],
                  [t.overview.nClusters, num(u.ddqc_n_clusters, lang)],
                ]}
              />
            </div>
          }
        />
        <MetricCard
          icon={<TriangleAlert className="h-4 w-4" />}
          title={t.overview.cards.doublet}
          sub={t.overview.cards.doubletSub}
          value={num(u.dbl_n_removed, lang)}
          foot={`${t.overview.doubletHighConfidence} ${num(u.dbl_n_high_confidence, lang)} · ${t.overview.doubletBorderline} ${num(u.dbl_n_borderline, lang)}`}
          details={
            <Rows
              rows={[
                [t.overview.doubletHighConfidence, num(u.dbl_n_high_confidence, lang)],
                [t.overview.doubletBorderline, num(u.dbl_n_borderline, lang)],
                [t.overview.doubletJaccard, u.dbl_jaccard.toFixed(3)],
                [t.tuning.doublet.nLoaded, num(u.dbl_n_loaded, lang)],
                [t.tuning.doublet.removeBorderline, u.dbl_remove_borderline ? t.app.on : t.app.off],
              ]}
            />
          }
        />
      </div>

      {/* 逐细胞指标 */}
      <Card className="mt-5 px-6 py-5">
        <div className="flex items-center gap-2">
          <h2 className="text-[15px] font-semibold">{t.overview.qcMetrics}</h2>
          <span className="text-[12.5px] text-[var(--color-ink-faint)]">{t.overview.afterQc}</span>
        </div>
        <div className="mt-4 grid gap-x-8 gap-y-4 sm:grid-cols-2 lg:grid-cols-4">
          <Stat label={t.overview.medianUmi} value={num(Math.round(u.median_umi), lang)} />
          <Stat label={t.overview.medianGenes} value={num(Math.round(u.median_genes), lang)} />
          <Stat
            label={t.overview.medianMito}
            value={pct(u.median_mito)}
            explain={t.facts.mitoSemantics[u.mito_semantics]}
          />
          <Stat
            label={t.overview.medianRibo}
            value={u.median_ribo != null ? pct(u.median_ribo) : t.app.notAvailable}
          />
        </div>
      </Card>

      {/* 图表：barcode rank 曲线与各项分布 */}
      {plots?.knee && (
        <div className="mt-5">
          <KneePlot data={plots} u={u} />
        </div>
      )}
      {plots?.qc && (
        <div className="mt-5">
          <DistributionPlots data={plots} u={u} />
        </div>
      )}

      {/* 瀑布 */}
      <Card className="mt-5 p-6">
        <h2 className="text-[15px] font-semibold">{t.overview.waterfall}</h2>
        <Plot
          height={Math.max(180, waterfall.labels.length * 44)}
          data={[
            {
              type: 'bar',
              orientation: 'h',
              x: waterfall.values,
              y: waterfall.labels,
              marker: {
                color: waterfall.values.map((_, i) =>
                  i === waterfall.values.length - 1 ? GOOD : i === 0 ? MUTED : ACCENT,
                ),
              },
              text: waterfall.values.map((v) => num(v, lang)),
              textposition: 'auto',
              hovertemplate: '%{y}: %{x:,}<extra></extra>',
            },
          ]}
          layout={{
            xaxis: { type: 'log' },
            yaxis: { autorange: 'reversed' },
            margin: { l: 210, r: 24, t: 8, b: 28 },
          }}
        />
        {u.removed_by_step.length > 0 && (
          <div className="mt-2 flex flex-wrap gap-x-6 gap-y-1 text-[13px] text-[var(--color-ink-soft)]">
            <span className="text-[var(--color-ink-faint)]">{t.overview.cellsRemovedBy}</span>
            {u.removed_by_step.map((r) => (
              <span key={r.id}>
                {removedByLabel(t, r.id)}: <span className="tnum font-medium">{num(r.n, lang)}</span>
              </span>
            ))}
          </div>
        )}
        {/* 这条链里没有环境 RNA，容易被当成"漏了一步"——直接说清楚 */}
      </Card>

      {u.warning_codes.length > 0 && (
        <Card className="mt-5 p-6">
          <h2 className="text-[15px] font-semibold">{t.overview.warnings}</h2>
          <ul className="mt-3 space-y-2.5">
            {u.warning_codes.map((code, i) => (
              <li key={`${code}-${i}`} className="flex gap-3 text-[13.5px]">
                <Info className="mt-0.5 h-4 w-4 shrink-0 text-[var(--color-warn)]" />
                <span className="text-[var(--color-ink-soft)]">
                  {/* 只用界面词典：引擎写的那句是中文，绝不能当兜底（SPEC §10.5） */}
                  {warnText(t, code) ?? code}
                </span>
              </li>
            ))}
          </ul>
        </Card>
      )}

      <ActionBar>
        <Button data-smoke="tune" variant="secondary" onClick={onTune}>
          {t.tuning.title}
        </Button>
        <Button data-smoke="next" size="lg" onClick={onNext}>
          {t.app.next}
        </Button>
      </ActionBar>
    </Page>
  )
}

/* ---------------------------------------------------------------- */

/** 批量时的样本切换条：失败的样本也列出来，不能让它悄悄消失。 */
function SampleSwitcher({
  jobs,
  activeId,
  onSelect,
}: {
  jobs: SampleJob[]
  activeId: string
  onSelect: (id: string) => void
}) {
  const { t, lang } = useT()
  const done = jobs.filter((j) => j.status === 'done').length
  return (
    <div className="mb-5">
      <div className="flex items-baseline gap-3">
        <h2 className="text-[13px] font-medium text-[var(--color-ink-soft)]">{t.overview.batchTitle}</h2>
        <span className="text-[12.5px] text-[var(--color-ink-faint)]">
          {fill(t.overview.batchSubtitle, { done: num(done, lang), total: num(jobs.length, lang) })}
        </span>
      </div>
      <div className="mt-2 flex flex-wrap gap-2">
        {jobs.map((j) => (
          <button
            key={j.id}
            disabled={j.status !== 'done'}
            onClick={() => onSelect(j.id)}
            className={cn(
              'rounded-full border px-3 py-1.5 text-[12.5px] transition',
              j.id === activeId
                ? 'border-transparent bg-[var(--color-accent)] text-white'
                : 'border-[var(--color-line)] bg-[var(--color-surface)] hover:bg-[var(--color-canvas)]',
              j.status === 'error' && 'border-transparent bg-[var(--color-bad-soft)] text-[var(--color-bad)]',
              j.status !== 'done' && 'cursor-default',
            )}
          >
            {j.name}
            {j.status === 'error' && <span className="ml-1.5 opacity-80">· {t.overview.batchFailed}</span>}
          </button>
        ))}
      </div>
    </div>
  )
}

function MetricCard({
  icon,
  title,
  sub,
  value,
  foot,
  badge,
  tone = 'neutral',
  details,
}: {
  icon: React.ReactNode
  title: string
  sub: string
  value: string
  foot?: string
  badge?: React.ReactNode
  tone?: 'neutral' | 'warn'
  details: React.ReactNode
}) {
  const { t } = useT()
  return (
    <Card className="p-6">
      <div className="flex items-center gap-2 text-[var(--color-ink-soft)]">
        <span className={cn(tone === 'warn' ? 'text-[var(--color-warn)]' : 'text-[var(--color-accent)]')}>{icon}</span>
        <h3 className="text-[14px] font-semibold text-[var(--color-ink)]">{title}</h3>
        <Explain>{sub}</Explain>
        {badge && <span className="ml-auto">{badge}</span>}
      </div>
      <div className="tnum mt-3 text-[30px] font-semibold leading-none">{value}</div>
      {foot && <p className="mt-2 line-clamp-2 text-[12.5px] text-[var(--color-ink-faint)]">{foot}</p>}
      <Advanced label={t.overview.openDetails}>{details}</Advanced>
    </Card>
  )
}

function Rows({ rows }: { rows: [string, string][] }) {
  return (
    <dl className="divide-y divide-[var(--color-line)]">
      {rows.map(([k, v], i) => (
        <div key={`${k}-${i}`} className="flex items-baseline justify-between gap-6 py-1.5 text-[13px]">
          <dt className="text-[var(--color-ink-soft)]">{k}</dt>
          <dd className="tnum shrink-0 font-medium">{v}</dd>
        </div>
      ))}
    </dl>
  )
}

function Stat({ label, value, explain }: { label: string; value: string; explain?: string }) {
  return (
    <div>
      <div className="flex items-center gap-1 text-[12.5px] text-[var(--color-ink-faint)]">
        {label}
        {explain && <Explain>{explain}</Explain>}
      </div>
      <div className="tnum mt-1 text-[20px] font-semibold">{value}</div>
    </div>
  )
}

/* ---- 稳定 id → 本地化文案（引擎的中文串一律不参与界面渲染）---- */

export function platformLabel(t: Dict, id: string): string {
  return (t.facts.platform as Record<string, string>)[id] ?? id
}
export function speciesLabel(t: Dict, id: string): string {
  return (t.facts.species as Record<string, string>)[id] ?? id
}
export function ccModeLabel(t: Dict, id: string): string {
  return (t.facts.ccMode as Record<string, string>)[id] ?? id
}
export function dimLabel(t: Dict, id: string): string {
  return (t.facts.dim as Record<string, string>)[id] ?? id
}
export function stepLabel(t: Dict, id: string): string {
  return (t.facts.step as Record<string, string>)[id] ?? id
}
export function removedByLabel(t: Dict, id: string): string {
  return (t.facts.removedBy as Record<string, string>)[id] ?? id
}
export function correctionStateLabel(t: Dict, id: string): string {
  return (t.facts.correctionState as Record<string, string>)[id] ?? id
}
export function ratingLabel(t: Dict, id: string): string {
  return (t.facts.rating as Record<string, string>)[id] ?? id
}
export function warnText(t: Dict, code: string): string | undefined {
  return (t.warn as Record<string, string>)[code]
}

/** 引擎错误 code → 界面文案。认不出来就给一句通用的，绝不显示引擎原文。 */
export function errText(t: Dict, code: string | null): string {
  if (!code) return t.errors.runFailed
  return (t.errByCode as Record<string, string>)[code] ?? t.errors.runFailedBody
}

/** 主展示值：一致时给区间，分歧时给两个数。 */
export function ambientHeadline(u: AmbientFacts): string {
  if (!u.ambient_available) return '—'
  const vals = [u.rho_soup, u.rho_decontx].filter((v): v is number => v != null && Number.isFinite(v))
  if (vals.length === 0) return '—'
  if (vals.length === 1) return pct(vals[0]!)
  const lo = Math.min(...vals)
  const hi = Math.max(...vals)
  return lo === hi ? pct(lo) : `${pct(lo)}–${pct(hi)}`
}
