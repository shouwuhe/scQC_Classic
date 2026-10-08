#!/usr/bin/env node
/**
 * 生成 `src/assets/third-party.json`：界面"版权与致谢"页里第三方软件那一节的数据源。
 *
 * 为什么要机器生成而不是手写：许可证是法律信息，手写的列表会随依赖升级悄悄过时，
 * 而且没人会发现。这里直接问包管理器要：
 *   - Rust：`cargo metadata` 的 `license` 字段（来自每个 crate 自己的 Cargo.toml）；
 *   - npm：递归读 `node_modules/<pkg>/package.json` 的 `license` 字段，只收**会进产物的**
 *     运行时依赖（`dependencies` 及其传递依赖），不收构建工具；
 *   - 运行时本体：Electron / Chromium / Node.js 单独列出（它们随包分发）。
 *
 * 在 `npm run build` 里自动执行，因此打出来的包里这份清单一定与实际依赖一致。
 */
import { execFileSync } from 'node:child_process'
import { existsSync, readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const OUT = join(ROOT, 'src', 'assets', 'third-party.json')

const norm = (v) => (typeof v === 'string' ? v : v && typeof v === 'object' ? (v.type ?? '') : '')

function rustCrates() {
  try {
    // --offline：只读 Cargo.lock 与本地缓存，不联网；构建机断网也能出清单
    const raw = execFileSync('cargo', ['metadata', '--format-version', '1', '--locked', '--offline'], {
      cwd: ROOT,
      maxBuffer: 1 << 26,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    })
    const meta = JSON.parse(raw)
    const own = new Set((meta.workspace_members ?? []).map((m) => String(m)))
    const seen = new Map()
    for (const p of meta.packages ?? []) {
      if (own.has(p.id)) continue
      if (p.name === 'scqc-core' || p.name === 'scqc-engine') continue
      const key = `${p.name}@${p.version}`
      if (seen.has(key)) continue
      seen.set(key, {
        name: p.name,
        version: p.version,
        license: p.license ?? (p.license_file ? `see ${p.license_file}` : ''),
        repository: p.repository ?? '',
      })
    }
    return [...seen.values()].sort((a, b) => a.name.localeCompare(b.name))
  } catch (e) {
    console.warn('[licenses] cargo metadata 不可用，跳过 Rust 依赖：', String(e).split('\n')[0])
    return []
  }
}

/** 只收会进产物的运行时依赖：package.json 的 dependencies 及其传递依赖。 */
function npmPackages() {
  const pkg = JSON.parse(readFileSync(join(ROOT, 'package.json'), 'utf8'))
  const out = new Map()
  const visit = (name, depth) => {
    if (depth > 12 || out.has(name)) return
    const dir = join(ROOT, 'node_modules', name)
    const pj = join(dir, 'package.json')
    if (!existsSync(pj)) return
    let meta
    try {
      meta = JSON.parse(readFileSync(pj, 'utf8'))
    } catch {
      return
    }
    out.set(name, {
      name,
      version: meta.version ?? '',
      license: norm(meta.license) || (Array.isArray(meta.licenses) ? meta.licenses.map(norm).join(' OR ') : ''),
      repository: norm(meta.repository?.url ?? meta.repository ?? ''),
    })
    for (const d of Object.keys(meta.dependencies ?? {})) visit(d, depth + 1)
  }
  for (const d of Object.keys(pkg.dependencies ?? {})) visit(d, 0)
  return [...out.values()].sort((a, b) => a.name.localeCompare(b.name))
}

/** 随包分发的运行时本体。版本从已安装的 electron 读，读不到就留空而不是猜。 */
function runtimes() {
  const list = []
  const ep = join(ROOT, 'node_modules', 'electron', 'package.json')
  let electronVersion = ''
  if (existsSync(ep)) {
    try {
      electronVersion = JSON.parse(readFileSync(ep, 'utf8')).version ?? ''
    } catch {
      /* 读不到就留空 */
    }
  }
  list.push({ name: 'Electron', version: electronVersion, license: 'MIT', repository: 'https://github.com/electron/electron' })
  list.push({ name: 'Chromium', version: '', license: 'BSD-3-Clause', repository: 'https://www.chromium.org/' })
  list.push({ name: 'Node.js', version: '', license: 'MIT', repository: 'https://nodejs.org/' })
  return list
}

// 某一侧的工具链不在时（例如只有 Node 没有 cargo 的构建环境），保留上一次真实采集到的结果，
// 而不是把它清空——清空会让版权页悄悄少掉一整块依赖，这比过时更糟。
let prev = { rust: [], npm: [], runtimes: [] }
if (existsSync(OUT)) {
  try {
    prev = JSON.parse(readFileSync(OUT, 'utf8'))
  } catch {
    /* 上一份坏了就当没有 */
  }
}
const rust = rustCrates()
const npm = npmPackages()
const data = {
  generatedAt: new Date().toISOString().slice(0, 10),
  rust: rust.length ? rust : (prev.rust ?? []),
  npm: npm.length ? npm : (prev.npm ?? []),
  runtimes: runtimes(),
}
mkdirSync(dirname(OUT), { recursive: true })
writeFileSync(OUT, JSON.stringify(data, null, 2) + '\n')
console.log(
  `[licenses] ${OUT.replace(ROOT + '/', '')}：Rust ${data.rust.length} 个、npm ${data.npm.length} 个、运行时 ${data.runtimes.length} 个`,
)
