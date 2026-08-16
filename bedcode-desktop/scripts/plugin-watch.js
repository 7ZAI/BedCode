/**
 * 插件前端 watch 构建（共享模块）
 *
 * 供各插件 scripts/build.js 调用：以 `vite build --watch` 长驻构建前端，
 * 每次重建完成后把产物复制到宿主资源目录（src-tauri/resources/plugins/desktop/<id>/）。
 *
 * 与宿主侧 PluginDevWatcher 配合形成完整热更新闭环：
 *   改插件源码 → vite 重建 dist/index.js → 复制到产物目录
 *   → watcher 检测 .js 变化 → emit plugin:dev-reload → 前端 reloadPlugin 重新加载
 *
 * 用法：
 *   import { startPluginWatch } from '../../scripts/plugin-watch.js'
 *   startPluginWatch({ root: ROOT, resourcesDir: RESOURCES_DIR, extraFiles: [...] })
 *
 * 只 watch 前端：WASM 产物不参与（Rust 改动仍需一次性 `node scripts/build.js` 全量构建）。
 */
import { spawn } from 'node:child_process'
import { cpSync, existsSync, mkdirSync, watch } from 'node:fs'
import { basename, resolve } from 'node:path'

/** dist 写入事件防抖间隔（vite 重建会连续触发多次文件事件） */
const COPY_DEBOUNCE_MS = 500

/**
 * 定位 vite 可执行文件：npm workspace 会将依赖 hoist 到仓库根 node_modules，
 * 因此从插件目录向上逐级查找，直到仓库根（含根自身）
 */
function findViteBin(startDir) {
  let dir = startDir
  for (let i = 0; i < 6; i++) {
    const candidate = resolve(dir, 'node_modules/vite/bin/vite.js')
    if (existsSync(candidate)) return candidate
    const parent = resolve(dir, '..')
    if (parent === dir) break
    dir = parent
  }
  return null
}

/**
 * 启动前端 watch 构建并自动复制产物
 *
 * @param {object} opts
 * @param {string} opts.root 插件工程根目录（含 package.json / dist / node_modules）
 * @param {string} opts.resourcesDir 宿主资源目标目录（如 src-tauri/resources/plugins/desktop/<id>）
 * @param {string[]} [opts.extraFiles] 需要随前端一并复制的静态附加产物（相对 root，如 hook 脚本）
 * @param {string} [opts.wasmFile] WASM 产物路径（相对 root，如
 *   rust/target/.../lib.wasm）；提供时启动前检查目标 resources 目录是否已有
 *   同名 wasm，缺失则醒目提示先跑一次全量构建（watch 只建前端，首次全新
 *   目录下不提示会导致插件加载失败且无从排查）
 */
export function startPluginWatch({ root, resourcesDir, extraFiles = [], wasmFile }) {
  const distDir = resolve(root, 'dist')
  const viteBin = findViteBin(root)
  const distMain = resolve(distDir, 'index.js')

  if (!viteBin) {
    console.error(`[watch] vite 未找到（自 ${root} 向上查找 node_modules/vite 均无）— 先在插件目录或仓库根运行 npm install`)
    process.exit(1)
  }

  // WASM 缺失预检：全新 resources 目录（从未跑过全量 build）时，产物复制完成后
  // 插件加载仍会因缺 .wasm 失败，此处提前给出可执行指引
  if (wasmFile) {
    const wasmDest = resolve(resourcesDir, basename(wasmFile))
    if (!existsSync(wasmDest)) {
      console.warn(`[watch] ⚠ 目标目录缺少 ${basename(wasmFile)}（${wasmDest}）`)
      console.warn(`[watch]   watch 只重建前端，不参与 WASM。首次请先执行一次全量构建：`)
      console.warn(`[watch]   cd ${root} && node scripts/build.js（含 cargo wasm32 构建）`)
    }
  }

  console.log('\n[watch] ====== 前端 watch 构建启动（vite build --watch） ======')
  console.log(`[watch] vite: ${viteBin}`)
  console.log(`[watch] 产物目录: ${resourcesDir}`)
  console.log('[watch] 修改插件前端源码后自动重建 + 复制，宿主 watcher 会触发前端热重载')
  console.log('[watch] Ctrl+C 退出\n')

  // 长驻 vite watch 构建（vite 5 CLI 支持 build --watch，首次启动即完整构建一次）
  const child = spawn(process.execPath, [viteBin, 'build', '--watch'], {
    cwd: root,
    stdio: 'inherit',
  })
  child.on('exit', (code) => process.exit(code ?? 0))

  // 复制产物到宿主资源目录（覆盖式，不删目录：宿主运行时可能正持有文件句柄）
  const copy = () => {
    try {
      if (!existsSync(distMain)) return
      mkdirSync(resourcesDir, { recursive: true })
      cpSync(distMain, resolve(resourcesDir, 'index.js'))
      cpSync(resolve(root, 'plugin.json'), resolve(resourcesDir, 'plugin.json'))
      for (const f of extraFiles) {
        const src = resolve(root, f)
        if (existsSync(src)) cpSync(src, resolve(resourcesDir, basename(f)))
      }
      console.log(`[watch] ${new Date().toLocaleTimeString()} 产物已复制 → ${resourcesDir}`)
    } catch (e) {
      console.error(`[watch] 复制失败: ${e.message}`)
    }
  }

  // dist 任意文件事件都防抖复制（Windows 上 fs.watch 的 filename 常为 null，
  // 且 vite 写入伴随临时文件/rename 多次事件，按时间聚合最稳）
  let timer = null
  const schedule = () => {
    clearTimeout(timer)
    timer = setTimeout(copy, COPY_DEBOUNCE_MS)
  }

  if (!existsSync(distDir)) mkdirSync(distDir, { recursive: true })
  const watcher = watch(distDir, { persistent: true }, schedule)
  watcher.on('error', (e) => console.error(`[watch] dist 监听错误: ${e.message}`))
}
