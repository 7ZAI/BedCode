#!/usr/bin/env node
/**
 * bedcode-plugin-desktop — BedCode 桌面端插件开发工具包命令行
 *
 * 用法：
 *   bedcode-plugin-desktop manifest [--check]
 *   bedcode-plugin-desktop dev [pluginDir] [--entry <file>] [--port <port>] [--host] [--open]
 *
 * manifest  按插件源码自动填充 plugin.json 的 contributes/permissions；
 *           --check 只检查不一致不写入（CI 用，exit 1 表示需要更新）
 * dev       启动浏览器开发环境（dev-shell）：vite dev server + HMR，插件源码在
 *           mock 宿主的桌面端骨架中实时预览（Rust 后端不在浏览器运行）
 */

import { spawn, spawnSync } from 'node:child_process'
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

// ==================== 命令：dev（浏览器开发环境） ====================

/** 启动 dev-shell：缺依赖时自动安装，然后以长驻 vite 进程运行 */
function cmdDev(positional, flags) {
  const cwd = process.cwd()
  const pluginDir = resolve(cwd, positional[0] || '.')
  const entry = flags.entry ? resolve(cwd, flags.entry) : resolve(pluginDir, 'src/index.ts')

  if (!existsSync(join(pluginDir, 'plugin.json')) && !existsSync(entry)) {
    console.error(`[bedcode-plugin-desktop] 目标不是插件工程（缺少 plugin.json 与 ${entry}）: ${pluginDir}`)
    console.error('用法: bedcode-plugin-desktop dev [pluginDir] [--entry <file>] [--port <port>] [--open]')
    process.exit(1)
  }

  const devShellDir = join(SDK_ROOT, 'dev-shell')
  if (!existsSync(devShellDir)) {
    console.error(`[bedcode-plugin-desktop] dev-shell 不存在: ${devShellDir}（SDK 包不完整）`)
    process.exit(1)
  }

  // dev-shell 首次运行需要安装自身依赖（vue / vite / tailwind 等）
  const viteBin = join(devShellDir, 'node_modules/vite/bin/vite.js')
  if (!existsSync(viteBin)) {
    console.log('[bedcode-plugin-desktop] dev-shell 依赖缺失，正在安装（仅首次）…')
    run('npm', ['install', '--no-audit', '--no-fund'], devShellDir)
  }

  const args = [
    viteBin,
    '--config',
    join(devShellDir, 'vite.config.ts'),
    '--port',
    String(flags.port || 5173),
  ]
  if (flags.host) args.push('--host', typeof flags.host === 'string' ? flags.host : '0.0.0.0')
  if (flags.open) args.push('--open')

  console.log(`[bedcode-plugin-desktop] 启动 dev-shell（插件: ${pluginDir}）`)
  console.log(`[bedcode-plugin-desktop] 浏览器打开 http://localhost:${flags.port || 5173}/ 预览（Ctrl+C 退出）`)
  const child = spawn(process.execPath, args, {
    cwd: devShellDir,
    stdio: 'inherit',
    env: {
      ...process.env,
      BEDCODE_DEV_PLUGINS: `${pluginDir}::${entry}`,
    },
  })
  child.on('exit', (code) => {
    process.exit(code ?? 0)
  })
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
    console.log('  bedcode-plugin-desktop dev [pluginDir] [--entry <file>] [--port <port>] [--host] [--open]   # 浏览器开发环境（HMR）')
    process.exit(0)
  }

  switch (cmd) {
    case 'manifest':
      cmdManifest(flags)
      break
    case 'dev':
      cmdDev(positional.slice(1), flags)
      break
    default:
      console.error(`未知命令: ${cmd}（运行 bedcode-plugin-desktop --help 查看用法）`)
      process.exit(1)
  }
}

main()
