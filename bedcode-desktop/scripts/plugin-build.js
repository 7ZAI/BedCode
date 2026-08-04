#!/usr/bin/env node

/**
 * Plugin Build Script
 *
 * 生产构建脚本，委托给各插件的构建系统
 *
 * 用法：node scripts/plugin-build.js [--plugin <plugin-id>]
 * 默认构建 ai-chatbox 插件
 */

import { execSync } from 'child_process'
import { resolve, dirname } from 'path'
import { fileURLToPath } from 'url'
import { platform } from 'os'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const ROOT = resolve(__dirname, '..')
const IS_WIN = platform() === 'win32'

// 插件配置 — 指向合并后的插件工程目录
const PLUGINS = {
  'com.bedcode.ai-chatbox': {
    pluginDir: 'plugins/ai-chatbox',
  },
  'com.bedcode.auto-task': {
    pluginDir: 'plugins/auto-task',
  },
  'com.bedcode.file-transfer': {
    pluginDir: 'plugins/file-transfer',
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

// 委托给插件的构建脚本
const pluginDir = resolve(ROOT, config.pluginDir)
console.log(`Running plugin build in: ${pluginDir}`)

try {
  const npmCmd = IS_WIN ? 'npm.cmd' : 'npm'
  execSync(`${npmCmd} run build`, {
    cwd: pluginDir,
    stdio: 'inherit',
    env: { ...process.env },
  })
} catch (e) {
  console.error('Plugin build failed!')
  process.exit(1)
}

console.log(`\n=== Plugin build complete: ${targetPlugin} ===\n`)
