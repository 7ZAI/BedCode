/**
 * Terminal Session Center 插件统一构建脚本（rust-ts）
 *
 * 串联：vite build（前端）→ cargo build（WASM wasip3）→ 复制产物到内置资源目录。
 *
 * 票 03 说明：桌面插件统一 wasm32-wasip3（pinned nightly 提供 std；cdylib 直出
 * Component，免 componentize 编码步骤），构建链配置取自仓库单一真源
 * `scripts/plugin-wasm-config.mjs`，本脚本不自带 target / toolchain 字面量。
 * wasip3 实例化需宿主 async store（阶段 2 票 02 已落）。
 *
 * 用法：node scripts/build.js [--watch | --frontend-only | --rust-only]
 */

import { execSync } from 'child_process'
import { cpSync, mkdirSync, existsSync, rmSync } from 'fs'
import { resolve, dirname, basename } from 'path'
import { fileURLToPath } from 'url'
import { startPluginWatch } from '../../../scripts/plugin-watch.js'
import { WASM_TARGET, wasip3CargoEnv } from '../../../../scripts/plugin-wasm-config.mjs'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const ROOT = resolve(__dirname, '..')
const PLUGIN_ID = 'com.bedcode.session'
const RUST_LIB_NAME = 'bedcode_plugin_session'

// 插件调试模式：BEDCODE_PLUGIN_DEBUG=1 → wasm 以 debug profile 构建（保留 DWARF，
// 宿主开启 backtrace 行号栈用）；release 构建忽略（宿主侧以 cfg!(debug_assertions)
// 兜底，见 wasm_runtime.rs plugin_debug_mode）
const DEBUG_MODE = !!process.env.BEDCODE_PLUGIN_DEBUG
const WASM_PROFILE = DEBUG_MODE ? 'debug' : 'release'
const WASM_PROFILE_DIR = `rust/target/${WASM_TARGET}/${WASM_PROFILE}`

// 产物目标目录（内置资源，宿主按 resources/plugins/desktop/<id>/ 扫描加载）
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
    `cargo build --target ${WASM_TARGET} --no-default-features --features wasm --manifest-path rust/Cargo.toml${DEBUG_MODE ? '' : ' --release'}`,
    { env: wasip3CargoEnv() },
  )
}

/** 随包 Agent hook 脚本（部署到用户项目里的集成入口，hooks.rs 按名取用） */
const HOOK_SCRIPTS = [
  'auto_task_hook.py',
  'codex_task_hook.py',
  'pi_task_hook.ts',
  'opencode_task_hook.ts',
]

/** 复制存在的可选随包资源（缺失即跳过）；flatten 时压平一级路径，落资源目录根 */
function copyOptionalFiles(files, flatten = false) {
  for (const rel of files) {
    const src = resolve(ROOT, rel)
    if (!existsSync(src)) continue
    const target = flatten ? basename(rel) : rel
    cpSync(src, resolve(RESOURCES_DIR, target))
    console.log(`[build] Copied ${rel}${flatten ? ` → ${target}` : ''}`)
  }
}

function copyArtifacts() {
  console.log('\n[build] ====== Copying artifacts ======')

  if (existsSync(RESOURCES_DIR)) {
    rmSync(RESOURCES_DIR, { recursive: true })
  }
  mkdirSync(RESOURCES_DIR, { recursive: true })

  const entryJs = resolve(ROOT, 'dist/index.js')
  if (!existsSync(entryJs)) {
    console.error(`[build] ERROR: 前端产物缺失 ${entryJs}（先跑 build:frontend）`)
    process.exit(1)
  }
  cpSync(entryJs, resolve(RESOURCES_DIR, 'index.js'))
  cpSync(resolve(ROOT, 'plugin.json'), resolve(RESOURCES_DIR, 'plugin.json'))

  // 随包资源：图标（PluginIcon.vue 经 asset protocol 读取）
  copyOptionalFiles(['icon.svg'])

  // Agent hook 脚本（票 16）：会话注入时 hooks.rs 按 `<resource_dir>/<脚本名>` 取，
  // 即**安装目录根**（不是 scripts/ 子目录），故复制时压平一级路径。
  // hook 与插件版本同进退（spec D1）：脚本内容改了就必须随包重出，否则项目里部署
  // 的副本指向已不存在的端点。
  copyOptionalFiles(HOOK_SCRIPTS.map((name) => `scripts/${name}`), true)

  // WASM 模块（按 profile 取产物；缺失时回退另一 profile）
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
      console.error(`[build] ERROR: WASM not found at ${wasmPath} or ${fallbackWasmPath}`)
      process.exit(1)
    }
    cpSync(fallbackWasmPath, resolve(RESOURCES_DIR, `${RUST_LIB_NAME}.wasm`))
    console.log(`[build] Copied WASM (${fallbackProfile} fallback): ${RUST_LIB_NAME}.wasm`)
  }

  console.log(`[build] Artifacts copied to: ${RESOURCES_DIR}`)
  for (const name of HOOK_SCRIPTS) {
    console.log(`[build]   - ${name}${existsSync(resolve(RESOURCES_DIR, name)) ? '' : ' (缺失！检查 scripts/)'}`)
  }
}

// ==================== Main ====================

const args = process.argv.slice(2)
const watchMode = args.includes('--watch')
const frontendOnly = args.includes('--frontend-only')
const rustOnly = args.includes('--rust-only')

if (watchMode) {
  // 前端 watch：改源码自动重建 + 复制产物（配合宿主 PluginDevWatcher 热重载）。
  // hook 脚本是静态文件，随每次重建一并刷新（extraFiles 相对插件根）
  startPluginWatch({
    root: ROOT,
    resourcesDir: RESOURCES_DIR,
    extraFiles: ['icon.svg', ...HOOK_SCRIPTS.map((name) => `scripts/${name}`)],
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
