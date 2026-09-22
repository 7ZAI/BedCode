#!/usr/bin/env node

/**
 * Plugin Build Script
 *
 * 生产构建脚本，委托给各插件的构建系统
 *
 * 用法：node scripts/plugin-build.js [--plugin <plugin-id>]
 * 默认构建 com.bedcode.terminal-session（终端会话中心）插件
 */

import { execSync } from 'child_process'
import { resolve, dirname } from 'path'
import { fileURLToPath } from 'url'
import { platform } from 'os'
import { generateManifest } from '../packages/plugin-sdk-desktop/bin/manifest-gen.js'
import { validateManifest } from '../packages/plugin-sdk-desktop/bin/manifest-validate.js'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const ROOT = resolve(__dirname, '..')
const IS_WIN = platform() === 'win32'

// 插件配置 — 指向合并后的插件工程目录
const PLUGINS = {
  'com.bedcode.ai-chatbox': {
    pluginDir: 'plugins/ai-chatbox',
  },
  'com.bedcode.file-transfer': {
    pluginDir: 'plugins/file-transfer',
  },
  'com.bedcode.terminal-session': {
    pluginDir: 'plugins/terminal-session',
  },
}

// 解析参数
const args = process.argv.slice(2)
let targetPlugin = 'com.bedcode.terminal-session'
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

// 插件调试模式提示（BEDCODE_PLUGIN_DEBUG 经 env 透传给插件构建脚本；
// release 构建不应设置该变量，宿主以 cfg!(debug_assertions) 兜底）
if (process.env.BEDCODE_PLUGIN_DEBUG) {
  console.log('[plugin-build] 注意：BEDCODE_PLUGIN_DEBUG 已设置——插件将以 debug profile 构建（仅供调试）')
}

// 委托给插件的构建脚本
const pluginDir = resolve(ROOT, config.pluginDir)
console.log(`Running plugin build in: ${pluginDir}`)

// 构建前：按源码自动填充 plugin.json 的 contributes/permissions
// （与插件源码单一真源约定，保证产物与源码一致——一致的口径是「除构建注入的 wasmHash 外逐字一致」，
//   wasmHash 只存在于产物目录，源清单不带；见 packages/plugin-sdk-desktop/bin/wasm-hash.js）
try {
  const { changed, report } = generateManifest(pluginDir)
  if (changed) {
    console.log('[plugin-build] plugin.json 已根据源码自动填充:')
    for (const line of report) console.log(`  ${line}`)
  } else {
    console.log('[plugin-build] plugin.json 已是最新，无需更新')
  }
} catch (e) {
  console.error(`[plugin-build] manifest 自动填充失败: ${e.message}`)
  process.exit(1)
}

// 构建前：校验 plugin.json（与 CLI `bedcode-plugin-desktop validate` 同一套规则）
// 词汇/结构不合法的清单过去只在人工跑 CLI 时才被发现，声明了不存在的权限位会
// 在宿主授权时被静默过滤（等于没声明），因此挂在构建链上强制拦一次。
let validation
try {
  validation = validateManifest(pluginDir)
} catch (e) {
  console.error(`[plugin-build] manifest 校验不可用: ${e.message}`)
  process.exit(1)
}
for (const w of validation.warnings) console.log(`[plugin-build] manifest ⚠ ${w}`)
if (validation.errors.length) {
  for (const e of validation.errors) console.error(`[plugin-build] manifest ✗ ${e}`)
  console.error(
    `[plugin-build] plugin.json 校验失败: ${validation.errors.length} 个错误（${pluginDir}）`,
  )
  process.exit(1)
}
console.log('[plugin-build] manifest 校验通过')

try {
  const pkgMgrCmd = IS_WIN ? 'pnpm.cmd' : 'pnpm'
  execSync(`${pkgMgrCmd} run build`, {
    cwd: pluginDir,
    stdio: 'inherit',
    env: { ...process.env },
  })
} catch (e) {
  console.error('Plugin build failed!')
  process.exit(1)
}

console.log(`\n=== Plugin build complete: ${targetPlugin} ===\n`)
