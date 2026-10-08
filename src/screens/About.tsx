import { useEffect, useState } from 'react'
import { X } from 'lucide-react'
import { Card } from '@/components/ui/primitives'
import { useT } from '@/i18n'
import { host, type EngineCredits } from '@/lib/api'
import thirdParty from '@/assets/third-party.json'

/**
 * 版权与致谢。
 *
 * 三块内容，来源各不相同，都不是手写死的：
 *  1. **算法与文献** —— 问引擎要（`scqc-engine credits`），与报告的"方法与文献"一节、`docs/methods.md` 同源；
 *  2. **第三方软件** —— `src/assets/third-party.json`，由 `scripts/collect-licenses.mjs`
 *     在每次构建时从 `cargo metadata` 与 `node_modules` 重新生成；
 *  3. **本软件自身** —— 许可证、版本、以及"数据不离开本机"这条硬约束。
 *
 * 手写的许可证清单会随依赖升级悄悄过时且没人发现，所以这里一行都不手写。
 */
export function AboutDialog({ onClose }: { onClose: () => void }) {
  const { t, lang } = useT()
  const [credits, setCredits] = useState<EngineCredits | null>(null)
  const [version, setVersion] = useState('')

  useEffect(() => {
    let alive = true
    void host()
      .credits(lang)
      .then((c) => alive && setCredits(c))
      .catch(() => {})
    void host()
      .engineVersion()
      .then((v) => alive && setVersion(v.trim()))
      .catch(() => {})
    return () => {
      alive = false
    }
  }, [lang])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  const tp = thirdParty as {
    generatedAt: string
    rust: Dep[]
    npm: Dep[]
    runtimes: Dep[]
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/30 px-6 py-10" onClick={onClose}>
      <Card
        className="flex max-h-full w-full max-w-[860px] flex-col overflow-hidden shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="flex shrink-0 items-center gap-3 border-b border-[var(--color-line)] px-6 py-4">
          <h2 className="text-[16px] font-semibold">{t.about.title}</h2>
          <span className="text-[12.5px] text-[var(--color-ink-faint)]">
            scQC {version || '—'}
          </span>
          <button
            className="ml-auto rounded-md p-1.5 text-[var(--color-ink-faint)] hover:bg-[var(--color-canvas)]"
            onClick={onClose}
            aria-label={t.app.close}
          >
            <X className="h-4 w-4" />
          </button>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
          {/* 1. 本软件 */}
          <Section title={t.about.software}>
            <p className="max-w-[74ch] text-[13.5px] leading-relaxed text-[var(--color-ink-soft)]">
              {t.about.softwareBody}
            </p>
            <dl className="mt-3 grid gap-x-8 gap-y-1.5 sm:grid-cols-2">
              <Row k={t.about.licenseLabel} v="MIT" />
              <Row k={t.overview.engineVersion} v={version || '—'} />
            </dl>
          </Section>

          {/* 2. 算法与文献 */}
          <Section title={t.about.algorithms}>
            <ul className="space-y-3">
              {(credits?.methods ?? []).map((m) => (
                <li key={m.key} className="border-l-2 border-[var(--color-line)] pl-3">
                  <div className="text-[13.5px] font-medium">{m.name}</div>
                  <div className="mt-0.5 text-[12.5px] leading-relaxed text-[var(--color-ink-soft)]">{m.citation}</div>
                  <div className="mt-0.5 text-[12px] leading-relaxed text-[var(--color-ink-faint)]">
                    {m.license_note}
                  </div>
                </li>
              ))}
              {!credits && <li className="text-[13px] text-[var(--color-ink-faint)]">{t.app.loading}</li>}
            </ul>
          </Section>

          {/* 3. 第三方软件 */}
          <Section title={t.about.thirdParty} note={tp.generatedAt ? `${t.about.generatedAt} ${tp.generatedAt}` : ''}>
            <DepTable title={t.about.runtimes} deps={tp.runtimes} />
            <DepTable title={t.about.rustCrates} deps={tp.rust} />
            <DepTable title={t.about.npmPackages} deps={tp.npm} />
            {tp.rust.length === 0 && tp.npm.length === 0 && (
              <p className="text-[13px] text-[var(--color-ink-faint)]">{t.about.thirdPartyEmpty}</p>
            )}
          </Section>

        </div>
      </Card>
    </div>
  )
}

interface Dep {
  name: string
  version: string
  license: string
  repository: string
}

function Section({ title, note, children }: { title: string; note?: string; children: React.ReactNode }) {
  return (
    <section className="mb-7 last:mb-0">
      <div className="mb-2 flex items-baseline gap-3">
        <h3 className="text-[14px] font-semibold">{title}</h3>
        {note && <span className="text-[12px] text-[var(--color-ink-faint)]">{note}</span>}
      </div>
      {children}
    </section>
  )
}

function Row({ k, v }: { k: string; v: string }) {
  return (
    <div className="flex items-baseline justify-between gap-4 text-[13px]">
      <dt className="text-[var(--color-ink-soft)]">{k}</dt>
      <dd className="shrink-0 font-medium">{v}</dd>
    </div>
  )
}

function DepTable({ title, deps }: { title: string; deps: Dep[] }) {
  if (!deps.length) return null
  return (
    <div className="mb-4 last:mb-0">
      <div className="mb-1 text-[12.5px] text-[var(--color-ink-soft)]">
        {title} <span className="tnum text-[var(--color-ink-faint)]">({deps.length})</span>
      </div>
      <ul className="columns-1 gap-x-8 text-[12px] leading-relaxed sm:columns-2">
        {deps.map((d) => (
          <li key={`${d.name}@${d.version}`} className="break-inside-avoid">
            <span className="font-mono">{d.name}</span>
            {d.version && <span className="tnum text-[var(--color-ink-faint)]"> {d.version}</span>}
            {d.license && <span className="text-[var(--color-ink-soft)]"> — {d.license}</span>}
          </li>
        ))}
      </ul>
    </div>
  )
}
