import { useEffect, useRef } from 'react'
import Plotly from 'plotly.js-dist-min'

/**
 * Plotly 封装。图表配置统一在这里，保证各页视觉一致。
 *
 * 全部走 SVG trace，不用 WebGL：界面里的图点数都是几千级（引擎已折叠/分箱），
 * 而 WebGL 在虚拟机、远程桌面、老显卡上可能整块渲染不出来，
 * Plotly 会在图的位置显示一行 "WebGL is not supported" —— 对一个"傻瓜式"工具来说这是不可接受的失败方式。
 */
export function Plot({
  data,
  layout,
  height = 260,
  className,
}: {
  data: Plotly.Data[]
  layout?: Partial<Plotly.Layout>
  height?: number
  className?: string
}) {
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const el = ref.current
    if (!el) return
    const base: Partial<Plotly.Layout> = {
      height,
      margin: { l: 52, r: 16, t: 12, b: 40 },
      paper_bgcolor: 'rgba(0,0,0,0)',
      plot_bgcolor: 'rgba(0,0,0,0)',
      font: { size: 11, color: '#6b7280', family: 'inherit' },
      showlegend: false,
      hovermode: 'closest',
      xaxis: { gridcolor: '#eef0f3', zeroline: false, automargin: true },
      yaxis: { gridcolor: '#eef0f3', zeroline: false, automargin: true },
    }
    void Plotly.newPlot(el, data, { ...base, ...layout }, { displayModeBar: false, responsive: true })
    return () => {
      Plotly.purge(el)
    }
  }, [data, layout, height])
  return <div ref={ref} className={className} />
}

export const ACCENT = '#4f5bd5'
export const MUTED = '#c7cbd4'
export const GOOD = '#2f9e68'
export const WARN = '#d99a2b'
export const BAD = '#d2523f'
