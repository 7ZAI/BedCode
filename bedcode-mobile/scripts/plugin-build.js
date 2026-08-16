#!/usr/bin/env node

/**
 * Plugin Build Script (Mobile) — 薄包装
 *
 * 扫描 plugins/ 下所有插件，调用 SDK CLI（bedcode-plugin build）构建，
 * 并将产物复制到 src-tauri/resources/plugins/mobile/{id}/（进 APK 资源）。
 *
 * 用法：node scripts/plugin-build.js [--plugin <plugin-id>]
 */

import { execSync } from 'child_process'
import { readdirSync, existsSync, readFileSync, rmSync } from 'fs'
import { resolve, dirname, join } from 'path'
import { fileURLToPath } from 'url'
import { platform, homedir } from 'os'

const __dirname = dirname(fileURLToPath(import.meta.url))
const ROOT = resolve(__dirname, '..')
const IS_WIN = platform() === 'win32'

const args = process.argv.slice(2)
let targetPlugin = null
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--plugin' && args[i + 1]) { targetPlugin = args[i + 1]; i++ }
}

const pluginsDir = resolve(ROOT, 'plugins')
const resourcesDir = resolve(ROOT, 'src-tauri/resources/plugins/mobile')

// 暂停开发的插件：不参与构建，恢复开发时从列表移除
const EXCLUDED_PLUGINS = []

// 扫描插件目录（跳过模板/隐藏目录/暂停开发的插件，需含 plugin.json）
const candidates = readdirSync(pluginsDir, { withFileTypes: true })
  .filter((d) => d.isDirectory() && !d.name.startsWith('_') && !d.name.startsWith('.'))
  .map((d) => d.name)
  .filter((name) => !EXCLUDED_PLUGINS.includes(name))
  .filter((name) => existsSync(resolve(pluginsDir, name, 'plugin.json')))

const selected = targetPlugin
  ? candidates.filter((name) => {
      const manifest = JSON.parse(readFileSync(resolve(pluginsDir, name, 'plugin.json'), 'utf-8'))
      return manifest.id === targetPlugin
    })
  : candidates

if (selected.length === 0) {
  console.error(targetPlugin ? `Unknown plugin: ${targetPlugin}` : 'No plugins found in plugins/')
  process.exit(1)
}

const builtIds = []
for (const name of selected) {
  const manifest = JSON.parse(readFileSync(resolve(pluginsDir, name, 'plugin.json'), 'utf-8'))
  builtIds.push(manifest.id)
  const cwd = resolve(pluginsDir, name)
  console.log(`\n=== Plugin Build (Mobile): ${manifest.id} ===\n`)
  const npxCmd = IS_WIN ? 'npx.cmd' : 'npx'
  execSync(
    `${npxCmd} --no-install bedcode-plugin build --resources-dir "${resourcesDir}"`,
    { cwd, stdio: 'inherit' },
  )
  console.log(`\n=== Plugin build complete (Mobile): ${manifest.id} ===\n`)
}

// 构建产物已写入 src-tauri/resources/plugins/mobile（APK 资源源），
// 但宿主 dev（tauri:dev 桌面窗口 / tauri:android:dev）启动时按
// `.bedcode-source` 标记 `apk-asset:{appVersion}` 跳过已复制/已解压目录：
// 插件重构建而应用版本未变时，dev 环境会一直加载旧产物。
// 此处刷新本地宿主 dev 运行副本，使下次启动即复制最新产物。
refreshMobileDevCopies(builtIds)

// ==================== 宿主 dev 运行副本刷新 ====================

/**
 * 删除宿主本地 dev 运行副本中的 apk-asset 来源内置插件目录
 *
 * 目标目录为宿主 app_data_dir/plugins/{id}（桌面 dev 窗口经 dev_copy_plugins
 * 从 src-tauri/resources/plugins/mobile 复制而来）。仅删除 `.bedcode-source`
 * 标记以 `apk-asset:` 开头（内置来源）的目录，保留 file-install /
 * remote-download 来源的用户安装/下载插件；目录不存在（从未以桌面 dev 运行）时静默跳过。
 * Android 设备上的副本无法从构建脚本触及，见 PluginAssetExtractor.kt（debug 始终重解压）。
 */
function refreshMobileDevCopies(builtIds) {
  const appDataDir = resolveAppDataDir()
  if (!appDataDir) {
    console.warn('[plugin-build] 无法定位宿主 app_data_dir，跳过 dev 副本刷新')
    return
  }
  const pluginsRoot = join(appDataDir, 'plugins')
  if (!existsSync(pluginsRoot)) {
    console.log('[plugin-build] 宿主本地 dev 运行副本不存在，跳过刷新')
    return
  }
  let removed = 0
  for (const id of builtIds) {
    const dir = join(pluginsRoot, id)
    if (!existsSync(dir)) continue
    let source = ''
    try {
      source = readFileSync(join(dir, '.bedcode-source'), 'utf-8').trim()
    } catch {
      // 无标记（历史产物）按内置处理，同样需要刷新
    }
    if (source && !source.startsWith('apk-asset:')) continue
    try {
      rmSync(dir, { recursive: true, force: true })
      removed++
      console.log(`[plugin-build] 已清除宿主 dev 旧副本: ${id}（下次启动重新复制最新产物）`)
    } catch (e) {
      console.warn(`[plugin-build] 清除 ${id} dev 副本失败（宿主 dev 可能正在运行）: ${e.message}`)
    }
  }
  console.log(
    removed === 0
      ? '[plugin-build] 宿主 dev 无待刷新副本'
      : `[plugin-build] 宿主 dev 副本刷新完成，共清除 ${removed} 个`,
  )
}

/**
 * 宿主 app_data_dir（与 tauri app_data_dir() 一致）
 *
 * Windows: %APPDATA%/{identifier}；macOS: ~/Library/Application Support/{identifier}；
 * Linux: $XDG_DATA_HOME 或 ~/.local/share/{identifier}。identifier 读自 tauri.conf.json。
 */
function resolveAppDataDir() {
  let base
  if (platform() === 'win32') base = process.env.APPDATA
  else if (platform() === 'darwin') base = join(homedir(), 'Library', 'Application Support')
  else base = process.env.XDG_DATA_HOME || join(homedir(), '.local', 'share')
  if (!base) return null
  try {
    const conf = JSON.parse(readFileSync(join(ROOT, 'src-tauri', 'tauri.conf.json'), 'utf-8'))
    return conf.identifier ? join(base, conf.identifier) : null
  } catch {
    return null
  }
}
