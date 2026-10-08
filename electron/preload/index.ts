import { contextBridge, ipcRenderer, webUtils } from 'electron'

/** 渲染进程唯一能碰到的宿主 API（contextIsolation 打开，不暴露 Node）。 */
const api = {
  platform: process.platform,
  /** Electron 32+ 移除了 File.path，拖拽文件夹要走 webUtils */
  pathForFile: (f: File): string => webUtils.getPathForFile(f),
  chooseFolder: (title?: string): Promise<string | null> => ipcRenderer.invoke('dialog:folder', title),
  chooseFile: (title?: string, exts?: string[]): Promise<string | null> =>
    ipcRenderer.invoke('dialog:file', title, exts),
  inspectFolder: (dir: string): Promise<unknown> => ipcRenderer.invoke('fs:inspect', dir),
  scanInput: (path: string): Promise<unknown> => ipcRenderer.invoke('fs:scan', path),
  suggestOutput: (parent: string, name: string, tag?: string): Promise<string> =>
    ipcRenderer.invoke('fs:suggestOutput', parent, name, tag),
  subOutput: (base: string, name: string): Promise<string> => ipcRenderer.invoke('fs:subOutput', base, name),
  listOutput: (dir: string): Promise<unknown> => ipcRenderer.invoke('fs:listOutput', dir),
  readPlots: (dir: string, names: string[]): Promise<unknown> => ipcRenderer.invoke('plots:read', dir, names),
  chooseSavePath: (defaultPath: string, ext: string): Promise<string | null> =>
    ipcRenderer.invoke('dialog:save', defaultPath, ext),
  detect: (args: unknown): Promise<unknown> => ipcRenderer.invoke('engine:detect', args),
  run: (args: unknown): Promise<unknown> => ipcRenderer.invoke('engine:run', args),
  cancel: (): Promise<void> => ipcRenderer.invoke('engine:cancel'),
  answerCorrection: (method: string, force: boolean, runId?: string): Promise<void> =>
    ipcRenderer.invoke('engine:answer-correction', method, force, runId),
  readReport: (dir: string): Promise<unknown> => ipcRenderer.invoke('engine:report', dir),
  openPath: (p: string): Promise<void> => ipcRenderer.invoke('shell:open', p),
  revealPath: (p: string): Promise<void> => ipcRenderer.invoke('shell:reveal', p),
  exportPdf: (htmlPath: string, pdfPath: string): Promise<void> =>
    ipcRenderer.invoke('report:pdf', htmlPath, pdfPath),
  engineVersion: (): Promise<string> => ipcRenderer.invoke('engine:version'),
  credits: (lang: string): Promise<unknown> => ipcRenderer.invoke('engine:credits', lang),
  /** macOS 应用菜单里的"关于"：打开界面自己的版权页 */
  onMenuCredits: (cb: () => void): (() => void) => {
    const h = () => cb()
    ipcRenderer.on('menu:credits', h)
    return () => ipcRenderer.removeListener('menu:credits', h)
  },
  onEvent: (cb: (e: unknown) => void): (() => void) => {
    const h = (_: unknown, payload: unknown) => cb(payload)
    ipcRenderer.on('engine:event', h)
    return () => ipcRenderer.removeListener('engine:event', h)
  },
}

contextBridge.exposeInMainWorld('scqc', api)
export type ScqcHostApi = typeof api
