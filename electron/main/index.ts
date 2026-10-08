import { app, BrowserWindow, dialog, ipcMain, shell, Menu } from 'electron'
import { existsSync, statSync } from 'node:fs'
import { readdir, stat } from 'node:fs/promises'
import { join } from 'node:path'
import { spawn } from 'node:child_process'
import { EngineRun, enginePath, readReport, engineVersion } from './engine'
import { scanInput } from './scan'
import { runSmoke, smokeDir, smokeInput, smokeOutput } from './smoke'

/** 主进程：窗口、文件对话框、引擎子进程。绝不访问网络（硬约束 C1）。 */

let win: BrowserWindow | null = null

const isDev = !app.isPackaged && process.env.SCQC_DEV === '1'

function createWindow() {
  win = new BrowserWindow({
    width: 1280,
    height: 860,
    minWidth: 1040,
    minHeight: 720,
    title: 'scQC',
    // 只有 macOS 把红绿灯嵌进内容区；Windows 用系统标准标题栏
    ...(process.platform === 'darwin' ? ({ titleBarStyle: 'hiddenInset' } as const) : {}),
    backgroundColor: '#ffffff',
    webPreferences: {
      preload: join(__dirname, '..', 'preload', 'index.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
    },
  })
  if (isDev) {
    void win.loadURL('http://localhost:5173')
  } else {
    void win.loadFile(join(app.getAppPath(), 'dist', 'index.html'))
  }
  win.on('closed', () => (win = null))
  if (smokeDir) {
    win.webContents.once('did-finish-load', () => {
      void runSmoke(win as BrowserWindow, () => app.quit())
    })
  }
  // 外部链接一律用系统浏览器打开，窗口里不允许导航到别处
  win.webContents.setWindowOpenHandler(({ url }) => {
    void shell.openExternal(url)
    return { action: 'deny' }
  })
  installMenu(win)
  installEditShortcuts(win)
}

/**
 * 应用菜单。
 *
 * Windows / Linux 上没有系统级菜单栏，Electron 会把菜单画进窗口里，变成标题栏下面
 * 一条横条 —— 而 scQC 的每一项功能界面上都有入口，那条横条里没有一项是有用的，
 * 所以直接不要（`null`）。
 *
 * macOS 上应用菜单是系统约定，不能没有，但只留两项：关于（打开我们自己的版权页，
 * 不是 Electron 那个简陋的默认面板）和退出。编辑、窗口两个菜单删掉了 ——
 * 代价是 macOS 的 Cmd+C/V/X/A/Z 要靠菜单项才绑得上，所以下面用 before-input-event 补回来。
 */
function installMenu(win: BrowserWindow) {
  if (process.platform !== 'darwin') {
    Menu.setApplicationMenu(null)
    return
  }
  Menu.setApplicationMenu(
    Menu.buildFromTemplate([
      {
        label: app.name,
        submenu: [
          { label: `About ${app.name}`, click: () => win.webContents.send('menu:credits') },
          { type: 'separator' },
          { role: 'quit' },
        ],
      },
    ]),
  )
}

/** 没有编辑菜单时，macOS 的标准编辑快捷键要自己接一遍。 */
function installEditShortcuts(win: BrowserWindow) {
  if (process.platform !== 'darwin') return
  const actions: Record<string, (c: Electron.WebContents) => void> = {
    c: (c) => c.copy(),
    v: (c) => c.paste(),
    x: (c) => c.cut(),
    a: (c) => c.selectAll(),
    z: (c) => c.undo(),
    Z: (c) => c.redo(),
  }
  win.webContents.on('before-input-event', (e, input) => {
    if (!input.meta || input.control || input.alt || input.type !== 'keyDown') return
    const key = input.shift && input.key.toLowerCase() === 'z' ? 'Z' : input.key.toLowerCase()
    const act = actions[key]
    if (!act) return
    act(win.webContents)
    e.preventDefault()
  })
}

app.setName('scQC')

app.whenReady().then(() => {
  createWindow()
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

/**
 * 同时可以有多个引擎进程：批量时每个样本跑完第 7 步就停在那里等校正决定，
 * 凑齐整批再一起回答（SPEC §11.3）。只有一个进程在算，其余都停着，
 * 所以 CPU 不抢，只是各自占着自己的中间态内存。
 */
const runs = new Map<string, EngineRun>()

app.on('window-all-closed', () => {
  for (const r of runs.values()) r.cancel()
  if (process.platform !== 'darwin') app.quit()
})

ipcMain.handle('dialog:folder', async (_e, title?: string) => {
  // 冒烟测试模式下不弹系统对话框，直接给出预置目录
  if (smokeInput) return smokeInput
  const r = await dialog.showOpenDialog({ title, properties: ['openDirectory'] })
  return r.canceled ? null : (r.filePaths[0] ?? null)
})

ipcMain.handle('dialog:file', async (_e, title?: string, exts?: string[]) => {
  const r = await dialog.showOpenDialog({
    title,
    properties: ['openFile'],
    filters: exts?.length ? [{ name: 'files', extensions: exts }] : undefined,
  })
  return r.canceled ? null : (r.filePaths[0] ?? null)
})

/** 看一眼目录：是不是 10x 矩阵目录，有哪些文件、多大。 */
ipcMain.handle('fs:inspect', async (_e, dir: string) => {
  if (!dir || !existsSync(dir) || !statSync(dir).isDirectory()) return { ok: false, reason: 'not_a_directory' }
  const names = await readdir(dir)
  const has = (base: string) => names.find((n) => n === base || n === `${base}.gz`) ?? null
  const matrix = has('matrix.mtx')
  const features = has('features.tsv') ?? has('genes.tsv')
  const barcodes = has('barcodes.tsv')
  const size = (n: string | null) => (n ? statSync(join(dir, n)).size : 0)
  return {
    ok: Boolean(matrix && features && barcodes),
    dir,
    files: { matrix, features, barcodes },
    bytes: size(matrix) + size(features) + size(barcodes),
    siblings: names.filter((n) => n.endsWith('.csv') || n.endsWith('.h5')).slice(0, 20),
  }
})

/**
 * 给这次运行挑一个**不存在**的输出目录（硬约束 C7：绝不覆盖输入或已有结果）。
 * 默认放在输入目录的上一级：`<parent>/scqc-<样本名>-<yymmdd-hhmm>`。
 */
const safeName = (n: string) => n.replace(/[/\\:*?"<>|]+/g, '_').replace(/\s+/g, '_').slice(0, 60) || 'sample'

/** 批量运行时每个样本一个子目录；基目录只算一次，这里只做拼接。 */
ipcMain.handle('fs:subOutput', (_e, base: string, name: string) => join(base, safeName(name)))

/**
 * 建议输出目录：`<parent>/scqc-<name>-<时间戳>`，已存在就往后加序号。
 *
 * `parent` 由扫描结果给出（`ScanResult.outputParent`，= 用户拖进来的那个东西的同级目录），
 * 不再从样本目录往上推——那样拖一棵树进来会把结果写进树里面，拖压缩包会写进临时目录。
 */
ipcMain.handle('fs:suggestOutput', (_e, parent: string, name: string, tag?: string) => {
  if (smokeOutput) return tag ? `${smokeOutput}-${tag}` : smokeOutput
  const sample = (name || 'sample').replace(/[/\\:*?"<>|]/g, '_').trim() || 'sample'
  const d = new Date()
  const p2 = (n: number) => String(n).padStart(2, '0')
  const stamp = `${String(d.getFullYear()).slice(2)}${p2(d.getMonth() + 1)}${p2(d.getDate())}-${p2(d.getHours())}${p2(d.getMinutes())}`
  const base = join(parent, `scqc-${sample}-${stamp}${tag ? `-${tag}` : ''}`)
  let out = base
  let i = 2
  while (existsSync(out)) out = `${base}-${i++}`
  return out
})

/** 输出目录里到底写了什么（导出页逐条列出，让用户知道拿到了哪些文件）。 */
ipcMain.handle('fs:listOutput', async (_e, dir: string) => {
  if (!dir || !existsSync(dir)) return { ok: false, entries: [] }
  const walk = async (d: string, prefix: string, depth: number): Promise<{ path: string; bytes: number }[]> => {
    const out: { path: string; bytes: number }[] = []
    for (const name of await readdir(d)) {
      const full = join(d, name)
      const st = await stat(full)
      if (st.isDirectory()) {
        if (depth > 0) out.push(...(await walk(full, `${prefix}${name}/`, depth - 1)))
      } else {
        out.push({ path: `${prefix}${name}`, bytes: st.size })
      }
    }
    return out
  }
  const entries = (await walk(dir, '', 2)).sort((a, b) => a.path.localeCompare(b.path))
  return { ok: true, entries }
})

ipcMain.handle('dialog:save', async (_e, defaultPath: string, ext: string) => {
  const r = await dialog.showSaveDialog({ defaultPath, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] })
  return r.canceled ? null : (r.filePath ?? null)
})

/**
 * 读 `plots/` 的 manifest 与二进制列数据（SPEC §10.3：大数组不走 JSON）。
 * 原始字节直接交给渲染进程，由它构造 Float32Array 喂给 Plotly。
 */
ipcMain.handle('plots:read', async (_e, dir: string, names: string[]) => {
  const pdir = join(dir, 'details', 'plots')
  if (!existsSync(pdir)) return { ok: false, entries: [], buffers: {} }
  const { readFile } = await import('node:fs/promises')
  const manifest = JSON.parse(await readFile(join(pdir, 'manifest.json'), 'utf8')) as {
    name: string
    file: string
    shape: number[]
    columns: string[]
  }[]
  const buffers: Record<string, Uint8Array> = {}
  for (const e of manifest) {
    if (names.length && !names.includes(e.name)) continue
    const p = join(pdir, e.file)
    if (existsSync(p)) buffers[e.name] = new Uint8Array(await readFile(p))
  }
  return { ok: true, entries: manifest, buffers }
})

/** 拖进来的东西 → 一组可分析的样本（目录树任意深度，压缩包自动解压）。 */
ipcMain.handle('fs:scan', (_e, path: string) => scanInput(path))

/** 版权页用：问引擎要"所用算法的论文出处"。与报告第 8 节同源。 */
ipcMain.handle('engine:credits', async (_e, lang: string) => {
  return new Promise((resolve) => {
    const child = spawn(enginePath(), ['credits', '--lang', lang])
    let out = ''
    child.stdout.on('data', (d: Buffer) => (out += d.toString()))
    child.on('error', () => resolve(null))
    child.on('close', () => {
      for (const line of out.split('\n')) {
        const t = line.trim()
        if (!t) continue
        try {
          const ev = JSON.parse(t) as { type?: string; summary?: unknown }
          if (ev.type === 'result') return resolve(ev.summary ?? null)
        } catch {
          /* 非 JSON 行忽略 */
        }
      }
      resolve(null)
    })
  })
})

ipcMain.handle('engine:version', () => engineVersion())

/** detect：只跑平台识别，用于导入页的即时反馈。 */
ipcMain.handle('engine:detect', async (_e, args: { rawDir?: string; filteredDir: string; metricsCsv?: string }) => {
  return new Promise((resolve) => {
    const a = ['detect', '--filtered', args.filteredDir]
    if (args.rawDir) a.push('--raw', args.rawDir)
    if (args.metricsCsv) a.push('--metrics', args.metricsCsv)
    const child = spawn(enginePath(), a)
    let out = ''
    let err = ''
    child.stdout.on('data', (d: Buffer) => (out += d.toString()))
    child.stderr.on('data', (d: Buffer) => (err += d.toString()))
    child.on('error', (e) => resolve({ ok: false, error: String(e) }))
    child.on('close', (code) => {
      const events = out
        .split('\n')
        .map((l) => l.trim())
        .filter(Boolean)
        .map((l) => {
          try {
            return JSON.parse(l)
          } catch {
            return null
          }
        })
        .filter(Boolean)
      resolve({ ok: code === 0 || code === 3, code, events, stderr: err.slice(-4000) })
    })
  })
})

ipcMain.handle(
  'engine:run',
  async (_e, args: Parameters<EngineRun['start']>[0] & { runId?: string }) => {
    const id = args.runId ?? 'single'
    if (runs.get(id)?.running) return { ok: false, error: 'already_running' }
    return new Promise((resolve) => {
      const run = new EngineRun(
        (ev) => win?.webContents.send('engine:event', ev),
        (code, stderr) => {
          runs.delete(id)
          resolve({ ok: code === 0, code, stderr: stderr.slice(-8000) })
        },
      )
      runs.set(id, run)
      run.start(args).catch((e: unknown) => {
        runs.delete(id)
        resolve({ ok: false, error: String(e) })
      })
    })
  },
)

ipcMain.handle('engine:cancel', () => {
  for (const r of runs.values()) r.cancel()
})

/** runId 省略 = 回答全部停着的进程（批量时一个决定管整批） */
ipcMain.handle(
  'engine:answer-correction',
  (_e, method: 'none' | 'soup' | 'decontx', force: boolean, runId?: string) => {
    if (runId) runs.get(runId)?.answerCorrection(method, force)
    else for (const r of runs.values()) r.answerCorrection(method, force)
  },
)

ipcMain.handle('engine:report', (_e, dir: string) => readReport(dir))

ipcMain.handle('shell:open', (_e, p: string) => shell.openPath(p))
ipcMain.handle('shell:reveal', (_e, p: string) => shell.showItemInFolder(p))

/** 报告 PDF：用离屏窗口加载本地 report.html 后打印（SPEC §4.5）。 */
ipcMain.handle('report:pdf', async (_e, htmlPath: string, pdfPath: string) => {
  const w = new BrowserWindow({ show: false, webPreferences: { offscreen: true, javascript: false } })
  await w.loadFile(htmlPath)
  const data = await w.webContents.printToPDF({
    printBackground: true,
    pageSize: 'A4',
    margins: { top: 0.4, bottom: 0.4, left: 0.4, right: 0.4 },
  })
  const { writeFile } = await import('node:fs/promises')
  await writeFile(pdfPath, data)
  w.destroy()
})
