import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process'
import { existsSync } from 'node:fs'
import { mkdtemp, writeFile, readFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { app } from 'electron'

/**
 * scqc-engine 子进程管理（SPEC §4.3、§10）。
 *
 * - stdout 逐行 NDJSON；stderr 是人类日志，只在出错时回传。
 * - 取消走 stdin（Windows 上 Electron 无法发 SIGTERM，见 DESIGN §9.5）。
 * - 引擎绝不联网；这里也不做任何网络访问（硬约束 C1）。
 */

export interface EngineRunArgs {
  rawDir: string | null
  filteredDir: string | null
  metricsCsv: string | null
  platform: string | null
  species: string | null
  multiplexN: number | null
  mitoGuardrail: number | null
  correct: 'none' | 'soup' | 'decontx'
  forceCorrect: boolean
  /** 跑完第 7 步停下来问一次校正决定，而不是让界面为这一个决定重跑前七步 */
  awaitCorrection?: boolean
  /** 批量时用来区分同时存在的多个引擎进程 */
  runId?: string
  outputDir: string
  params: Record<string, unknown>
  /** 报告语言：英语 + 这个语言（'en' 时只出英语） */
  reportLang: 'en' | 'zh' | 'ja'
}

export function enginePath(): string {
  // Windows 上可执行文件带 .exe；其余平台没有后缀
  const name = process.platform === 'win32' ? 'scqc-engine.exe' : 'scqc-engine'
  // 打包后：<Resources>/bin/<name>；开发时：仓库内的 release 构建
  const packaged = join(process.resourcesPath ?? '', 'bin', name)
  if (existsSync(packaged)) return packaged
  const dev = join(app.getAppPath(), 'target.nosync', 'release', name)
  if (existsSync(dev)) return dev
  return join(app.getAppPath(), 'target', 'release', name)
}

function argsFor(a: EngineRunArgs, paramsFile: string): string[] {
  const out = ['run', '--filtered', a.filteredDir ?? '', '--output', a.outputDir, '--params', paramsFile]
  if (a.rawDir) out.push('--raw', a.rawDir)
  if (a.metricsCsv) out.push('--metrics', a.metricsCsv)
  if (a.platform) out.push('--platform', a.platform)
  if (a.species) out.push('--species', a.species)
  if (a.multiplexN != null) out.push('--multiplex-n', String(a.multiplexN))
  if (a.mitoGuardrail != null) out.push('--mito-guardrail', String(a.mitoGuardrail))
  if (a.correct !== 'none') out.push('--correct', a.correct)
  if (a.forceCorrect) out.push('--force-correct')
  if (a.awaitCorrection) out.push('--await-correction')
  if (a.reportLang) out.push('--report-lang', a.reportLang)
  return out
}

export class EngineRun {
  private child: ChildProcessWithoutNullStreams | null = null
  private buf = ''
  private stderr: string[] = []

  constructor(
    private readonly onEvent: (e: unknown) => void,
    private readonly onClose: (code: number | null, stderr: string) => void,
  ) {}

  async start(a: EngineRunArgs): Promise<void> {
    const dir = await mkdtemp(join(tmpdir(), 'scqc-'))
    const paramsFile = join(dir, 'params.json')
    await writeFile(paramsFile, JSON.stringify(a.params, null, 2), 'utf8')
    const bin = enginePath()
    this.child = spawn(bin, argsFor(a, paramsFile), {
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...process.env, RUST_LOG: 'info' },
    }) as ChildProcessWithoutNullStreams
    this.child.stdout.setEncoding('utf8')
    this.child.stdout.on('data', (chunk: string) => this.feed(chunk))
    this.child.stderr.setEncoding('utf8')
    this.child.stderr.on('data', (chunk: string) => {
      this.stderr.push(chunk)
      if (this.stderr.length > 400) this.stderr.splice(0, this.stderr.length - 400)
    })
    this.child.on('close', (code) => {
      this.flush()
      this.onClose(code, this.stderr.join(''))
      this.child = null
    })
  }

  private feed(chunk: string) {
    this.buf += chunk
    let idx: number
    while ((idx = this.buf.indexOf('\n')) >= 0) {
      const line = this.buf.slice(0, idx).trim()
      this.buf = this.buf.slice(idx + 1)
      if (line) this.emit(line)
    }
  }

  private flush() {
    const line = this.buf.trim()
    this.buf = ''
    if (line) this.emit(line)
  }

  private emit(line: string) {
    try {
      this.onEvent(JSON.parse(line))
    } catch {
      /* 非 JSON 行忽略：引擎保证 stdout 只有 NDJSON，这里只是防御 */
    }
  }

  /** 回答引擎的 `await_correction`：走的是取消用的同一条 stdin 通道 */
  answerCorrection(method: 'none' | 'soup' | 'decontx', force: boolean) {
    if (!this.child) return
    try {
      this.child.stdin.write(`${JSON.stringify({ type: 'correct', method, force })}\n`)
    } catch {
      /* 进程可能已退出 */
    }
  }

  cancel() {
    if (!this.child) return
    try {
      this.child.stdin.write('cancel\n')
    } catch {
      /* 进程可能已退出 */
    }
    setTimeout(() => this.child?.kill('SIGTERM'), 3000)
  }

  get running() {
    return this.child !== null
  }
}

export async function readReport(outputDir: string): Promise<unknown> {
  const txt = await readFile(join(outputDir, 'details', 'report_data.json'), 'utf8')
  return JSON.parse(txt)
}

/**
 * 引擎版本号，形如 `0.1.0 (a1b2c3d)`。
 *
 * `scqc-engine version` 走的是和其它子命令一样的 NDJSON 协议（stdout 只出 NDJSON），
 * 所以这里必须解析，不能把整行原样往界面上放——那会在版权页上显示一行 JSON。
 */
export async function engineVersion(): Promise<string> {
  return new Promise((resolve) => {
    const child = spawn(enginePath(), ['version'])
    let out = ''
    child.stdout.on('data', (d: Buffer) => (out += d.toString()))
    child.on('close', () => {
      let version = ''
      let hash = ''
      for (const line of out.split('\n')) {
        const s = line.trim()
        if (!s.startsWith('{')) continue
        try {
          const ev = JSON.parse(s) as { type?: string; summary?: Record<string, unknown> }
          if (ev.type !== 'result' || !ev.summary) continue
          version = String(ev.summary.version ?? '')
          hash = String(ev.summary.git_hash ?? '')
        } catch {
          /* 不是我们认识的那一行就跳过 */
        }
      }
      if (!version) return resolve('')
      resolve(hash && hash !== 'unknown' ? `${version} (${hash.slice(0, 7)})` : version)
    })
    child.on('error', () => resolve(''))
  })
}
