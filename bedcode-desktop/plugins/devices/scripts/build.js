/**
 * Devices 插件统一构建脚本（headless rust-only）
 *
 * 串联：cargo build (WASM wasip3) → 复制产物到 resources 目录。
 * 无前端：rust-only 插件前端 loader 跳过（src/plugin/loader.ts），不产出 dist/。
 *
 * 票 03 说明：桌面插件统一 wasm32-wasip3（pinned nightly 提供 std；产物 cdylib
 * 直出 Component，免 componentize 编码步骤）。wasip3 实例化需宿主 async store。
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
const PLUGIN_ID = 'com.bedcode.devices'
const RUST_LIB_NAME = 'bedcode_plugin_devices'

// 插件调试模式：BEDCODE_PLUGIN_DEBUG=1 → wasm 以 debug profile 构建（保留
// DWARF，宿主开启 backtrace 行号栈用）；release 构建忽略（宿主侧以
// cfg!(debug_assertions) 兜底，见 wasm_runtime.rs plugin_debug_mode）
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

function buildRust() {
  console.log('\n[build] ====== Building Rust backend (WASM) ======')
  run(
    `cargo build --target ${WASM_TARGET} --no-default-features --features wasm --manifest-path rust/Cargo.toml${DEBUG_MODE ? '' : ' --release'}`,
    { env: wasip3CargoEnv() },
  )
}

function copyArtifacts() {
  console.log('\n[build] ====== Copying artifacts ======')

  // 清理并创建目标目录
  if (existsSync(RESOURCES_DIR)) {
    rmSync(RESOURCES_DIR, { recursive: true })
  }
  mkdirSync(RESOURCES_DIR, { recursive: true })

  // 复制 plugin.json
  cpSync(resolve(ROOT, 'plugin.json'), resolve(RESOURCES_DIR, 'plugin.json'))

  // 复制 WASM 模块（按构建 profile 取产物；缺失时回退另一 profile）
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
      throw new Error(`WASM artifact not found: ${wasmPath}`)
    }
    cpSync(fallbackWasmPath, resolve(RESOURCES_DIR, `${RUST_LIB_NAME}.wasm`))
    console.log(`[build] Copied WASM (${fallbackProfile} fallback): ${RUST_LIB_NAME}.wasm`)
  }
}

// 监听模式：rust 源码变更触发增量重构建（debug profile；产物为 debug wasm）
const WATCH_PATTERNS = ['rust/src/**/*.rs', 'plugin.json']

if (process.argv.includes('--watch')) {
  startPluginWatch(WATCH_PATTERNS, () => {
    buildRust()
    copyArtifacts()
  })
} else {
  buildRust()
  copyArtifacts()
  console.log('\n[build] Devices plugin build complete')
}
