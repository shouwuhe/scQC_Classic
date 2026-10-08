// 用 esbuild 打包主进程与 preload（CommonJS，Electron 直接加载）
import { build } from 'esbuild'

const common = {
  bundle: true,
  platform: 'node',
  target: 'node20',
  format: 'cjs',
  sourcemap: false,
  external: ['electron'],
  logLevel: 'info',
}

await build({ ...common, entryPoints: ['electron/main/index.ts'], outfile: 'dist-electron/main/index.cjs' })
await build({ ...common, entryPoints: ['electron/preload/index.ts'], outfile: 'dist-electron/preload/index.cjs' })
