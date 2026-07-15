/**
 * AI Chatbox 插件统一构建脚本 (Mobile)
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
const PLUGIN_ID = 'com.bedcode.ai-chatbox'
const RUST_LIB_NAME = 'bedcode_plugin_ai_chatbox'

const RESOURCES_DIR = resolve(ROOT, '../../src-tauri/resources/plugins/mobile', PLUGIN_ID)

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

  if (existsSync(RESOURCES_DIR)) {
    rmSync(RESOURCES_DIR, { recursive: true })
  }
  mkdirSync(RESOURCES_DIR, { recursive: true })

  const distDir = resolve(ROOT, 'dist')
  cpSync(resolve(distDir, 'index.js'), resolve(RESOURCES_DIR, 'index.js'))
  cpSync(resolve(ROOT, 'plugin.json'), resolve(RESOURCES_DIR, 'plugin.json'))

  const wasmPath = resolve(ROOT, 'rust/target/wasm32-unknown-unknown/release', `${RUST_LIB_NAME}.wasm`)

  if (!existsSync(wasmPath)) {
    const debugWasmPath = resolve(ROOT, 'rust/target/wasm32-unknown-unknown/debug', `${RUST_LIB_NAME}.wasm`)
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
}

const args = process.argv.slice(2)
const frontendOnly = args.includes('--frontend-only')
const rustOnly = args.includes('--rust-only')

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
