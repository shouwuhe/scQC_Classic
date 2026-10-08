import { mkdir, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import type { BrowserWindow } from 'electron'

/**
 * 打包后 App 的冒烟测试（只在设置了 `SCQC_SMOKE_DIR` 时启用）。
 *
 * 为什么需要它：浏览器里的 mock 能验证界面逻辑，却证明不了"**打包后的 .app 真的能跑**"——
 * preload 是否注入成功、CSP 是否拦住了自己的脚本、`Contents/Resources/bin/scqc-engine`
 * 能否找到并启动、真实数据能否跑完。而在 CI 或远程会话里又没有屏幕录制权限，
 * `screencapture` 拿不到窗口。`webContents.capturePage()` 走 Electron 自己的合成器，不需要任何系统权限。
 *
 * 用法：
 * ```
 * SCQC_SMOKE_DIR=/tmp/scqc-smoke \
 * SCQC_SMOKE_INPUT=/path/to/sample \      # 目录下要有 raw/ 与 filtered/
 * SCQC_SMOKE_OUTPUT=/tmp/scqc-smoke-out \ # 引擎写结果的目录（必须不存在）
 *   ./release/mac-arm64/scQC.app/Contents/MacOS/scQC
 * ```
 * 另有 `SCQC_SMOKE_SCAN_ONLY=1`：只验证"拖一棵目录树进来能扫出多个样本并列成表"，
 * 扫完即退出，不跑引擎（真实数据跑满一整队要十几分钟，不适合放进冒烟）。
 * 跑完自动退出；失败时把最后一屏截图和错误写进 `SCQC_SMOKE_DIR`。
 */

export const smokeDir = process.env.SCQC_SMOKE_DIR || null
export const smokeInput = process.env.SCQC_SMOKE_INPUT || null
export const smokeOutput = process.env.SCQC_SMOKE_OUTPUT || null
/** 只验证"扫描 + 批量表格"，不真的跑引擎（真实数据跑满一队要十几分钟） */
export const smokeScanOnly = process.env.SCQC_SMOKE_SCAN_ONLY === '1'

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))

async function shot(win: BrowserWindow, name: string) {
  if (!smokeDir) return
  const img = await win.webContents.capturePage()
  await writeFile(join(smokeDir, `${name}.png`), img.toPNG())
  log(`shot ${name}`)
}

const lines: string[] = []
function log(s: string) {
  lines.push(s)
  process.stderr.write(`[smoke] ${s}\n`)
}

/**
 * 点按钮。优先按 `data-smoke="<id>"` 找，退回按可见文字找。
 *
 * 为什么要有 `data-smoke`：按文字点两次把这个测试搞坏过 ——
 * 一次是改文案（"Export" 改成了 "Output"），一次是 App 记住了上一次用户选的中文界面，
 * 于是英文标签一个都对不上。两次都表现为"静默点不到"，而不是明确报错。
 * 界面文案随时会改、语言随时会切，测试不该挂在它们身上。
 */
async function click(win: BrowserWindow, id: string, text?: string): Promise<boolean> {
  const ok = await win.webContents.executeJavaScript(
    `(() => {
       const id = ${JSON.stringify(id)}
       const t = ${JSON.stringify(text ?? '')}
       let el = document.querySelector('[data-smoke=' + JSON.stringify(id) + ']')
       if (!el && t) {
         const els = [...document.querySelectorAll('button, a, [role="button"], [role="option"]')]
         el = els.find((e) => (e.textContent || '').trim() === t) ||
              els.find((e) => (e.textContent || '').trim().startsWith(t))
       }
       if (!el) return false
       el.click()
       return true
     })()`,
  )
  log(`click ${id} → ${ok}`)
  return Boolean(ok)
}

async function textOf(win: BrowserWindow, selector: string): Promise<string> {
  return String(
    await win.webContents.executeJavaScript(
      `(document.querySelector(${JSON.stringify(selector)})?.textContent || '').trim().slice(0, 200)`,
    ),
  )
}

export async function runSmoke(win: BrowserWindow, quit: () => void): Promise<void> {
  if (!smokeDir) return
  let failed = false
  try {
    await mkdir(smokeDir, { recursive: true })
    await sleep(1500)
    // 界面语言存在 localStorage 里，会跨次启动保留。上一轮就是因为 App 记着中文，
    // 英文标签一个都点不到。冒烟测试固定跑英语，与用户上次选了什么无关。
    await win.webContents.executeJavaScript(`localStorage.setItem('scqc.lang', 'en')`)
    win.webContents.reload()
    await sleep(2500)
    await shot(win, '01-import-empty')

    if (smokeInput) {
      if (!(await click(win, 'browse', 'or choose a folder'))) throw new Error('no folder button')
      await sleep(smokeScanOnly ? 20000 : 3000) // 递归扫描 + 每个样本一次 detect 子进程
      await shot(win, '02-import-ready')

      if (smokeScanOnly) {
        const rows = Number(
          await win.webContents.executeJavaScript(`document.querySelectorAll('tbody tr').length`),
        )
        log(`scan rows=${rows}`)
        if (!Number.isFinite(rows) || rows < 2) throw new Error(`batch scan found ${rows} samples`)
        log('SMOKE OK')
        return
      }

      if (!(await click(win, 'run', 'Run QC'))) throw new Error('no Run QC button')
      await sleep(1500)
      await shot(win, '03-running')

      // 等引擎跑完（最多 8 分钟）：出现结论条即算完成
      // 第 1–7 步跑完后引擎会停下来问"要不要校正"（SPEC §11.3）。
      // 这里等那张卡出现、断言按钮可按，然后**真的选校正**——顺带验证第 8 步在打包后的 App 里能跑。
      let asked = false
      for (let i = 0; i < 120; i++) {
        await sleep(2000)
        const st = await win.webContents.executeJavaScript(
          `(() => { const b = document.querySelector('[data-smoke="ambient-run"]'); return b ? !b.disabled : null })()`,
        )
        if (st !== null) {
          log(`ambient pause reached, correct button enabled=${String(st)}`)
          if (st !== true) throw new Error('ambient correction button not pressable')
          await shot(win, '03b-ambient-pause')
          if (!(await click(win, 'ambient-run', 'Correct counts and re-run'))) {
            throw new Error('no ambient run button')
          }
          asked = true
          break
        }
      }
      if (!asked) throw new Error('engine never asked for the correction decision')

      const t0 = Date.now()
      let done = false
      while (Date.now() - t0 < 480_000) {
        await sleep(4000)
        // 不能按 h1 的文字判断（换语言就永远等不到）；结论条只在结果页存在
        const seen = await win.webContents.executeJavaScript(
          `Boolean(document.querySelector('[data-smoke="tune"]'))`,
        )
        if (seen) {
          done = true
          break
        }
      }
      log(`run finished=${done} in ${Math.round((Date.now() - t0) / 1000)}s, h1=${await textOf(win, 'h1')}`)
      await shot(win, '04-results')
      if (!done) throw new Error('engine run did not finish')

      // 按钮文字必须与 `src/i18n/en.ts` 完全一致，否则这里会静默点不到。
      if (!(await click(win, 'tune', 'Adjust filtering'))) throw new Error('no tuning button')
      await sleep(1200)
      await shot(win, '05-tuning')
      if (!(await click(win, 'back', 'Back'))) throw new Error('no back button')
      await sleep(800)

      if (!(await click(win, 'next', 'Continue'))) throw new Error('no continue button')
      await sleep(1500)
      await sleep(2000)
      await shot(win, '07-export')

      // 版权页：它读引擎的 `credits` 子命令，是"打包后引擎真的能被调起来"的第二个证据
      if (!(await click(win, 'credits', 'Credits'))) throw new Error('no credits button')
      await sleep(1500)
      const creditsN = Number(
        await win.webContents.executeJavaScript(
          `document.querySelectorAll('[role="dialog"] li, .fixed li').length`,
        ),
      )
      log(`credits entries=${creditsN}`)
      await shot(win, '08-credits')
      if (!Number.isFinite(creditsN) || creditsN < 20) throw new Error(`credits list too short: ${creditsN}`)
    }
    log('SMOKE OK')
  } catch (e) {
    failed = true
    log(`SMOKE FAILED: ${String(e)}`)
    try {
      await shot(win, '99-failure')
    } catch {
      /* 截图也失败就算了 */
    }
  } finally {
    if (smokeDir) await writeFile(join(smokeDir, 'smoke.log'), lines.join('\n') + '\n')
    process.exitCode = failed ? 1 : 0
    quit()
  }
}
