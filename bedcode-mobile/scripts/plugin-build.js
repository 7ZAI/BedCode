#!/usr/bin/env node

/**
 * Plugin Build Script (Mobile)
 *
 * 用法：node scripts/plugin-build.js [--plugin <plugin-id>]
 */

import { execSync } from 'child_process'
import { resolve, dirname } from 'path'
import { fileURLToPath } from 'url'
import { platform } from 'os'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const ROOT = resolve(__dirname, '..')
const IS_WIN = platform() === 'win32'

const PLUGINS = {
  'com.bedcode.ai-chatbox': { pluginDir: 'plugins/ai-chatbox' },
}

const args = process.argv.slice(2)
let targetPlugin = 'com.bedcode.ai-chatbox'
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--plugin' && args[i + 1]) { targetPlugin = args[i + 1]; i++ }
}

const config = PLUGINS[targetPlugin]
if (!config) {
  console.error(`Unknown plugin: ${targetPlugin}`)
  console.error(`Available: ${Object.keys(PLUGINS).join(', ')}`)
  process.exit(1)
}

console.log(`\n=== Plugin Build (Mobile): ${targetPlugin} ===\n`)

try {
  const npmCmd = IS_WIN ? 'npm.cmd' : 'npm'
  execSync(`${npmCmd} run build`, { cwd: resolve(ROOT, config.pluginDir), stdio: 'inherit', env: { ...process.env } })
} catch (e) {
  console.error('Plugin build failed!')
  process.exit(1)
}

console.log(`\n=== Plugin build complete (Mobile): ${targetPlugin} ===\n`)
