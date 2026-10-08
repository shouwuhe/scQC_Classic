import { useEffect, useMemo, useState } from 'react'
import { Plot, ACCENT, MUTED, GOOD, WARN, BAD } from '@/components/Plot'
import { Card } from '@/components/ui/primitives'
import { useT } from '@/i18n'
import { num } from '@/lib/format'
import { host, type PlotBundle } from '@/lib/api'
import type { UiFacts } from '@/lib/types'

/**
 * 结果页的图表。
 *
 * 数据来自引擎写出的 `plots/*.bin`（原始 f32，SPEC §10.3：大数组不走 JSON），
 * 在这里构造 Float32Array 直接喂给 Plotly。报告里那几张静态 SVG 保持不变——
 * 界面要的是能悬停、能缩放的版本。
 */

export interface PlotData {
  knee?: { rank: Float32Array; total: Float32Array }
  qc?: {
    n: number
    totalUmi: Float32Array
    nGenes: Float32Array
    pctMito: Float32Array
    pctRibo: Float32Array
    isCell: Float32Array
    passed: Float32Array
  }
}

/** 读取并解包 `plots/`；任何一步失败都安静地返回空对象（图表区自动消失）。 */
export function usePlots(outDir: string | null): PlotData | null {
  const [data, setData] = useState<PlotData | null>(null)
  useEffect(() => {
    let alive = true
    if (!outDir) {
      setData(null)
      return
    }
    void host()
      .readPlots(outDir, ['knee_curve', 'qc_metrics'])
      .then((b: PlotBundle) => {
        if (!alive || !b.ok) return setData({})
        setData(unpack(b))
      })
      .catch(() => alive && setData({}))
    return () => {
      alive = false
    }
  }, [outDir])
  return data
}

function unpack(b: PlotBundle): PlotData {
  const out: PlotData = {}
  const col = (name: string, i: number, k: number, rows: number) => {
    const raw = b.buffers[name]
    if (!raw) return null
    const f = new Float32Array(raw.buffer, raw.byteOffset, Math.floor(raw.byteLength / 4))
    const v = new Float32Array(rows)
    for (let r = 0; r < rows; r++) v[r] = f[r * k + i]!
    return v
  }
  const knee = b.entries.find((e) => e.name === 'knee_curve')
  if (knee) {
    const rows = knee.shape[0] ?? 0
    const rank = col('knee_curve', 0, 2, rows)
    const total = col('knee_curve', 1, 2, rows)
    if (rank && total) out.knee = { rank, total }
  }
  const qc = b.entries.find((e) => e.name === 'qc_metrics')
  if (qc) {
    const rows = qc.shape[0] ?? 0
    const k = qc.shape[1] ?? 6
    const g = (i: number) => col('qc_metrics', i, k, rows)
    const [a, c, d, e, f, h] = [g(0), g(1), g(2), g(3), g(4), g(5)]
    if (a && c && d && e && f && h)
      out.qc = { n: rows, totalUmi: a, nGenes: c, pctMito: d, pctRibo: e, isCell: f, passed: h }
  }
  return out
}

/* ------------------------------------------------------------------ */

/** Barcode rank 曲线，标注 knee / inflection / 判定细胞数。 */
export function KneePlot({ data, u }: { data: PlotData; u: UiFacts }) {
  const { t, lang } = useT()
  const k = data.knee
  const traces = useMemo(() => {
    if (!k) return []
    const x = Array.from(k.rank)
    const y = Array.from(k.total)
    const cut = u.cc_n_called
    const cellX: number[] = []
    const cellY: number[] = []
    const emptyX: number[] = []
    const emptyY: number[] = []
    for (let i = 0; i < x.length; i++) {
      if (x[i]! <= cut) {
        cellX.push(x[i]!)
        cellY.push(y[i]!)
      } else {
        emptyX.push(x[i]!)
        emptyY.push(y[i]!)
      }
    }
    // 让两段视觉上连起来
    if (cellX.length && emptyX.length) {
      emptyX.unshift(cellX[cellX.length - 1]!)
      emptyY.unshift(cellY[cellY.length - 1]!)
    }
    // 这条曲线用 SVG 画，不用 WebGL：引擎已经把同 total 的 barcode 折叠过，
    // 点数是几千不是几十万；而 WebGL 在虚拟机、远程桌面、老显卡上会整块画不出来
    // （Plotly 会在图位置上显示一行 "WebGL is not supported"），
    // 为了几千个点冒这个险不值得。真遇到超长曲线就等距抽稀，log–log 下看不出区别。
    const thin = (a: number[], b: number[]) => {
      const MAX = 4000
      if (a.length <= MAX) return [a, b] as const
      const step = Math.ceil(a.length / MAX)
      const xa: number[] = []
      const yb: number[] = []
      for (let i = 0; i < a.length; i += step) {
        xa.push(a[i]!)
        yb.push(b[i]!)
      }
      // 末点必须保留，否则曲线尾巴会被截掉
      if (xa[xa.length - 1] !== a[a.length - 1]) {
        xa.push(a[a.length - 1]!)
        yb.push(b[b.length - 1]!)
      }
      return [xa, yb] as const
    }
    const [cx, cy] = thin(cellX, cellY)
    const [ex, ey] = thin(emptyX, emptyY)
    return [
      {
        type: 'scatter' as const,
        mode: 'lines' as const,
        x: cx,
        y: cy,
        line: { color: ACCENT, width: 2.5 },
        name: t.overview.cards.cells,
        hovertemplate: `${t.overview.barcodeRankAxis} %{x:,}<br>${t.overview.umiAxis} %{y:,.0f}<extra></extra>`,
      },
      {
        type: 'scatter' as const,
        mode: 'lines' as const,
        x: ex,
        y: ey,
        line: { color: MUTED, width: 2 },
        name: t.facts.removedBy.cell_calling,
        hovertemplate: `${t.overview.barcodeRankAxis} %{x:,}<br>${t.overview.umiAxis} %{y:,.0f}<extra></extra>`,
      },
    ]
  }, [k, u.cc_n_called, t])

  if (!k) return null
  const marks = [
    { y: u.cc_knee, label: t.overview.knee, color: GOOD },
    { y: u.cc_inflection, label: t.overview.inflection, color: WARN },
  ].filter((m) => m.y > 0)

  return (
    <Card className="p-6">
      <h2 className="text-[15px] font-semibold">{t.overview.kneePlot}</h2>
      <p className="mt-1 max-w-[78ch] text-[12.5px] text-[var(--color-ink-faint)]">{t.overview.kneePlotSub}</p>
      <Plot
        height={320}
        data={traces}
        layout={{
          // dtick: 1 = 只标十进制刻度；否则 Plotly 会在 1 和 10 之间再标 2、5，五个数量级下满屏碎数字
          xaxis: { type: 'log', dtick: 1, title: { text: t.overview.barcodeRankAxis, standoff: 8 } },
          yaxis: { type: 'log', dtick: 1, title: { text: t.overview.umiAxis, standoff: 8 } },
          showlegend: true,
          legend: { orientation: 'h', y: 1.12, x: 0, font: { size: 11 } },
          margin: { l: 62, r: 16, t: 28, b: 44 },
          shapes: [
            ...marks.map((m) => ({
              type: 'line' as const,
              xref: 'paper' as const,
              x0: 0,
              x1: 1,
              y0: m.y,
              y1: m.y,
              line: { color: m.color, width: 1, dash: 'dot' as const },
            })),
            {
              type: 'line' as const,
              yref: 'paper' as const,
              y0: 0,
              y1: 1,
              x0: u.cc_n_called,
              x1: u.cc_n_called,
              line: { color: ACCENT, width: 1, dash: 'dash' as const },
            },
          ],
          annotations: [
            ...marks.map((m) => ({
              xref: 'paper' as const,
              x: 1,
              y: Math.log10(Math.max(1, m.y)),
              xanchor: 'right' as const,
              yanchor: 'bottom' as const,
              text: `${m.label} ${num(m.y, lang)}`,
              showarrow: false,
              font: { size: 10, color: m.color },
            })),
            {
              yref: 'paper' as const,
              y: 1,
              x: Math.log10(Math.max(1, u.cc_n_called)),
              yanchor: 'top' as const,
              xanchor: 'left' as const,
              text: ` ${num(u.cc_n_called, lang)} ${t.app.cells}`,
              showarrow: false,
              font: { size: 10, color: ACCENT },
            },
          ],
        }}
      />
    </Card>
  )
}

/** 四张分布图：质控前（全部判定细胞）与质控后（最终保留）叠放。 */
export function DistributionPlots({ data, u }: { data: PlotData; u: UiFacts }) {
  const { t } = useT()
  const qc = data.qc
  const panels = useMemo(() => {
    if (!qc) return []
    const keep = (i: number) => qc.isCell[i]! > 0.5
    const pick = (src: Float32Array, passedOnly: boolean) => {
      const out: number[] = []
      for (let i = 0; i < qc.n; i++) {
        if (!keep(i)) continue
        if (passedOnly && qc.passed[i]! < 0.5) continue
        const v = src[i]!
        if (Number.isFinite(v)) out.push(v)
      }
      return out
    }
    return [
      { key: 'umi', title: t.overview.umiDist, src: qc.totalUmi, log: true, pct: false, marks: [] as number[] },
      {
        key: 'genes',
        title: t.overview.genesDist,
        src: qc.nGenes,
        log: true,
        pct: false,
        marks: u.ddqc_strict ? [u.ddqc_fixed_min_genes] : [],
      },
      {
        key: 'mito',
        title: t.overview.mitoDist,
        src: qc.pctMito,
        log: false,
        pct: true,
        marks: u.ddqc_strict ? [u.ddqc_fixed_max_mito] : [],
      },
      ...(u.median_ribo != null
        ? [{ key: 'ribo', title: t.overview.riboDist, src: qc.pctRibo, log: false, pct: true, marks: [] as number[] }]
        : []),
    ].map((p) => {
      const before = pick(p.src, false)
      const after = pick(p.src, true)
      // 前后两条直方图必须用**同一套分箱**，否则叠在一起看是误导性的
      const xs = (p.log ? before.map((v) => Math.log10(Math.max(1, v))) : before).filter(Number.isFinite)
      const lo = xs.length ? Math.min(...xs) : 0
      const hi = xs.length ? Math.max(...xs) : 1
      const size = Math.max((hi - lo) / 60, 1e-9)
      return { ...p, before, after, bins: { start: lo, end: hi + size, size } }
    })
  }, [qc, u, t])

  if (!qc) return null

  return (
    <Card className="p-6">
      <div className="flex items-baseline gap-3">
        <h2 className="text-[15px] font-semibold">{t.overview.distribution}</h2>
        <span className="flex items-center gap-3 text-[12px] text-[var(--color-ink-faint)]">
          <span className="flex items-center gap-1.5">
            <span className="inline-block h-2 w-3 rounded-sm" style={{ background: MUTED }} />
            {t.overview.beforeQc}
          </span>
          <span className="flex items-center gap-1.5">
            <span className="inline-block h-2 w-3 rounded-sm" style={{ background: ACCENT }} />
            {t.overview.afterQcLegend}
          </span>
        </span>
      </div>
      <div className="mt-3 grid gap-5 md:grid-cols-2">
        {panels.map((p) => (
          <div key={p.key}>
            <div className="text-[12.5px] text-[var(--color-ink-soft)]">{p.title}</div>
            <Plot
              height={190}
              data={[
                {
                  type: 'histogram',
                  x: p.log ? p.before.map((v) => Math.log10(Math.max(1, v))) : p.before,
                  marker: { color: MUTED },
                  opacity: 0.85,
                  autobinx: false,
                  xbins: p.bins,
                  hoverinfo: 'skip',
                },
                {
                  type: 'histogram',
                  x: p.log ? p.after.map((v) => Math.log10(Math.max(1, v))) : p.after,
                  marker: { color: ACCENT },
                  opacity: 0.8,
                  autobinx: false,
                  xbins: p.bins,
                  hoverinfo: 'skip',
                },
              ]}
              layout={{
                barmode: 'overlay',
                margin: { l: 46, r: 10, t: 6, b: 34 },
                xaxis: p.log
                  ? {
                      tickmode: 'array',
                      tickvals: [1, 2, 3, 4, 5],
                      ticktext: ['10', '100', '1k', '10k', '100k'],
                    }
                  : { tickformat: p.pct ? '.0%' : undefined },
                yaxis: { title: { text: t.overview.cellsAxis, standoff: 6 } },
                shapes: p.marks
                  .filter((v) => v > 0)
                  .map((v) => ({
                    type: 'line' as const,
                    yref: 'paper' as const,
                    y0: 0,
                    y1: 1,
                    x0: p.log ? Math.log10(v) : v,
                    x1: p.log ? Math.log10(v) : v,
                    line: { color: BAD, width: 1, dash: 'dash' as const },
                  })),
              }}
            />
          </div>
        ))}
      </div>
      {u.median_ribo == null && (
        <p className="mt-2 text-[12px] text-[var(--color-ink-faint)]">{t.overview.riboHidden}</p>
      )}
    </Card>
  )
}
