import { useEffect, useState } from 'react'
import { Globe, Info } from 'lucide-react'
import { LANGS, useT } from '@/i18n'
import { Select } from '@/components/ui/primitives'
import { AboutDialog } from '@/screens/About'
import { cn } from '@/lib/utils'
import type { Lang } from '@/i18n'

export type ScreenId = 'import' | 'run' | 'review' | 'export'

/** 顶栏：产品名 + 四步指示器 + 语言。macOS 上给红绿灯留位。 */
const isMac = typeof navigator === 'undefined' || /Mac/i.test(navigator.platform || navigator.userAgent)

export function AppShell({
  screen,
  canJump,
  onJump,
  children,
}: {
  screen: ScreenId
  canJump: (s: ScreenId) => boolean
  onJump: (s: ScreenId) => void
  children: React.ReactNode
}) {
  const { t, lang, setLang } = useT()
  const [about, setAbout] = useState(false)
  // macOS 菜单栏的"关于"走这条线打开版权页；浏览器里跑设计稿时没有这个桥，忽略即可
  useEffect(() => window.scqc?.onMenuCredits?.(() => setAbout(true)), [])
  const steps: { id: ScreenId; label: string }[] = [
    { id: 'import', label: t.steps.import },
    { id: 'run', label: t.steps.run },
    { id: 'review', label: t.steps.review },
    { id: 'export', label: t.steps.export },
  ]
  const activeIdx = steps.findIndex((s) => s.id === screen)
  return (
    <div className="flex h-full flex-col">
      {/* 左边那段空白是给 macOS 红绿灯留的位；Windows 用系统标题栏，不需要 */}
      <header
        className={cn(
          'titlebar-drag flex h-14 shrink-0 items-center gap-4 border-b border-[var(--color-line)] bg-[var(--color-surface)] pr-4',
          isMac ? 'pl-[86px]' : 'pl-5',
        )}
      >
        <div className="text-[15px] font-semibold tracking-tight">{t.app.name}</div>
        <nav className="titlebar-nodrag flex items-center gap-1">
          {steps.map((s, i) => {
            const state = i === activeIdx ? 'active' : i < activeIdx ? 'done' : 'todo'
            return (
              <button
                key={s.id}
                data-smoke={`nav-${s.id}`}
                disabled={!canJump(s.id)}
                onClick={() => onJump(s.id)}
                className={cn(
                  'flex items-center gap-2 rounded-full px-3 py-1.5 text-[13px] transition',
                  state === 'active' && 'bg-[var(--color-accent-soft)] font-medium text-[var(--color-accent)]',
                  state === 'done' && 'text-[var(--color-ink-soft)] hover:bg-[var(--color-canvas)]',
                  state === 'todo' && 'text-[var(--color-ink-faint)]',
                  !canJump(s.id) && 'pointer-events-none',
                )}
              >
                <span
                  className={cn(
                    'flex h-5 w-5 items-center justify-center rounded-full text-[11px] tnum',
                    state === 'active' && 'bg-[var(--color-accent)] text-white',
                    state === 'done' && 'bg-[var(--color-line)] text-[var(--color-ink-soft)]',
                    state === 'todo' && 'border border-[var(--color-line)]',
                  )}
                >
                  {i + 1}
                </span>
                {s.label}
              </button>
            )
          })}
        </nav>
        <div className="titlebar-nodrag ml-auto flex items-center gap-1">
          <Globe className="h-4 w-4 text-[var(--color-ink-faint)]" />
          <Select<Lang>
            value={lang}
            onChange={setLang}
            options={LANGS.map((l) => ({ value: l.id, label: l.label }))}
            className="h-8 border-transparent bg-transparent hover:bg-[var(--color-canvas)]"
          />
          {/* 版权与致谢：放在最右下角落，不参与主流程 */}
          <button
            data-smoke="credits"
            onClick={() => setAbout(true)}
            title={t.about.title}
            aria-label={t.about.title}
            className="ml-1 flex h-8 items-center gap-1.5 rounded-md px-2 text-[12.5px] text-[var(--color-ink-faint)] transition hover:bg-[var(--color-canvas)] hover:text-[var(--color-ink-soft)]"
          >
            <Info className="h-3.5 w-3.5" />
            {t.about.open}
          </button>
        </div>
      </header>
      <main className="min-h-0 flex-1 overflow-y-auto">{children}</main>
      {about && <AboutDialog onClose={() => setAbout(false)} />}
    </div>
  )
}

export function Page({
  title,
  subtitle,
  children,
  wide = false,
}: {
  title?: string
  subtitle?: string
  children: React.ReactNode
  wide?: boolean
}) {
  return (
    <div className={cn('mx-auto px-8 pb-28 pt-10', wide ? 'max-w-[1120px]' : 'max-w-[860px]')}>
      {title && <h1 className="text-[26px] font-semibold tracking-tight">{title}</h1>}
      {subtitle && <p className="mt-2 max-w-[62ch] text-[14.5px] text-[var(--color-ink-soft)]">{subtitle}</p>}
      <div className="mt-8">{children}</div>
    </div>
  )
}

export function ActionBar({ children }: { children: React.ReactNode }) {
  return (
    <div className="fixed inset-x-0 bottom-0 z-20 border-t border-[var(--color-line)] bg-[var(--color-surface)]">
      <div className="mx-auto flex max-w-[1120px] items-center justify-end gap-3 px-8 py-4">{children}</div>
    </div>
  )
}
