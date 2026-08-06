#!/usr/bin/env node
/**
 * bedcode-plugin-desktop — BedCode 桌面端插件开发工具包命令行
 *
 * 用法：
 *   bedcode-plugin-desktop manifest [--check]
 *
 * manifest  按插件源码自动填充 plugin.json 的 contributes/permissions；
 *           --check 只检查不一致不写入（CI 用，exit 1 表示需要更新）
 */

import { existsSync, readFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { generateManifest } from './manifest-gen.js'

const SDK_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..')

/** 解析命令行参数：positional 数组 + flags 映射（--flag value / --flag=value / 布尔 --flag） */
function parseArgs(argv) {
  const positional = []
  const flags = {}
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i]
    if (arg.startsWith('--')) {
      const eq = arg.indexOf('=')
      if (eq !== -1) {
        flags[arg.slice(2, eq)] = arg.slice(eq + 1)
      } else {
        flags[arg.slice(2)] = true
      }
    } else {
      positional.push(arg)
    }
  }
  return { positional, flags }
}

function readJson(path, what) {
  try {
    return JSON.parse(readFileSync(path, 'utf-8'))
  } catch (e) {
    console.error(`[bedcode-plugin-desktop] 读取 ${what} 失败: ${path} — ${e.message}`)
    process.exit(1)
  }
}

// ==================== 命令：manifest（自动填充） ====================

function cmdManifest(flags) {
  const cwd = process.cwd()
  const check = flags.check === true
  if (!existsSync(join(cwd, 'plugin.json'))) {
    console.error(`[bedcode-plugin-desktop] 当前目录不是插件工程（缺少 plugin.json）: ${cwd}`)
    console.error('请在插件目录内运行，或在插件构建脚本中调用 SDK 的 manifest-gen')
    process.exit(1)
  }
  try {
    const { changed, report } = generateManifest(cwd, { check })
    if (!changed) {
      console.log('[bedcode-plugin-desktop] plugin.json 已是最新，无需更新')
      return
    }
    for (const line of report) console.log(`[bedcode-plugin-desktop]   ${line}`)
    if (check) {
      console.log('[bedcode-plugin-desktop] --check 模式：plugin.json 与源码不一致（未写入）')
      process.exit(1)
    }
    console.log('[bedcode-plugin-desktop] plugin.json 已根据源码自动填充')
  } catch (e) {
    console.error(`[bedcode-plugin-desktop] manifest 生成失败: ${e.message}`)
    process.exit(1)
  }
}

// ==================== 入口 ====================

function main() {
  const { positional, flags } = parseArgs(process.argv.slice(2))
  const [cmd] = positional

  if (flags.help || flags.h || !cmd) {
    const pkg = readJson(join(SDK_ROOT, 'package.json'), 'SDK package.json')
    console.log(`bedcode-plugin-desktop v${pkg.version}`)
    console.log('\nBedCode 桌面端插件开发工具包\n')
    console.log('用法:')
    console.log('  bedcode-plugin-desktop manifest [--check]   # 按源码自动填充 contributes/permissions')
    process.exit(0)
  }

  switch (cmd) {
    case 'manifest':
      cmdManifest(flags)
      break
    default:
      console.error(`未知命令: ${cmd}（运行 bedcode-plugin-desktop --help 查看用法）`)
      process.exit(1)
  }
}

main()
