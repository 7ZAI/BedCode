#!/usr/bin/env node

/**
 * Plugin Build Script (Mobile) — 薄包装
 *
 * 扫描 wasm-apps/ 下所有插件，调用 SDK CLI（bedcode-plugin build）构建，
 * 并将产物复制到 src-tauri/resources/plugins/mobile/{id}/（进 APK 资源）。
 *
 * 共享 rust 插件（package.json 声明 `bedcode.sharedRustManifest`，如
 * file-transfer 单产物）：先构建**共享 crate**（桌面 wasm-app 的 manifest，
 * unknown-unknown + 仓库级组件化工具）到移动端共享 target（`target/wasm-apps`），
 * 再以 --frontend-only 走 CLI（前端 + 清单 + 图标），wasm 由本脚本复制——
 * 双端同一份 crate 源、同一编码路径，产物随各自构建产出。
 *
 * 用法：node scripts/plugin-build.js [--plugin <plugin-id>] [--rust-only]
 *
 * `--rust-only`（dev-run 过期补建快速路径）：只（重）建 wasm——共享 rust 插件
 * 走共享 crate 构建 + wasm 复制；自有 rust/ 插件委派给 SDK CLI 的 --rust-only。
 * 前端 / 清单 / 图标不动（目录全新时调用方应先走全量构建）。
 */

import { execSync } from 'child_process'
import { readdirSync, existsSync, readFileSync, rmSync, copyFileSync } from 'fs'
import { resolve, dirname, join } from 'path'
import { fileURLToPath } from 'url'
import { platform, homedir } from 'os'

import { readSharedRustManifest } from './lib/shared-rust.js'

const __dirname = dirname(fileURLToPath(import.meta.url))
const ROOT = resolve(__dirname, '..')
const REPO_ROOT = resolve(__dirname, '../..')
const IS_WIN = platform() === 'win32'

const args = process.argv.slice(2)
let targetPlugin = null
let rustOnly = false
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--plugin' && args[i + 1]) { targetPlugin = args[i + 1]; i++ }
  else if (args[i] === '--rust-only') rustOnly = true
}

const pluginsDir = resolve(ROOT, 'wasm-apps')
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
      const manifest = readManifest(name)
      return manifest.id === targetPlugin
    })
  : candidates

if (selected.length === 0) {
  console.error(targetPlugin ? `Unknown plugin: ${targetPlugin}` : 'No plugins found in wasm-apps/')
  process.exit(1)
}

const builtIds = []
for (const name of selected) {
  const manifest = readManifest(name)
  builtIds.push(manifest.id)
  const cwd = resolve(pluginsDir, name)
  console.log(`\n=== Plugin Build (Mobile): ${manifest.id} ===\n`)
  const npxCmd = IS_WIN ? 'npx.cmd' : 'npx'

  // 共享 rust（file-transfer 单产物）：先构建共享 crate 产出 wasm；非共享插件
  // 没有这一步，其 rust 构建由 SDK CLI 在本脚本内统一发起
  const sharedRust = readSharedRustManifest(cwd)
  const sharedWasmPath = sharedRust ? buildSharedRust(cwd, manifest, sharedRust) : null

  if (rustOnly) {
    // dev-run 过期补建快速路径：只刷新 wasm（前端 / 清单 / 图标保持既有产物）
    if (sharedWasmPath) {
      copySharedWasm(manifest, sharedWasmPath)
    } else {
      runCli(npxCmd, cwd, manifest.id, ['--rust-only', '--resources-dir', `"${resourcesDir}"`])
    }
  } else {
    // 共享 rust 插件：CLI 只做前端 / 清单 / 图标（--frontend-only），wasm 由
    // 本分支随后的 copySharedWasm 复制（CLI 的 wasm 复制门控在 !frontendOnly）
    const cliArgs = ['--resources-dir', `"${resourcesDir}"`]
    if (sharedRust) cliArgs.push('--frontend-only')
    runCli(npxCmd, cwd, manifest.id, cliArgs)
    if (sharedWasmPath) copySharedWasm(manifest, sharedWasmPath)
  }

  console.log(`\n=== Plugin build complete (Mobile): ${manifest.id} ===\n`)
}

/**
 * 调 SDK CLI 构建（自有 rust 插件）
 *
 * execSync 默认只透传底层错误（无 cwd / 插件名上下文），这里补上插件定位信息，
 * 便于用户在多插件批量构建失败时定位是哪一支构建挂了。
 */
function runCli(npxCmd, cwd, pluginId, extraArgs) {
  try {
    execSync(`${npxCmd} --no-install bedcode-plugin build ${extraArgs.join(' ')}`, {
      cwd,
      stdio: 'inherit',
    })
  } catch (e) {
    throw new Error(`[plugin-build] 插件 ${pluginId}（${cwd}）构建失败: ${e.message}`)
  }
}

/**
 * 复制共享 crate 编出的 wasm 到宿主资源目录（按插件 id 建子目录）
 *
 * wasmHash 不注入：内置 APK 资源不走分发完整性校验，与既有移动构建一致。
 */
function copySharedWasm(manifest, wasmPath) {
  const dest = join(resourcesDir, manifest.id, `${manifest.rustLibrary}.wasm`)
  copyFileSync(wasmPath, dest)
  console.log(`[plugin-build] 共享 wasm 已复制: ${dest}`)
}

/**
 * 构建共享 rust crate（file-transfer 单产物）并组件化
 *
 * - manifest 路径相对**插件目录**解析（package.json 声明值）；
 * - target-dir 落**移动端共享 target**（`bedcode-mobile/target/wasm-apps`，与桌面
 *   `bedcode-desktop/target/wasm-apps` 同构）——**不得**用插件目录下的
 *   `rust/target/`：那会重建已退役的 `wasm-apps/file-transfer/rust/` 目录，
 *   触发 wiring_lock 的「移动端产物源不得重建」判据；
 * - 组件化工具 = 仓库级 `packages/bedcode-componentize`（双端同一编码路径）。
 * 返回组件化后的 wasm 路径。
 */
function buildSharedRust(cwd, manifest, sharedRustManifest) {
  if (!manifest.rustLibrary) {
    throw new Error(`[plugin-build] ${manifest.id} 声明了 sharedRustManifest 但缺 rustLibrary`)
  }
  const manifestPath = resolve(cwd, sharedRustManifest)
  if (!existsSync(manifestPath)) {
    throw new Error(`[plugin-build] 共享 rust manifest 不存在: ${manifestPath}`)
  }
  const targetDir = resolve(ROOT, 'target/wasm-apps')
  const wasmPath = join(targetDir, 'wasm32-unknown-unknown/release', `${manifest.rustLibrary}.wasm`)

  console.log(`[plugin-build] 构建共享 rust: ${manifestPath}`)
  execSync(
    'cargo build --target wasm32-unknown-unknown --no-default-features --features wasm ' +
      `--manifest-path "${manifestPath}" --release --target-dir "${targetDir}"`,
    { cwd, stdio: 'inherit' },
  )
  if (!existsSync(wasmPath)) {
    throw new Error(`[plugin-build] 共享 rust 构建未产出 ${wasmPath}`)
  }

  // 组件化（core module → Component；工具幂等，已是组件时直通）
  const componentizeManifest = resolve(REPO_ROOT, 'packages/bedcode-componentize/Cargo.toml')
  console.log('[plugin-build] 组件化共享 wasm (Component Model)')
  execSync(
    `cargo run --release --manifest-path "${componentizeManifest}" -- "${wasmPath}" -o "${wasmPath}"`,
    { cwd, stdio: 'inherit' },
  )
  return wasmPath
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

/**
 * 读取插件 manifest（含错误上下文：什么插件的 plugin.json 解析失败）。
 * 无效 JSON 让 process 直接退出（脚本语义上等同于缺 manifest，不应继续构建）。
 */
function readManifest(name) {
  const path = resolve(pluginsDir, name, 'plugin.json')
  try {
    return JSON.parse(readFileSync(path, 'utf-8'))
  } catch (e) {
    console.error(`[plugin-build] 解析插件 manifest 失败（${name} @ ${path}）: ${e.message}`)
    process.exit(1)
  }
}
