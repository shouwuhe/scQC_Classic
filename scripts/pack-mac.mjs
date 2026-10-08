#!/usr/bin/env node
/**
 * 打包 macOS 应用。
 *
 * 目前只出 **Apple Silicon（arm64）** 版本（阶段 1 决定）。x86_64 的路径完整保留：
 * `node scripts/pack-mac.mjs x64` 就会用 x86_64 的 Rust target 编译引擎并让
 * electron-builder 出 x64 包——没有任何一步是 arm64 专用的硬编码。
 * 交叉编译需要先 `rustup target add <triple>`。
 */
import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync, rmSync, existsSync, chmodSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const ARCH = (process.argv[2] ?? 'arm64').toLowerCase()
const TRIPLE = { arm64: 'aarch64-apple-darwin', x64: 'x86_64-apple-darwin' }[ARCH]
if (!TRIPLE) {
  console.error(`unsupported arch: ${ARCH} (expected arm64 | x64)`)
  process.exit(2)
}

const run = (cmd, args, opts = {}) => {
  console.log(`$ ${cmd} ${args.join(' ')}`)
  execFileSync(cmd, args, { cwd: ROOT, stdio: 'inherit', ...opts })
}

// 0. 图标：iconset → icns（PNG 由 scripts/make-icon.py 生成，已入库）
const iconset = join(ROOT, 'build', 'resources', 'icon.iconset')
const icns = join(ROOT, 'build', 'resources', 'icon.icns')
if (existsSync(iconset) && !existsSync(icns)) {
  run('iconutil', ['-c', 'icns', iconset, '-o', icns])
}

// 1. 引擎：按目标架构编译，产物放到 build/engine/scqc-engine
run('cargo', ['build', '--release', '--target', TRIPLE, '-p', 'scqc-engine'])
const bin = join(ROOT, 'target.nosync', TRIPLE, 'release', 'scqc-engine')
if (!existsSync(bin)) {
  console.error(`engine binary not found: ${bin}`)
  process.exit(1)
}
const stage = join(ROOT, 'build', 'engine')
rmSync(stage, { recursive: true, force: true })
mkdirSync(stage, { recursive: true })
copyFileSync(bin, join(stage, 'scqc-engine'))
chmodSync(join(stage, 'scqc-engine'), 0o755)

// 2. 渲染进程 + 主进程
run('npm', ['run', 'build'])

// 3. 打包（不签名、不公证：内部分发；正式发布时补 identity 与 notarize）
run('npx', ['electron-builder', '--mac', `--${ARCH}`, '--publish', 'never'])
console.log(`\n✅ ${ARCH} build done → release/`)
