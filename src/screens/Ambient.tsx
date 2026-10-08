import { useEffect, useState } from 'react'
import { TriangleAlert } from 'lucide-react'
import { Button, Card, Note, Toggle } from '@/components/ui/primitives'
import { useT, type Dict } from '@/i18n'
import { pct } from '@/lib/format'
import type { AmbientFacts } from '@/lib/types'

type Method = 'soup' | 'decontx'

/**
 * 环境 RNA 决策用到的零件。没有独立界面：判定与选择都发生在运行页
 * （第 1–7 步之后引擎停下来问一次），结果在查看页呈现。
 */

/* ---------------------------------------------------------------- */

/**
 * 参考区间对比条（SPEC §11.2）。三段底色 = 三种推荐程度，刻度上是本样本的两三个估计值。
 * 没有参考基准的平台（Flex）只画一条素轨道 —— 不画不存在的区间。
 */
export function ReferenceBar({ u }: { u: AmbientFacts }) {
  const { t } = useT()
  const vals: [string, number][] = [
    ['A', u.rho_soup],
    ['B', u.rho_decontx],
    ['C', u.rho_mito],
  ].filter((e): e is [string, number] => typeof e[1] === 'number' && Number.isFinite(e[1])) as [string, number][]
  const lo = u.ambient_reference_grey_below
  const hi = u.ambient_reference_discourage_above
  const top = Math.max(0.25, ...vals.map((v) => v[1]), hi ?? 0, u.ambient_reference_mean ?? 0) * 1.2
  const max = Math.min(1, Math.ceil(top * 20) / 20)
  const x = (v: number) => `${Math.min(100, (v / max) * 100)}%`

  return (
    <div className="mt-4">
      <div className="relative h-9">
        <div className="absolute inset-x-0 top-0 h-3 overflow-hidden rounded-full bg-[var(--color-line)]">
          {lo != null && hi != null && (
            <>
              <div className="absolute inset-y-0 left-0 bg-[var(--color-line)]" style={{ width: x(lo) }} />
              <div
                className="absolute inset-y-0 bg-[var(--color-accent-soft)]"
                style={{ left: x(lo), width: `calc(${x(hi)} - ${x(lo)})` }}
              />
              <div className="absolute inset-y-0 right-0 bg-[var(--color-warn-soft)]" style={{ left: x(hi) }} />
            </>
          )}
        </div>
        {u.ambient_reference_mean != null && (
          <div
            className="absolute top-0 h-3 w-px bg-[var(--color-ink-faint)]"
            style={{ left: x(u.ambient_reference_mean) }}
            title={t.ambientScreen.typical}
          />
        )}
        {vals.map(([k, v]) => (
          <div key={k} className="absolute top-0" style={{ left: x(v) }}>
            <div className="h-3 w-[2px] -translate-x-1/2 bg-[var(--color-accent)]" />
            <div className="tnum -translate-x-1/2 whitespace-nowrap pt-1 text-[11px] text-[var(--color-accent)]">
              {k} {pct(v, 0)}
            </div>
          </div>
        ))}
      </div>
      <div className="flex justify-between text-[11px] text-[var(--color-ink-faint)]">
        <span>0%</span>
        {lo != null && <span>{`${t.ambientScreen.lowEnd} · ${pct(lo, 0)}`}</span>}
        {hi != null && <span>{`${t.ambientScreen.highEnd} · ${pct(hi, 0)}`}</span>}
        <span>{pct(max, 0)}</span>
      </div>
    </div>
  )
}

/**
 * 不建议时的确认框。按钮不置灰（v2.5），但要把原因原封不动摆一遍，
 * 并且必须显式勾选才放行 —— 勾选就是引擎的 force 标志。
 */
export function ConfirmDialog({ reason, onCancel, onGo }: { reason: string; onCancel: () => void; onGo: () => void }) {
  const { t } = useT()
  const [ack, setAck] = useState(false)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onCancel()
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onCancel])
  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/30 px-6 py-10" onClick={onCancel}>
      <Card className="w-full max-w-[560px] p-6 shadow-xl" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center gap-2">
          <TriangleAlert className="h-5 w-5 text-[var(--color-warn)]" />
          <h2 className="text-[15px] font-semibold">{t.ambientScreen.confirmTitle}</h2>
        </div>
        <p className="mt-3 text-[13.5px] leading-relaxed text-[var(--color-ink-soft)]">{reason}</p>
        <div className="mt-4">
          <Note tone="warn">{t.tuning.ambient.caveat}</Note>
        </div>
        <div className="mt-4 flex items-center gap-3">
          <Toggle checked={ack} onChange={setAck} />
          <span className="text-[13px]">{t.ambientScreen.confirmAck}</span>
        </div>
        <div className="mt-5 flex justify-end gap-2">
          <Button variant="secondary" onClick={onCancel}>
            {t.app.cancel}
          </Button>
          <Button data-smoke="ambient-confirm" disabled={!ack} onClick={onGo}>
            {t.ambientScreen.confirmGo}
          </Button>
        </div>
      </Card>
    </div>
  )
}

/* ---------------------------------------------------------------- */

export function recLabel(t: Dict, s: string): string {
  if (s === 'available') return t.ambientScreen.recAvailable
  if (s === 'unrated') return t.ambientScreen.recUnrated
  if (s === 'discouraged') return t.ambientScreen.recDiscouraged
  return t.ambientScreen.recGreyed
}

export function whyText(t: Dict, s: string): string {
  if (s === 'available') return t.ambientScreen.whyAvailable
  if (s === 'unrated') return t.ambientScreen.whyUnrated
  if (s === 'discouraged') return t.ambientScreen.whyDiscouraged
  return t.ambientScreen.whyGreyed
}

export function recTone(s: string): 'accent' | 'neutral' | 'warn' {
  if (s === 'available') return 'accent'
  if (s === 'unrated') return 'neutral'
  return 'warn'
}

/** 按钮颜色随推荐程度变，但永远可按（SPEC v2.5 §8.3.2）。 */
export function runVariant(s: string, applied: Method | null, method: Method): 'primary' | 'secondary' {
  if (applied != null && applied === method) return 'secondary'
  return s === 'available' ? 'primary' : 'secondary'
}

export function runClass(s: string, applied: Method | null, method: Method): string {
  if (applied != null && applied === method) return ''
  if (s === 'unrated') return 'border-[var(--color-accent)] text-[var(--color-accent)]'
  if (s === 'greyed') return 'text-[var(--color-ink-soft)]'
  if (s === 'discouraged')
    return 'bg-[var(--color-warn-soft)] border-[var(--color-warn)] text-[oklch(0.45_0.12_70)]'
  return ''
}
