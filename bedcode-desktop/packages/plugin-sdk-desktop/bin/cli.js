#!/usr/bin/env node
/**
 * bedcode-plugin-desktop — BedCode 桌面端插件开发工具包命令行
 *
 * 用法：
 *   bedcode-plugin-desktop create <id> <name> [--author <author>] [--dir <dir>] [--rust] [--registry]
 *   bedcode-plugin-desktop build [--resources-dir <dir>] [--frontend-only] [--rust-only]
 *   bedcode-plugin-desktop dev [pluginDir] [--entry <file>] [--port <port>] [--host] [--open]
 *   bedcode-plugin-desktop manifest [--check]
 *   bedcode-plugin-desktop validate [--dir <dir>]
 *   bedcode-plugin-desktop doctor
 *
 * create  从 SDK 内置模板生成插件工程（默认 ts-only；--rust 附带 WASM 后端脚手架）；
 *          --registry 时引用已发布的 SDK 版本，否则引用本地 SDK 相对路径
 * build   串联 vite build → cargo wasm32（rust-ts 插件）；--resources-dir 时复制产物到宿主资源目录
 * dev     启动浏览器开发环境（dev-shell）：vite dev server + HMR（Rust 后端不在浏览器运行）
 * manifest 按插件源码自动填充 plugin.json 的 contributes/permissions；--check 只检查（CI 用）
 * validate 校验 plugin.json 结构与字段合法性（CI 用，exit 1 表示不合法）
 * doctor   环境自检：Node / Rust / wasm32 target / dev-shell 依赖 / SDK 构建产物
 */

import { spawn, spawnSync } from 'node:child_process'
import {
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import { dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { generateManifest } from './manifest-gen.js'

const SDK_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const TEMPLATE_DIR = join(SDK_ROOT, 'template')

// ==================== 工具函数 ====================

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
      } else if (i + 1 < argv.length && !argv[i + 1].startsWith('--')) {
        flags[arg.slice(2)] = argv[++i]
      } else {
        flags[arg.slice(2)] = true
      }
    } else {
      positional.push(arg)
    }
  }
  return { positional, flags }
}

function toPosix(p) {
  return p.replace(/\\/g, '/')
}

function readJson(path, what) {
  try {
    return JSON.parse(readFileSync(path, 'utf-8'))
  } catch (e) {
    console.error(`[bedcode-plugin-desktop] 读取 ${what} 失败: ${path} — ${e.message}`)
    process.exit(1)
  }
}

function run(cmd, args, cwd) {
  const r = spawnSync(cmd, args, { cwd, stdio: 'inherit' })
  if (r.status !== 0) {
    console.error(`[bedcode-plugin-desktop] 命令失败 (${cmd} ${args.join(' ')})`)
    process.exit(1)
  }
}

/** 递归收集目录下所有文件 */
function walk(dir) {
  const out = []
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name)
    if (entry.isDirectory()) out.push(...walk(full))
    else out.push(full)
  }
  return out
}

/** 判断文件是否为文本（含 NUL 字节视为二进制，跳过占位符替换） */
function isText(file) {
  const buf = readFileSync(file)
  return !buf.includes(0)
}

/** 递归替换文件中的占位符 */
function fillPlaceholders(files, repl) {
  for (const file of files) {
    if (!isText(file)) continue
    let content = readFileSync(file, 'utf-8')
    let changed = false
    for (const [key, value] of Object.entries(repl)) {
      if (content.includes(key)) {
        content = content.split(key).join(value)
        changed = true
      }
    }
    if (changed) writeFileSync(file, content, 'utf-8')
  }
}

/** 复制目录内容（不含顶层目录本身） */
function copyDirContents(src, dest) {
  for (const entry of readdirSync(src, { withFileTypes: true })) {
    const from = join(src, entry.name)
    const to = join(dest, entry.name)
    if (entry.isDirectory()) {
      mkdirSync(to, { recursive: true })
      copyDirContents(from, to)
    } else {
      copyFileSync(from, to)
    }
  }
}

/** 读取当前目录插件清单（不存在返回 null） */
function readManifestOrNull(cwd) {
  const path = join(cwd, 'plugin.json')
  if (!existsSync(path)) return null
  try {
    return JSON.parse(readFileSync(path, 'utf-8'))
  } catch {
    return null
  }
}

// ==================== 命令：create ====================

function cmdCreate(positional, flags) {
  const [id, name] = positional
  if (!id || !name) {
    console.error('用法: bedcode-plugin-desktop create <id> <name> [--author <author>] [--dir <dir>] [--rust]')
    console.error('示例: bedcode-plugin-desktop create com.example.my-plugin "My Plugin" --author "me" --rust')
    process.exit(1)
  }
  if (!/^[a-zA-Z0-9]+([._-][a-zA-Z0-9]+)*$/.test(id)) {
    console.error(`非法插件 id: "${id}" — 使用反域名风格，如 com.example.my-plugin`)
    process.exit(1)
  }
  if (!id.includes('.')) {
    console.error(`插件 id 应包含域名段（反域名风格）: "${id}"`)
    process.exit(1)
  }

  const author = flags.author || ''
  const last = id.split('.').pop()
  const outDir = resolve(process.cwd(), flags.dir || last)
  if (existsSync(outDir)) {
    console.error(`目标目录已存在: ${outDir}`)
    process.exit(1)
  }

  // 派生身份：crate 名 / 结构体名 / npm 包名
  const crate = `bedcode_plugin_${last.replace(/[^a-zA-Z0-9_]/g, '_').replace(/^_+|_+$/g, '')}`
  const struct = `${last
    .split(/[^a-zA-Z0-9]/)
    .filter(Boolean)
    .map((seg) => seg.charAt(0).toUpperCase() + seg.slice(1))
    .join('')}Plugin`
  const pkgName = `@bedcode/plugin-${last}`

  // 插件类型：默认 ts-only；--rust 附带 WASM 后端
  const withRust = flags.rust === true
  const pluginType = withRust ? 'rust-ts' : 'ts-only'
  const rustLibraryLine = withRust ? `  "rustLibrary": "${crate}",\n` : ''

  // SDK 依赖标识：--registry 引用已发布版本（npm + crates.io），默认引用本地 SDK 相对路径
  const sdkPkg = readJson(join(SDK_ROOT, 'package.json'), 'SDK package.json')
  const sdkJs = flags.registry === true
    ? `^${sdkPkg.version}`
    : `file:${toPosix(relative(outDir, SDK_ROOT))}`
  const sdkRust = flags.registry === true
    ? `{ version = "${sdkPkg.version}", features = ["wasm"] }`
    : `{ path = "${toPosix(relative(join(outDir, 'rust'), join(SDK_ROOT, 'rust')))}", features = ["wasm"] }`

  // 复制模板并填充占位符
  cpSync(TEMPLATE_DIR, outDir, { recursive: true })
  if (!withRust) {
    rmSync(join(outDir, 'rust'), { recursive: true, force: true })
  }
  fillPlaceholders(walk(outDir), {
    '{{ID}}': id,
    '{{NAME}}': name,
    '{{AUTHOR}}': author,
    '{{CRATE}}': crate,
    '{{STRUCT}}': struct,
    '{{PKG_NAME}}': pkgName,
    '{{SDK_JS}}': sdkJs,
    '{{SDK_RUST}}': sdkRust,
    '{{PLUGIN_TYPE}}': pluginType,
    '{{RUST_LIBRARY_LINE}}': rustLibraryLine,
  })

  console.log(`\n[bedcode-plugin-desktop] 已生成插件工程: ${outDir}`)
  console.log(`  id:         ${id}`)
  console.log(`  name:       ${name}`)
  console.log(`  pluginType: ${pluginType}${withRust ? ` (crate: ${crate})` : ''}`)
  console.log(`\n下一步：`)
  console.log(`  cd ${toPosix(relative(process.cwd(), outDir)) || '.'}`)
  console.log(`  npm install`)
  console.log(`  npm run dev            # 浏览器开发环境（HMR，无需真机）`)
  console.log(`  npm run build          # 构建（vite${withRust ? ' + cargo wasm32' : ''}）`)
  console.log(`  npm run build -- --resources-dir <宿主resources/plugins父目录>  # 复制产物到宿主`)
}

// ==================== 命令：build ====================

function cmdBuild(flags) {
  const cwd = process.cwd()
  const manifestPath = join(cwd, 'plugin.json')
  if (!existsSync(manifestPath)) {
    console.error(`[bedcode-plugin-desktop] 当前目录不是插件工程（缺少 plugin.json）: ${cwd}`)
    console.error('请先在插件目录内运行，或先用 bedcode-plugin-desktop create 生成工程')
    process.exit(1)
  }
  const manifest = readJson(manifestPath, 'plugin.json')
  const { id, main, rustLibrary, pluginType } = manifest
  const hasWasm = pluginType === 'rust-ts' && rustLibrary
  const frontendOnly = flags['frontend-only'] === true
  const rustOnly = flags['rust-only'] === true
  const resourcesDir = flags['resources-dir']

  // 0. 根据源码自动填充 contributes/permissions（构建前同步，保证产物与源码一致）
  try {
    const { changed, report } = generateManifest(cwd)
    if (changed) {
      for (const line of report) console.log(`[bedcode-plugin-desktop]   ${line}`)
      console.log('[bedcode-plugin-desktop] plugin.json 已自动填充')
    }
  } catch (e) {
    console.error(`[bedcode-plugin-desktop] manifest 自动填充失败: ${e.message}`)
    process.exit(1)
  }

  const distMain = join(cwd, 'dist', main || 'index.js')
  const wasmPath = hasWasm
    ? join(cwd, 'rust/target/wasm32-unknown-unknown/release', `${rustLibrary}.wasm`)
    : null

  // 1. 前端构建
  if (!rustOnly) {
    const viteBin = join(cwd, 'node_modules/vite/bin/vite.js')
    if (!existsSync(viteBin)) {
      console.error(`[bedcode-plugin-desktop] vite 未安装 — 先在插件目录运行 npm install`)
      process.exit(1)
    }
    console.log('\n[bedcode-plugin-desktop] ====== 构建前端 (vite) ======')
    run(process.execPath, [viteBin, 'build'], cwd)
    if (!existsSync(distMain)) {
      console.error(`[bedcode-plugin-desktop] vite 构建未产出 dist/${main || 'index.js'}`)
      process.exit(1)
    }
  }

  // 2. WASM 构建（rust-ts 插件）
  if (hasWasm && !frontendOnly) {
    console.log('\n[bedcode-plugin-desktop] ====== 构建 WASM (cargo) ======')
    run('cargo', [
      'build',
      '--target',
      'wasm32-unknown-unknown',
      '--no-default-features',
      '--features',
      'wasm',
      '--manifest-path',
      'rust/Cargo.toml',
      '--release',
    ], cwd)
    if (!existsSync(wasmPath)) {
      console.error(`[bedcode-plugin-desktop] WASM 构建未产出 ${rustLibrary}.wasm`)
      console.error('  若提示 target 缺失: rustup target add wasm32-unknown-unknown')
      process.exit(1)
    }
  }

  // 3. 复制产物到宿主资源目录（--resources-dir <父目录>，按插件 id 建子目录）
  if (resourcesDir) {
    const dest = join(resolve(cwd, resourcesDir), id)
    mkdirSync(dest, { recursive: true })
    copyDirContents(join(cwd, 'dist'), dest)
    copyFileSync(manifestPath, join(dest, 'plugin.json'))
    if (hasWasm && !frontendOnly) {
      copyFileSync(wasmPath, join(dest, `${rustLibrary}.wasm`))
    }
    console.log(`\n[bedcode-plugin-desktop] 产物已复制到: ${dest}`)
  }

  console.log('\n[bedcode-plugin-desktop] 构建完成')
}

// ==================== 命令：dev（浏览器开发环境） ====================

/** 启动 dev-shell：缺依赖时自动安装，然后以长驻 vite 进程运行 */
function cmdDev(positional, flags) {
  const cwd = process.cwd()
  const pluginDir = resolve(cwd, positional[0] || '.')
  const entry = flags.entry ? resolve(cwd, flags.entry) : resolve(pluginDir, 'src/index.ts')

  if (!existsSync(join(pluginDir, 'plugin.json')) && !existsSync(entry)) {
    console.error(`[bedcode-plugin-desktop] 目标不是插件工程（缺少 plugin.json 与 ${entry}）: ${pluginDir}`)
    console.error('用法: bedcode-plugin-desktop dev [pluginDir] [--entry <file>] [--port <port>] [--host] [--open]')
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
  if (flags.host) {
    console.log(`[bedcode-plugin-desktop] 已监听局域网 — 手机/平板与电脑同一 WiFi 时，浏览器打开 http://<电脑IP>:${flags.port || 5173}/ 查看`)
  }
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

// ==================== 命令：validate（清单校验） ====================

/** 合法权限列表（与宿主 src/plugin/permission.ts VALID_PERMISSIONS 同步） */
const VALID_PERMISSIONS = new Set([
  'terminal:input',
  'terminal:output',
  'terminal:observe',
  'session:read',
  'session:write',
  'ui:sidebar',
  'ui:toolbox',
  'ui:statusbar',
  'ui:pageToolbar',
  'ui:input',
  'ui:fileHandler',
  'network:http',
  'storage',
  'broadcast',
  'fileservice',
  'transfer',
])

const VALID_PLUGIN_TYPES = new Set(['ts-only', 'rust-ts', 'rust'])

function cmdValidate(flags) {
  const dir = resolve(process.cwd(), flags.dir || '.')
  const manifestPath = join(dir, 'plugin.json')
  const errors = []
  const warnings = []

  if (!existsSync(manifestPath)) {
    console.error(`[bedcode-plugin-desktop] 缺少 plugin.json: ${dir}`)
    process.exit(1)
  }

  let manifest
  try {
    manifest = JSON.parse(readFileSync(manifestPath, 'utf-8'))
  } catch (e) {
    console.error(`[bedcode-plugin-desktop] plugin.json 不是合法 JSON: ${e.message}`)
    process.exit(1)
  }

  // id：反域名风格
  if (typeof manifest.id !== 'string' || !/^[a-zA-Z0-9]+([._-][a-zA-Z0-9]+)*$/.test(manifest.id) || !manifest.id.includes('.')) {
    errors.push(`id 非法: "${manifest.id}" — 使用反域名风格，如 com.example.my-plugin`)
  }

  // 必填字段
  for (const field of ['name', 'version', 'main', 'pluginType', 'permissions', 'contributes']) {
    if (manifest[field] === undefined || manifest[field] === null || manifest[field] === '') {
      errors.push(`缺少必填字段: ${field}`)
    }
  }

  // pluginType / sandbox / rustLibrary
  if (manifest.pluginType && !VALID_PLUGIN_TYPES.has(manifest.pluginType)) {
    errors.push(`pluginType 非法: "${manifest.pluginType}"（允许: ${[...VALID_PLUGIN_TYPES].join(' / ')}）`)
  }
  if (manifest.sandbox && !['inline', 'isolated'].includes(manifest.sandbox)) {
    errors.push(`sandbox 非法: "${manifest.sandbox}"（允许: inline / isolated）`)
  }
  if (manifest.pluginType === 'rust-ts' && !manifest.rustLibrary) {
    errors.push('pluginType=rust-ts 时必须提供 rustLibrary（与 Cargo.toml 包名一致）')
  }

  // 权限
  if (Array.isArray(manifest.permissions)) {
    const unknown = manifest.permissions.filter((p) => !VALID_PERMISSIONS.has(p))
    if (unknown.length) {
      errors.push(`未知权限: ${unknown.join(', ')}（合法列表见宿主 permission.ts）`)
    }
  }

  // main 产物存在性（未构建仅警告）
  const distMain = join(dir, 'dist', manifest.main || 'index.js')
  if (!existsSync(distMain)) {
    warnings.push(`dist/${manifest.main || 'index.js'} 不存在 — 尚未构建（npm run build）`)
  }

  // contributes 结构
  if (manifest.contributes && typeof manifest.contributes !== 'object') {
    errors.push('contributes 必须是对象')
  }

  for (const w of warnings) console.log(`  ⚠ ${w}`)
  for (const e of errors) console.error(`  ✗ ${e}`)

  if (errors.length) {
    console.error(`\n[bedcode-plugin-desktop] validate 失败: ${errors.length} 个错误`)
    process.exit(1)
  }
  console.log(`\n[bedcode-plugin-desktop] validate 通过${warnings.length ? `（${warnings.length} 个警告）` : ''}`)
}

// ==================== 命令：doctor（环境自检） ====================

function cmdDoctor() {
  const checks = []
  const add = (name, ok, detail) => checks.push({ name, ok, detail })

  // Node 版本
  const nodeMajor = Number(process.versions.node.split('.')[0])
  add('Node.js >= 20', nodeMajor >= 20, process.version)

  // Rust + wasm32 target
  let rustOk = false
  let rustDetail = '未安装'
  try {
    const r = spawnSync('cargo', ['--version'], { stdio: 'pipe' })
    if (r.status === 0) {
      rustOk = true
      rustDetail = r.stdout.toString().trim()
    }
  } catch {
    // 未安装
  }
  add('Rust (cargo)', rustOk, rustDetail)
  if (rustOk) {
    const t = spawnSync('rustup', ['target', 'list', '--installed'], { stdio: 'pipe' })
    const hasWasm = t.status === 0 && t.stdout.toString().includes('wasm32-unknown-unknown')
    add('wasm32-unknown-unknown target', hasWasm, hasWasm ? '已安装' : '缺失 — 运行 rustup target add wasm32-unknown-unknown')
  } else {
    add('wasm32-unknown-unknown target', false, '需先安装 Rust')
  }

  // 当前目录是否为插件工程
  const manifest = readManifestOrNull(process.cwd())
  if (manifest) {
    add('当前目录是插件工程', true, `${manifest.id} (${manifest.pluginType})`)
  } else {
    add('当前目录是插件工程', false, '缺少 plugin.json — 在插件目录内运行本命令时检查才有意义')
  }

  // dev-shell 依赖（dev 命令就绪性）
  const devVite = join(SDK_ROOT, 'dev-shell/node_modules/vite/bin/vite.js')
  add('dev-shell 依赖（dev 命令）', existsSync(devVite), existsSync(devVite) ? '已安装' : '首次运行 dev 命令时自动安装')

  // SDK dist（file: 依赖的插件需要）
  const sdkDist = join(SDK_ROOT, 'dist/index.js')
  add('SDK 构建产物（file: 依赖）', existsSync(sdkDist), existsSync(sdkDist) ? '已构建' : '缺失 — 运行 npm run build（SDK 目录内）')

  let failed = 0
  for (const c of checks) {
    const icon = c.ok ? '✓' : '✗'
    if (!c.ok) failed++
    console.log(`  ${icon} ${c.name} — ${c.detail}`)
  }
  console.log(`\n[bedcode-plugin-desktop] doctor 完成: ${checks.length - failed}/${checks.length} 通过`)
  if (failed) process.exit(1)
}

// ==================== 入口 ====================

function main() {
  const { positional, flags } = parseArgs(process.argv.slice(2))
  const [cmd] = positional
  const pkg = readJson(join(SDK_ROOT, 'package.json'), 'SDK package.json')

  if (flags.version || flags.v) {
    console.log(pkg.version)
    process.exit(0)
  }

  if (flags.help || flags.h || !cmd) {
    console.log(`bedcode-plugin-desktop v${pkg.version}`)
    console.log('\nBedCode 桌面端插件开发工具包\n')
    console.log('用法:')
    console.log('  bedcode-plugin-desktop create <id> <name> [--author <author>] [--dir <dir>] [--rust] [--registry]')
    console.log('  bedcode-plugin-desktop build [--resources-dir <dir>] [--frontend-only] [--rust-only]')
    console.log('  bedcode-plugin-desktop dev [pluginDir] [--entry <file>] [--port <port>] [--host] [--open]   # 浏览器开发环境（HMR）')
    console.log('  bedcode-plugin-desktop manifest [--check]   # 按源码自动填充 contributes/permissions')
    console.log('  bedcode-plugin-desktop validate [--dir <dir>]   # 校验 plugin.json 结构')
    console.log('  bedcode-plugin-desktop doctor   # 环境自检（Node/Rust/wasm32/dev-shell/SDK）')
    process.exit(0)
  }

  switch (cmd) {
    case 'create':
      cmdCreate(positional.slice(1), flags)
      break
    case 'build':
      cmdBuild(flags)
      break
    case 'dev':
      cmdDev(positional.slice(1), flags)
      break
    case 'manifest':
      cmdManifest(flags)
      break
    case 'validate':
      cmdValidate(flags)
      break
    case 'doctor':
      cmdDoctor()
      break
    default:
      console.error(`未知命令: ${cmd}（运行 bedcode-plugin-desktop --help 查看用法）`)
      process.exit(1)
  }
}

main()
