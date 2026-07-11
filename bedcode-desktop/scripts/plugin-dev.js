#!/usr/bin/env node

/**
 * Plugin Dev Script
 *
 * 开发模式统一构建脚本：
 * 1. cargo build 编译 cdylib（debug 模式）
 * 2. 复制 .dll 到 resources 目录
 * 3. vite build --watch 监听 TS 变更自动重编译
 *
 * 用法：node scripts/plugin-dev.js [--plugin <plugin-id>]
 * 默认构建 ai-chatbox 插件
 */

import { execSync, spawn } from 'child_process'
import { resolve, join, basename } from 'path'
import { copyFileSync, mkdirSync, existsSync, readdirSync, statSync } from 'fs'
import { platform } from 'os'

const __dirname = resolve()
const ROOT = resolve(__dirname, '..')
const IS_WIN = platform() === 'win32'

// 插件配置
const PLUGINS = {
  'com.bedcode.ai-chatbox': {
    rustProject: 'ai-chatbox',
    tsDir: 'src/plugins/com.bedcode.ai-chatbox',
    outDir: 'src-tauri/resources/plugins/desktop/com.bedcode.ai-chatbox',
  },
}

// 解析参数
const args = process.argv.slice(2)
let targetPlugin = 'com.bedcode.ai-chatbox'
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--plugin' && args[i + 1]) {
    targetPlugin = args[i + 1]
    i++
  }
}

const config = PLUGINS[targetPlugin]
if (!config) {
  console.error(`Unknown plugin: ${targetPlugin}`)
  console.error(`Available: ${Object.keys(PLUGINS).join(', ')}`)
  process.exit(1)
}

console.log(`\n=== Plugin Dev: ${targetPlugin} ===\n`)

// ==================== Step 1: Cargo build cdylib ====================

const rustProjectDir = resolve(ROOT, `src-tauri/plugins/${config.rustProject}`)
if (!existsSync(rustProjectDir)) {
  console.error(`Rust project not found: ${rustProjectDir}`)
  process.exit(1)
}

console.log('[1/3] Building Rust cdylib (debug)...')
try {
  execSync(`cargo build --manifest-path "${resolve(rustProjectDir, 'Cargo.toml')}"`, {
    stdio: 'inherit',
    env: { ...process.env },
  })
} catch (e) {
  console.error('Rust build failed!')
  process.exit(1)
}

// ==================== Step 2: Copy DLL to resources ====================

// 查找编译产物
const targetDir = resolve(rustProjectDir, 'target/debug')
const dllName = findDll(targetDir, config.rustProject)
if (!dllName) {
  console.error(`Compiled library not found in ${targetDir}`)
  process.exit(1)
}

const outDir = resolve(ROOT, config.outDir)
mkdirSync(outDir, { recursive: true })

const srcDll = resolve(targetDir, dllName)
const dstDll = resolve(outDir, dllName)

copyFileSync(srcDll, dstDll)
console.log(`[2/3] Copied: ${dllName} → ${config.outDir}/`)

// ==================== Step 3: Vite build --watch ====================

const tsDir = resolve(ROOT, config.tsDir)
console.log(`[3/3] Starting vite build --watch for TS...`)

const viteProc = spawn(IS_WIN ? 'npx.cmd' : 'npx', ['vite', 'build', '--watch'], {
  cwd: tsDir,
  stdio: 'inherit',
  env: { ...process.env },
})

viteProc.on('error', (err) => {
  console.error('Vite process error:', err)
})

viteProc.on('close', (code) => {
  console.log(`Vite process exited with code ${code}`)
  process.exit(code || 0)
})

// 优雅退出
process.on('SIGINT', () => {
  console.log('\nShutting down plugin dev...')
  viteProc.kill('SIGINT')
  process.exit(0)
})

process.on('SIGTERM', () => {
  viteProc.kill('SIGTERM')
  process.exit(0)
})

// ==================== Helpers ====================

/** 在 target/debug 目录查找编译产物 */
function findDll(dir, projectName) {
  if (!existsSync(dir)) return null

  // 将 project name 中的 - 转为 _（Rust crate 命名规则）
  const crateName = projectName.replace(/-/g, '_')

  const candidates = []
  if (IS_WIN) {
    candidates.push(`${crateName}.dll`)
  } else if (platform() === 'darwin') {
    candidates.push(`lib${crateName}.dylib`)
  } else {
    candidates.push(`lib${crateName}.so`)
  }

  for (const name of candidates) {
    if (existsSync(resolve(dir, name))) return name
  }

  // 遍历查找（处理可能的 hash 后缀）
  try {
    for (const file of readdirSync(dir)) {
      if (IS_WIN && file.endsWith('.dll') && file.includes(crateName)) return file
      if (platform() === 'darwin' && file.endsWith('.dylib') && file.includes(crateName)) return file
      if (!IS_WIN && platform() !== 'darwin' && file.endsWith('.so') && file.includes(crateName)) return file
    }
  } catch { /* ignore */ }

  return null
}
