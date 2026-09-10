/**
 * Auto Task 插件统一构建脚本
 *
 * 串联：vite build → cargo build (WASM) → 复制产物到 resources 目录
 */

import { execSync } from 'child_process'
import { cpSync, mkdirSync, existsSync, rmSync } from 'fs'
import { resolve, dirname } from 'path'
import { fileURLToPath } from 'url'
import { startPluginWatch } from '../../../scripts/plugin-watch.js'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const ROOT = resolve(__dirname, '..')
const PLUGIN_ID = 'com.bedcode.auto-task'
const RUST_LIB_NAME = 'bedcode_plugin_auto_task'

// 插件调试模式：BEDCODE_PLUGIN_DEBUG=1 → wasm 以 debug profile 构建（保留
// DWARF，宿主开启 backtrace 行号栈用）；release 构建忽略（宿主侧以
// cfg!(debug_assertions) 兜底，见 wasm_runtime.rs plugin_debug_mode）
const DEBUG_MODE = !!process.env.BEDCODE_PLUGIN_DEBUG
const WASM_PROFILE = DEBUG_MODE ? 'debug' : 'release'
const WASM_PROFILE_DIR = `rust/target/wasm32-unknown-unknown/${WASM_PROFILE}`

// 产物目标目录
const RESOURCES_DIR = resolve(ROOT, '../../src-tauri/resources/plugins/desktop', PLUGIN_ID)

function run(cmd, options = {}) {
  console.log(`[build] > ${cmd}`)
  try {
    execSync(cmd, { stdio: 'inherit', cwd: ROOT, ...options })
  } catch (e) {
    console.error(`[build] 命令失败: ${cmd} (cwd=${ROOT})`)
    throw e
  }
}

function buildFrontend() {
  console.log('\n[build] ====== Building frontend (Vite) ======')
  run('pnpm exec vite build')
}

function buildRust() {
  console.log('\n[build] ====== Building Rust backend (WASM) ======')
  run(
    `cargo build --target wasm32-unknown-unknown --no-default-features --features wasm --manifest-path rust/Cargo.toml${DEBUG_MODE ? '' : ' --release'}`,
  )
  // 迁移阶段 B：将 wit-bindgen 产出的 core module 编码为 Component Model 组件
  // （等价 wasm-tools component new；工具幂等——产物已是组件时直接复制）
  console.log('\n[build] ====== Componentizing WASM (Component Model) ======')
  const componentizeManifest = resolve(
    ROOT,
    '../../packages/plugin-sdk-desktop/rust/tools/componentize/Cargo.toml',
  )
  const wasmPath = resolve(ROOT, WASM_PROFILE_DIR, `${RUST_LIB_NAME}.wasm`)
  run(
    `cargo run --release --manifest-path "${componentizeManifest}" -- "${wasmPath}" -o "${wasmPath}"`,
  )
}

function copyArtifacts() {
  console.log('\n[build] ====== Copying artifacts ======')

  // 清理并创建目标目录
  if (existsSync(RESOURCES_DIR)) {
    rmSync(RESOURCES_DIR, { recursive: true })
  }
  mkdirSync(RESOURCES_DIR, { recursive: true })

  // 复制前端产物
  const distDir = resolve(ROOT, 'dist')
  cpSync(resolve(distDir, 'index.js'), resolve(RESOURCES_DIR, 'index.js'))

  // 复制 plugin.json
  cpSync(resolve(ROOT, 'plugin.json'), resolve(RESOURCES_DIR, 'plugin.json'))

  // 复制插件图标（PluginIcon.vue 经 asset protocol 加载 icon.svg）
  const iconSrc = resolve(ROOT, 'icon.svg')
  if (existsSync(iconSrc)) {
    cpSync(iconSrc, resolve(RESOURCES_DIR, 'icon.svg'))
  }

  // 复制 WASM 模块（按构建 profile 取产物；缺失时回退另一 profile）
  const wasmPath = resolve(ROOT, WASM_PROFILE_DIR, `${RUST_LIB_NAME}.wasm`)

  if (existsSync(wasmPath)) {
    cpSync(wasmPath, resolve(RESOURCES_DIR, `${RUST_LIB_NAME}.wasm`))
    console.log(`[build] Copied WASM (${WASM_PROFILE}): ${RUST_LIB_NAME}.wasm`)
  } else {
    const fallbackProfile = DEBUG_MODE ? 'release' : 'debug'
    const fallbackWasmPath = resolve(
      ROOT,
      `rust/target/wasm32-unknown-unknown/${fallbackProfile}`,
      `${RUST_LIB_NAME}.wasm`,
    )
    if (!existsSync(fallbackWasmPath)) {
      console.error(`[build] ERROR: WASM file not found at ${wasmPath} or ${fallbackWasmPath}`)
      process.exit(1)
    }
    cpSync(fallbackWasmPath, resolve(RESOURCES_DIR, `${RUST_LIB_NAME}.wasm`))
    console.log(`[build] Copied WASM (${fallbackProfile} fallback): ${RUST_LIB_NAME}.wasm`)
  }

  // 复制 auto_task_hook.py
  const hookSource = resolve(ROOT, 'scripts/auto_task_hook.py')
  if (existsSync(hookSource)) {
    cpSync(hookSource, resolve(RESOURCES_DIR, 'auto_task_hook.py'))
    console.log('[build] Copied auto_task_hook.py')
  } else {
    console.warn('[build] WARNING: auto_task_hook.py not found in scripts/')
  }

  // 复制 pi_task_hook.ts（pi 扩展，部署到项目 .pi/extensions/）
  const piHookSource = resolve(ROOT, 'scripts/pi_task_hook.ts')
  if (existsSync(piHookSource)) {
    cpSync(piHookSource, resolve(RESOURCES_DIR, 'pi_task_hook.ts'))
    console.log('[build] Copied pi_task_hook.ts')
  } else {
    console.warn('[build] WARNING: pi_task_hook.ts not found in scripts/')
  }

  // 复制 opencode_task_hook.ts（opencode 插件，部署到项目 .opencode/plugins/）
  const opencodeHookSource = resolve(ROOT, 'scripts/opencode_task_hook.ts')
  if (existsSync(opencodeHookSource)) {
    cpSync(opencodeHookSource, resolve(RESOURCES_DIR, 'opencode_task_hook.ts'))
    console.log('[build] Copied opencode_task_hook.ts')
  } else {
    console.warn('[build] WARNING: opencode_task_hook.ts not found in scripts/')
  }

  // 复制 codex_task_hook.py（Codex hooks，部署到项目 .codex/）
  const codexHookSource = resolve(ROOT, 'scripts/codex_task_hook.py')
  if (existsSync(codexHookSource)) {
    cpSync(codexHookSource, resolve(RESOURCES_DIR, 'codex_task_hook.py'))
    console.log('[build] Copied codex_task_hook.py')
  } else {
    console.warn('[build] WARNING: codex_task_hook.py not found in scripts/')
  }

  console.log(`[build] Artifacts copied to: ${RESOURCES_DIR}`)
  console.log(`[build]   - index.js`)
  console.log(`[build]   - plugin.json`)
  console.log(`[build]   - icon.svg`)
  console.log(`[build]   - ${RUST_LIB_NAME}.wasm`)
  console.log(`[build]   - auto_task_hook.py`)
  console.log(`[build]   - pi_task_hook.ts`)
  console.log(`[build]   - opencode_task_hook.ts`)
  console.log(`[build]   - codex_task_hook.py`)
}

// ==================== Main ====================

const args = process.argv.slice(2)
const watchMode = args.includes('--watch')
const frontendOnly = args.includes('--frontend-only')
const rustOnly = args.includes('--rust-only')

if (watchMode) {
  // 前端 watch 构建：改源码自动重建 + 复制产物（配合宿主 PluginDevWatcher 触发前端热重载）。
  // vite 子进程 + fs.watch 保持事件循环常驻，Ctrl+C 退出；hook 脚本为静态文件，随每次重建一并刷新
  startPluginWatch({
    root: ROOT,
    resourcesDir: RESOURCES_DIR,
    extraFiles: [
      'icon.svg',
      'scripts/auto_task_hook.py',
      'scripts/pi_task_hook.ts',
      'scripts/opencode_task_hook.ts',
      'scripts/codex_task_hook.py',
    ],
    wasmFile: `${WASM_PROFILE_DIR}/${RUST_LIB_NAME}.wasm`,
  })
} else if (frontendOnly) {
  buildFrontend()
  copyArtifacts()
} else if (rustOnly) {
  buildRust()
  copyArtifacts()
} else {
  buildFrontend()
  buildRust()
  copyArtifacts()
}

if (!watchMode) {
  console.log('\n[build] ====== Build complete! ======')
}
