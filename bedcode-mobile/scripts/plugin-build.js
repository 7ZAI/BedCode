#!/usr/bin/env node

/**
 * Plugin Build Script (Mobile) — 薄包装
 *
 * 扫描 plugins/ 下所有插件，调用 SDK CLI（bedcode-plugin build）构建，
 * 并将产物复制到 src-tauri/resources/plugins/mobile/{id}/（进 APK 资源）。
 *
 * 用法：node scripts/plugin-build.js [--plugin <plugin-id>]
 */

import { execSync } from 'child_process'
import { readdirSync, existsSync, readFileSync } from 'fs'
import { resolve, dirname } from 'path'
import { fileURLToPath } from 'url'
import { platform } from 'os'

const __dirname = dirname(fileURLToPath(import.meta.url))
const ROOT = resolve(__dirname, '..')
const IS_WIN = platform() === 'win32'

const args = process.argv.slice(2)
let targetPlugin = null
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--plugin' && args[i + 1]) { targetPlugin = args[i + 1]; i++ }
}

const pluginsDir = resolve(ROOT, 'plugins')
const resourcesDir = resolve(ROOT, 'src-tauri/resources/plugins/mobile')

// 暂停开发的插件：不参与构建，恢复开发时从列表移除
const EXCLUDED_PLUGINS = ['ai-chatbox']

// 扫描插件目录（跳过模板/隐藏目录/暂停开发的插件，需含 plugin.json）
const candidates = readdirSync(pluginsDir, { withFileTypes: true })
  .filter((d) => d.isDirectory() && !d.name.startsWith('_') && !d.name.startsWith('.'))
  .map((d) => d.name)
  .filter((name) => !EXCLUDED_PLUGINS.includes(name))
  .filter((name) => existsSync(resolve(pluginsDir, name, 'plugin.json')))

const selected = targetPlugin
  ? candidates.filter((name) => {
      const manifest = JSON.parse(readFileSync(resolve(pluginsDir, name, 'plugin.json'), 'utf-8'))
      return manifest.id === targetPlugin
    })
  : candidates

if (selected.length === 0) {
  console.error(targetPlugin ? `Unknown plugin: ${targetPlugin}` : 'No plugins found in plugins/')
  process.exit(1)
}

for (const name of selected) {
  const manifest = JSON.parse(readFileSync(resolve(pluginsDir, name, 'plugin.json'), 'utf-8'))
  const cwd = resolve(pluginsDir, name)
  console.log(`\n=== Plugin Build (Mobile): ${manifest.id} ===\n`)
  const npxCmd = IS_WIN ? 'npx.cmd' : 'npx'
  execSync(
    `${npxCmd} --no-install bedcode-plugin build --resources-dir "${resourcesDir}"`,
    { cwd, stdio: 'inherit' },
  )
  console.log(`\n=== Plugin build complete (Mobile): ${manifest.id} ===\n`)
}
