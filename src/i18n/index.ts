import { createContext, useContext } from 'react'
import { en, type Dict } from './en'
import { zh } from './zh'
import { ja } from './ja'

export type Lang = 'en' | 'zh' | 'ja'
export const LANGS: { id: Lang; label: string }[] = [
  { id: 'en', label: 'English' },
  { id: 'zh', label: '简体中文' },
  { id: 'ja', label: '日本語' },
]
const DICTS: Record<Lang, Dict> = { en, zh, ja }

export const LangContext = createContext<{ lang: Lang; setLang: (l: Lang) => void }>({
  lang: 'en',
  setLang: () => {},
})

/** 默认英语（SPEC 阶段 1 要求）；用户选择存在 localStorage。 */
export function initialLang(): Lang {
  const saved = typeof localStorage !== 'undefined' ? localStorage.getItem('scqc.lang') : null
  if (saved === 'en' || saved === 'zh' || saved === 'ja') return saved
  return 'en'
}

export function useT(): { t: Dict; lang: Lang; setLang: (l: Lang) => void } {
  const { lang, setLang } = useContext(LangContext)
  return { t: DICTS[lang], lang, setLang }
}

/** "{kept} of {total}" 这类占位符替换。 */
export function fill(s: string, vars: Record<string, string | number>): string {
  return s.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m))
}

export type { Dict }
