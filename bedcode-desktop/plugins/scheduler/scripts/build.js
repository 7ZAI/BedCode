/**
 * Scheduler 插件构建脚本（rust-ts：Rust WASM + 前端只读面板）
 *
 * 串联：vite build（前端）→ cargo build (WASM) → componentize → CLI (bedtask) 构建 →
 * 复制产物（plugin.json + wasm + cli/ + index.js）到 resources 目录
 */

import { execSync } from 'child_process'
import { cpSync, mkdirSync, existsSync, rmSync } from 'fs'
import { resolve, dirname } from 'path'
import { fileURLToPath } from 'url'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const ROOT = resolve(__dirname, '..')
const PLUGIN_ID = 'com.bedcode.scheduler'
const RUST_LIB_NAME = 'bedcode_plugin_scheduler'

// 产物目标目录
const RESOURCES_DIR = resolve(ROOT, '../../src-tauri/resources/plugins/desktop', PLUGIN_ID)

function run(cmd, options = {}) {
  console.log(`[build] > ${cmd}`)
  execSync(cmd, { stdio: 'inherit', cwd: ROOT, ...options })
}

function buildFrontend() {
  console.log('\n[build] ====== Building frontend (Vite) ======')
  run('pnpm exec vite build')
}

function buildRust() {
  console.log('\n[build] ====== Building Rust backend (WASM) ======')
  run(
    'cargo build --target wasm32-unknown-unknown --no-default-features --features wasm --manifest-path rust/Cargo.toml --release',
  )
  // 将 wit-bindgen 产出的 core module 编码为 Component Model 组件
  console.log('\n[build] ====== Componentizing WASM (Component Model) ======')
  const componentizeManifest = resolve(
    ROOT,
    '../../packages/plugin-sdk-desktop/rust/tools/componentize/Cargo.toml',
  )
  const wasmPath = resolve(
    ROOT,
    'rust/target/wasm32-unknown-unknown/release',
    `${RUST_LIB_NAME}.wasm`,
  )
  run(
    `cargo run --release --manifest-path "${componentizeManifest}" -- "${wasmPath}" -o "${wasmPath}"`,
  )
}

function buildCli() {
  console.log('\n[build] ====== Building CLI (bedtask) ======')
  run('cargo build --release --manifest-path cli/Cargo.toml')
}

function copyArtifacts() {
  console.log('\n[build] ====== Copying artifacts ======')

  if (existsSync(RESOURCES_DIR)) {
    rmSync(RESOURCES_DIR, { recursive: true })
  }
  mkdirSync(RESOURCES_DIR, { recursive: true })

  // 复制 plugin.json
  cpSync(resolve(ROOT, 'plugin.json'), resolve(RESOURCES_DIR, 'plugin.json'))

  // 复制前端产物（manifest.main 指向 index.js）
  const distJs = resolve(ROOT, 'dist/index.js')
  if (!existsSync(distJs)) {
    console.error(
      `[build] ERROR: frontend dist not found at ${distJs}（先跑 pnpm install && pnpm run build:frontend）`,
    )
    process.exit(1)
  }
  cpSync(distJs, resolve(RESOURCES_DIR, 'index.js'))
  console.log('[build] Copied frontend: index.js')

  // 复制 WASM 模块
  const wasmPath = resolve(
    ROOT,
    'rust/target/wasm32-unknown-unknown/release',
    `${RUST_LIB_NAME}.wasm`,
  )
  if (!existsSync(wasmPath)) {
    console.error(`[build] ERROR: WASM file not found at ${wasmPath}`)
    process.exit(1)
  }
  cpSync(wasmPath, resolve(RESOURCES_DIR, `${RUST_LIB_NAME}.wasm`))
  console.log(`[build] Copied WASM: ${RUST_LIB_NAME}.wasm`)

  // 复制 CLI（插件包 cli/ 目录，宿主 activate 时安装到用户 bin 目录）
  const exe = process.platform === 'win32' ? 'bedtask.exe' : 'bedtask'
  const cliPath = resolve(ROOT, 'cli/target/release', exe)
  if (!existsSync(cliPath)) {
    console.error(`[build] ERROR: CLI binary not found at ${cliPath}`)
    process.exit(1)
  }
  const cliDir = resolve(RESOURCES_DIR, 'cli')
  mkdirSync(cliDir, { recursive: true })
  cpSync(cliPath, resolve(cliDir, exe))
  console.log(`[build] Copied CLI: ${exe}`)
}

buildFrontend()
buildRust()
buildCli()
copyArtifacts()
console.log('\n[build] Done.')
