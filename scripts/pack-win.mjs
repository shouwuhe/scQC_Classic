#!/usr/bin/env node
/**
 * 在 macOS 上打出 **Windows x86_64** 安装包与免安装压缩包。
 *
 * 引擎用 `x86_64-pc-windows-gnu` 交叉编译（mingw-w64 提供链接器），
 * 不走 MSVC 目标 —— 那个需要 Windows SDK，macOS 上拿不到。
 *
 * 前置（只装一次）：
 *   rustup target add x86_64-pc-windows-gnu
 *   brew install mingw-w64
 *
 * 产物（release/）：
 *   scQC-<版本>-win-x64-setup.exe   安装程序（按用户安装，不需要管理员）
 *   scQC-<版本>-win-x64.zip         免安装版，解压即用，适合拷到测试机
 *
 * 未做代码签名：Windows 会弹 SmartScreen，点"更多信息 → 仍要运行"。
 */
import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync, rmSync, existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const TRIPLE = 'x86_64-pc-windows-gnu'
/**
 * 编译产物放到**不含空格**的目录。仓库在 iCloud 里，路径带空格，
 * 而 mingw 的 dlltool / as 不给临时路径加引号，会在第一个空格处截断然后报
 * "can't open ... No such file or directory"。macOS 原生编译不受影响，只有交叉编译踩这个坑。
 */
const TARGET_DIR = join(process.env.HOME ?? ROOT, '.scqc-win-target')

const run = (cmd, args, opts = {}) => {
  console.log(`$ ${cmd} ${args.join(' ')}`)
  execFileSync(cmd, args, { cwd: ROOT, stdio: 'inherit', ...opts })
}
const runCargo = (args) =>
  run('cargo', args, { env: { ...process.env, CARGO_TARGET_DIR: TARGET_DIR } })

// 0. 前置检查：缺了就直说，别让 cargo 报一堆链接错误
try {
  execFileSync('x86_64-w64-mingw32-gcc', ['--version'], { stdio: 'ignore' })
} catch {
  console.error('缺少 mingw-w64（交叉编译的链接器）。先运行：brew install mingw-w64')
  process.exit(1)
}

// 1. 引擎
runCargo(['build', '--release', '--target', TRIPLE, '-p', 'scqc-engine'])
const bin = join(TARGET_DIR, TRIPLE, 'release', 'scqc-engine.exe')
if (!existsSync(bin)) {
  console.error(`engine binary not found: ${bin}`)
  process.exit(1)
}
const stage = join(ROOT, 'build', 'engine')
rmSync(stage, { recursive: true, force: true })
mkdirSync(stage, { recursive: true })
copyFileSync(bin, join(stage, 'scqc-engine.exe'))

// 2. 渲染进程 + 主进程
run('npm', ['run', 'build'])

// 3. 打包
run('npx', ['electron-builder', '--win', '--x64', '--publish', 'never'])
console.log('\n✅ Windows x64 build done → release/')
