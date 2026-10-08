import type { Lang } from '@/i18n'

const LOCALE: Record<Lang, string> = { en: 'en-US', zh: 'zh-CN', ja: 'ja-JP' }

export function num(v: number | string | null | undefined, lang: Lang = 'en'): string {
  const n = typeof v === 'string' ? Number(v.replace(/,/g, '')) : v
  if (n == null || !Number.isFinite(n)) return '—'
  return new Intl.NumberFormat(LOCALE[lang]).format(n)
}

export function pct(v: number | null | undefined, digits = 1): string {
  if (v == null || !Number.isFinite(v)) return '—'
  return `${(v * 100).toFixed(digits)}%`
}

export function secs(v: number | string | null | undefined): string {
  const n = typeof v === 'string' ? Number(v) : v
  if (n == null || !Number.isFinite(n)) return '—'
  if (n < 90) return `${n.toFixed(1)} s`
  const m = Math.floor(n / 60)
  return `${m} min ${Math.round(n - m * 60)} s`
}

/** 从报告里的 "2,874" / "6.71%" 这类字符串取数值。 */
export function parseNum(s: string | undefined | null): number | null {
  if (!s) return null
  const m = s.replace(/,/g, '').match(/-?\d+(\.\d+)?/)
  return m ? Number(m[0]) : null
}
