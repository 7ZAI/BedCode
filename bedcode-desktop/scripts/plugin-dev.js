#!/usr/bin/env node

/**
 * Plugin Dev Script
 *
 * 开发模式统一构建脚本，委托给各插件的构建系统
 *
 * 用法：node scripts/plugin-dev.js [--plugin <plugin-id>]
 * 默认构建 auto-task 插件
 */

import { spawn } from 'child_process'
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
let targetPlugin = 'com.bedcode.auto-task'
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

// 委托给插件的构建脚本
const pluginDir = resolve(ROOT, config.pluginDir)
console.log(`Running plugin dev build in: ${pluginDir}`)

const proc = spawn(IS_WIN ? 'npm.cmd' : 'npm', ['run', 'dev'], {
  cwd: pluginDir,
  stdio: 'inherit',
  env: { ...process.env },
})

proc.on('error', (err) => {
  console.error('Plugin dev process error:', err)
})

proc.on('close', (code) => {
  console.log(`Plugin dev process exited with code ${code}`)
  process.exit(code || 0)
})

process.on('SIGINT', () => {
  console.log('\nShutting down plugin dev...')
  proc.kill('SIGINT')
  process.exit(0)
})

process.on('SIGTERM', () => {
  proc.kill('SIGTERM')
  process.exit(0)
})
