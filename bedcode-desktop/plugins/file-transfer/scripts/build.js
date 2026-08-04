/**
 * File Transfer 插件统一构建脚本
 *
 * 串联：vite build → cargo build (WASM) → 复制产物到 resources 目录
 */

import { execSync } from 'child_process'
import { cpSync, mkdirSync, existsSync, rmSync } from 'fs'
import { resolve, dirname } from 'path'
import { fileURLToPath } from 'url'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const ROOT = resolve(__dirname, '..')
const PLUGIN_ID = 'com.bedcode.file-transfer'
const RUST_LIB_NAME = 'bedcode_plugin_file_transfer'

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
  run('cargo build --target wasm32-unknown-unknown --no-default-features --features wasm --manifest-path rust/Cargo.toml --release')
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
  const wasmPath = resolve(
    ROOT,
    'rust/target/wasm32-unknown-unknown/release',
    `${RUST_LIB_NAME}.wasm`
  )

  if (!existsSync(wasmPath)) {
    // 尝试 debug 构建
    const debugWasmPath = resolve(
      ROOT,
      'rust/target/wasm32-unknown-unknown/debug',
      `${RUST_LIB_NAME}.wasm`
    )
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
  console.log('[build] Watch mode not yet implemented, running one-shot build')
}

if (frontendOnly) {
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

console.log('\n[build] ====== Build complete! ======')
