/**
 * AI Chatbox 插件统一构建脚本
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
const PLUGIN_ID = 'com.bedcode.ai-chatbox'
const RUST_LIB_NAME = 'bedcode_plugin_ai_chatbox'

// 产物目标目录
const RESOURCES_DIR = resolve(ROOT, '../../src-tauri/resources/plugins/desktop', PLUGIN_ID)

function run(cmd, options = {}) {
  console.log(`[build] > ${cmd}`)
  execSync(cmd, { stdio: 'inherit', cwd: ROOT, ...options })
}

function buildFrontend() {
  console.log('\n[build] ====== Building frontend (Vite) ======')
  run('npx vite build')
}

function buildRust() {
  console.log('\n[build] ====== Building Rust backend (WASM) ======')
  // WASI preview2 目标（rustup target add wasm32-wasip2）：
  // - 产物直接是 Component Model 组件（wasm-component-ld 内嵌，无需再经 componentize 编码）
  // - 插件 std::fs 映射到 WASI（宿主 WASI preopen /data 后可直接读写，见 useSelfFileAccess）
  // 既有宿主接口（host_fs/host_db/...）在 wasip2 下同样可用，行为不变
  run(
    'cargo build --target wasm32-wasip2 --no-default-features --features wasm --manifest-path rust/Cargo.toml --release',
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

  // 复制 WASM 模块
  const wasmPath = resolve(ROOT, 'rust/target/wasm32-wasip2/release', `${RUST_LIB_NAME}.wasm`)

  if (!existsSync(wasmPath)) {
    // 尝试 debug 构建
    const debugWasmPath = resolve(ROOT, 'rust/target/wasm32-wasip2/debug', `${RUST_LIB_NAME}.wasm`)
    if (!existsSync(debugWasmPath)) {
      console.error(`[build] ERROR: WASM file not found at ${wasmPath} or ${debugWasmPath}`)
      process.exit(1)
    }
    cpSync(debugWasmPath, resolve(RESOURCES_DIR, `${RUST_LIB_NAME}.wasm`))
    console.log(`[build] Copied WASM (debug): ${RUST_LIB_NAME}.wasm`)
  } else {
    cpSync(wasmPath, resolve(RESOURCES_DIR, `${RUST_LIB_NAME}.wasm`))
    console.log(`[build] Copied WASM (release): ${RUST_LIB_NAME}.wasm`)
  }

  console.log(`[build] Artifacts copied to: ${RESOURCES_DIR}`)
  console.log(`[build]   - index.js`)
  console.log(`[build]   - plugin.json`)
  console.log(`[build]   - ${RUST_LIB_NAME}.wasm`)
}

// ==================== Main ====================

const args = process.argv.slice(2)
const watchMode = args.includes('--watch')
const frontendOnly = args.includes('--frontend-only')
const rustOnly = args.includes('--rust-only')

if (watchMode) {
  // 前端 watch 构建：改源码自动重建 + 复制产物（配合宿主 PluginDevWatcher 触发前端热重载）。
  // vite 子进程 + fs.watch 保持事件循环常驻，Ctrl+C 退出
  startPluginWatch({
    root: ROOT,
    resourcesDir: RESOURCES_DIR,
    wasmFile: `rust/target/wasm32-wasip2/release/${RUST_LIB_NAME}.wasm`,
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
