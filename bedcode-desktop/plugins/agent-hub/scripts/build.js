/**
 * Agent Hub 插件统一构建脚本
 *
 * 串联：vite build → cargo build (WASM) → 复制产物到 resources 目录
 * （与 file-transfer 同构，仅插件 ID / crate 名不同）
 */

import { execSync } from 'child_process'
import { cpSync, mkdirSync, existsSync, rmSync } from 'fs'
import { resolve, dirname } from 'path'
import { fileURLToPath } from 'url'
import { startPluginWatch } from '../../../scripts/plugin-watch.js'
import { WASM_TARGET, wasip3CargoEnv } from '../../../../scripts/plugin-wasm-config.mjs'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const ROOT = resolve(__dirname, '..')
const PLUGIN_ID = 'com.bedcode.agent-hub'
const RUST_LIB_NAME = 'bedcode_plugin_agent_hub'

// 插件调试模式：BEDCODE_PLUGIN_DEBUG=1 → wasm 以 debug profile 构建（保留
// DWARF，宿主开启 backtrace 行号栈用）；release 构建忽略
const DEBUG_MODE = !!process.env.BEDCODE_PLUGIN_DEBUG
const WASM_PROFILE = DEBUG_MODE ? 'debug' : 'release'
const WASM_PROFILE_DIR = `rust/target/${WASM_TARGET}/${WASM_PROFILE}`

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
  // 票 03：桌面插件统一 wasm32-wasip3（pinned nightly 提供 std；产物 cdylib 直出
  // Component，免 componentize 编码步骤）。wasip3 实例化需宿主 async store（票 02）。
  run(
    `cargo build --target ${WASM_TARGET} --no-default-features --features wasm --manifest-path rust/Cargo.toml${DEBUG_MODE ? '' : ' --release'}`,
    { env: wasip3CargoEnv() },
  )
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

  const iconSrc = resolve(ROOT, 'icon.svg')
  if (existsSync(iconSrc)) {
    cpSync(iconSrc, resolve(RESOURCES_DIR, 'icon.svg'))
  }

  const wasmPath = resolve(ROOT, WASM_PROFILE_DIR, `${RUST_LIB_NAME}.wasm`)
  if (existsSync(wasmPath)) {
    cpSync(wasmPath, resolve(RESOURCES_DIR, `${RUST_LIB_NAME}.wasm`))
    console.log(`[build] Copied WASM (${WASM_PROFILE}): ${RUST_LIB_NAME}.wasm`)
  } else {
    const fallbackProfile = DEBUG_MODE ? 'release' : 'debug'
    const fallbackWasmPath = resolve(
      ROOT,
      `rust/target/${WASM_TARGET}/${fallbackProfile}`,
      `${RUST_LIB_NAME}.wasm`,
    )
    if (!existsSync(fallbackWasmPath)) {
      console.error(`[build] ERROR: WASM file not found at ${wasmPath} or ${fallbackWasmPath}`)
      process.exit(1)
    }
    cpSync(fallbackWasmPath, resolve(RESOURCES_DIR, `${RUST_LIB_NAME}.wasm`))
    console.log(`[build] Copied WASM (${fallbackProfile} fallback): ${RUST_LIB_NAME}.wasm`)
  }

  console.log(`[build] Artifacts copied to: ${RESOURCES_DIR}`)
}

// ==================== Main ====================

const args = process.argv.slice(2)
const watchMode = args.includes('--watch')
const frontendOnly = args.includes('--frontend-only')
const rustOnly = args.includes('--rust-only')

if (watchMode) {
  startPluginWatch({
    root: ROOT,
    resourcesDir: RESOURCES_DIR,
    extraFiles: ['icon.svg'],
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
