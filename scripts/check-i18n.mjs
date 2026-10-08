#!/usr/bin/env node
/**
 * 三语词典一致性检查（SPEC §11.0）。
 *
 * TypeScript 只能保证 zh/ja 的**键**与 en 一致；这里额外检查两件类型系统看不见的事：
 *   1. 嵌套键集合完全一致（漏一个键会让界面掉回英文，typecheck 也可能放行 Record 索引）；
 *   2. `{占位符}` 集合一致（翻译时漏写 {n} 会让数字凭空消失，而且不会报错）。
 *
 * 用法：node --experimental-strip-types scripts/check-i18n.mjs
 */
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const SRC = join(dirname(fileURLToPath(import.meta.url)), '..', 'src', 'i18n')
const LANGS = ['en', 'zh', 'ja']

const mods = {}
for (const l of LANGS) mods[l] = (await import(join(SRC, `${l}.ts`)))[l]

const walk = (o, prefix = '') =>
  Object.entries(o)
    .flatMap(([k, v]) =>
      v && typeof v === 'object' && !Array.isArray(v) ? walk(v, `${prefix}${k}.`) : [`${prefix}${k}`],
    )
    .sort()

const get = (o, path) => path.split('.').reduce((a, k) => a?.[k], o)
const placeholders = (s) =>
  [...String(s).matchAll(/\{(\w+)\}/g)]
    .map((m) => m[1])
    .sort()
    .join(',')

const base = walk(mods.en)
let bad = 0
for (const l of LANGS.slice(1)) {
  const ks = walk(mods[l])
  const missing = base.filter((k) => !ks.includes(k))
  const extra = ks.filter((k) => !base.includes(k))
  if (missing.length || extra.length) {
    bad++
    console.error(`[${l}] 缺少 ${missing.length} 个键：`, missing.slice(0, 20))
    console.error(`[${l}] 多出 ${extra.length} 个键：`, extra.slice(0, 20))
  }
}
for (const k of base) {
  const a = placeholders(get(mods.en, k))
  for (const l of LANGS.slice(1)) {
    const b = placeholders(get(mods[l], k))
    if (a !== b) {
      bad++
      console.error(`占位符不一致 ${k}: en=[${a}] ${l}=[${b}]`)
    }
  }
}
// 3. 界面里引用的每个 t.xxx 都必须真实存在（打错字时 TS 只在字面量索引下才会报错）
const SRC_ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', 'src')
const files = []
const collect = (d) => {
  for (const n of readdirSync(d)) {
    const p = join(d, n)
    if (statSync(p).isDirectory()) collect(p)
    else if (/\.tsx?$/.test(p) && !p.includes(join('src', 'i18n'))) files.push(p)
  }
}
collect(SRC_ROOT)
const missing = new Set()
for (const f of files) {
  for (const m of readFileSync(f, 'utf8').matchAll(/\bt\.([A-Za-z_][\w.]*)/g)) {
    const path = m[1].replace(/\.$/, '')
    if (path.includes('(')) continue
    if (get(mods.en, path) === undefined) missing.add(`${f.replace(SRC_ROOT, 'src')}  t.${path}`)
  }
}
if (missing.size) {
  bad += missing.size
  console.error('界面引用了不存在的词典键：')
  for (const m of missing) console.error('  ' + m)
}

console.log(
  bad === 0
    ? `✅ i18n OK（${base.length} 个键 × ${LANGS.length} 种语言，${files.length} 个源文件的引用全部命中）`
    : `❌ i18n 有 ${bad} 处问题`,
)
process.exit(bad === 0 ? 0 : 1)
