#!/usr/bin/env node

/**
 * Plugin Build Script
 *
 * 生产构建脚本：编译 Rust cdylib（release）+ TS 前端，复制产物到 resources 目录
 *
 * 用法：node scripts/plugin-build.js [--plugin <plugin-id>]
 * 默认构建 ai-chatbox 插件
 */

import { execSync } from 'child_process'
import { resolve, join } from 'path'
import { copyFileSync, mkdirSync, existsSync, readdirSync } from 'fs'
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

console.log(`\n=== Plugin Build: ${targetPlugin} ===\n`)

// ==================== Step 1: Cargo build cdylib (release) ====================

const rustProjectDir = resolve(ROOT, `src-tauri/plugins/${config.rustProject}`)
if (!existsSync(rustProjectDir)) {
  console.error(`Rust project not found: ${rustProjectDir}`)
  process.exit(1)
}

console.log('[1/3] Building Rust cdylib (release)...')
try {
  execSync(`cargo build --release --manifest-path "${resolve(rustProjectDir, 'Cargo.toml')}"`, {
    stdio: 'inherit',
    env: { ...process.env },
  })
} catch (e) {
  console.error('Rust build failed!')
  process.exit(1)
}

// ==================== Step 2: Copy DLL to resources ====================

const targetDir = resolve(rustProjectDir, 'target/release')
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

// ==================== Step 3: Vite build TS ====================

const tsDir = resolve(ROOT, config.tsDir)
console.log('[3/3] Building TS frontend...')

try {
  execSync(IS_WIN ? 'npx.cmd' : 'npx', ['vite', 'build'], {
    cwd: tsDir,
    stdio: 'inherit',
    env: { ...process.env },
  })
} catch (e) {
  console.error('TS build failed!')
  process.exit(1)
}

console.log(`\n=== Plugin build complete: ${targetPlugin} ===\n`)

// ==================== Helpers ====================

function findDll(dir, projectName) {
  if (!existsSync(dir)) return null

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

  try {
    for (const file of readdirSync(dir)) {
      if (IS_WIN && file.endsWith('.dll') && file.includes(crateName)) return file
      if (platform() === 'darwin' && file.endsWith('.dylib') && file.includes(crateName)) return file
      if (!IS_WIN && platform() !== 'darwin' && file.endsWith('.so') && file.includes(crateName)) return file
    }
  } catch { /* ignore */ }

  return null
}
