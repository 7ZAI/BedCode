#!/usr/bin/env node
/**
 * bedcode-plugin — BedCode 移动端插件开发工具包命令行
 *
 * 用法：
 *   bedcode-plugin create <id> <name> [--author <author>] [--dir <dir>] [--ts-only] [--registry]
 *   bedcode-plugin build [--resources-dir <dir>] [--frontend-only] [--rust-only] [--watch]
 *   bedcode-plugin package [-o <file>] [--hash]
 *   bedcode-plugin dev [pluginDir] [--entry <file>] [--port <port>] [--host] [--open]
 *   bedcode-plugin manifest [--check]
 *   bedcode-plugin validate [--dir <dir>]
 *   bedcode-plugin doctor
 *
 * create  从 SDK 内置模板生成插件工程（填充 id/name/author/crate 名）；
 *          --ts-only 生成纯前端插件（无 rust/ 目录）；--registry 时引用已发布版本
 * build   串联 vite build → cargo wasm32 构建 → componentize 组件化（Component Model）；
 *          --resources-dir 时复制产物到宿主资源目录；
 *          --watch 时 vite 长驻构建前端，重建后自动复制产物（前端热更）
 * package 将产物打包为 {id}.zip 插件包（分发单元）；--hash 时计算并写入 wasmHash
 * dev     启动浏览器开发环境（dev-shell）：vite dev server + HMR，插件源码在
 *          mock 宿主的移动端骨架中实时预览（WASM 后端不在浏览器运行）；
 *          --host 时监听局域网，手机浏览器访问 http://<PC-IP>:<port> 可查看页面
 * validate 校验 plugin.json 结构与字段合法性（CI 用，exit 1 表示不合法）
 * doctor   环境自检：Node / Rust / wasm32 target / dev-shell 依赖 / SDK 构建产物
 */

import { spawn, spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import {
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  watch,
  writeFileSync,
} from 'node:fs'
import { dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { crc32 } from 'node:zlib'
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
    console.error(`[bedcode-plugin] 读取 ${what} 失败: ${path} — ${e.message}`)
    process.exit(1)
  }
}

function run(cmd, args, cwd) {
  const r = spawnSync(cmd, args, { cwd, stdio: 'inherit' })
  if (r.status !== 0) {
    console.error(`[bedcode-plugin] 命令失败 (${cmd} ${args.join(' ')})`)
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

// ==================== ZIP 打包（store 模式，零依赖） ====================

/** 构造 store 模式 zip 字节流 */
function buildZip(entries) {
  const chunks = []
  const central = []
  let offset = 0

  // DOS 时间戳（本地时间）
  const now = new Date()
  const dosTime = (now.getHours() << 11) | (now.getMinutes() << 5) | (now.getSeconds() >> 1)
  const dosDate =
    ((now.getFullYear() - 1980) << 9) | ((now.getMonth() + 1) << 5) | now.getDate()

  for (const entry of entries) {
    const nameBuf = Buffer.from(entry.name, 'utf-8')
    const data = entry.data
    const crc = crc32(data) >>> 0

    // 本地文件头
    const lh = Buffer.alloc(30)
    lh.writeUInt32LE(0x04034b50, 0) // signature
    lh.writeUInt16LE(20, 4) // version needed
    lh.writeUInt16LE(0, 6) // flags
    lh.writeUInt16LE(0, 8) // method: store
    lh.writeUInt16LE(dosTime, 10)
    lh.writeUInt16LE(dosDate, 12)
    lh.writeUInt32LE(crc, 14)
    lh.writeUInt32LE(data.length, 18) // compressed size
    lh.writeUInt32LE(data.length, 22) // uncompressed size
    lh.writeUInt16LE(nameBuf.length, 26)
    lh.writeUInt16LE(0, 28) // extra len
    chunks.push(lh, nameBuf, data)

    // 中央目录项
    const ch = Buffer.alloc(46)
    ch.writeUInt32LE(0x02014b50, 0) // signature
    ch.writeUInt16LE(20, 4) // version made by
    ch.writeUInt16LE(20, 6) // version needed
    ch.writeUInt16LE(0, 8) // flags
    ch.writeUInt16LE(0, 10) // method
    ch.writeUInt16LE(dosTime, 12)
    ch.writeUInt16LE(dosDate, 14)
    ch.writeUInt32LE(crc, 16)
    ch.writeUInt32LE(data.length, 20)
    ch.writeUInt32LE(data.length, 24)
    ch.writeUInt16LE(nameBuf.length, 28)
    ch.writeUInt16LE(0, 30) // extra len
    ch.writeUInt16LE(0, 32) // comment len
    ch.writeUInt16LE(0, 34) // disk start
    ch.writeUInt16LE(0, 36) // internal attrs
    ch.writeUInt32LE(0, 38) // external attrs
    ch.writeUInt32LE(offset, 42) // local header offset
    central.push(ch, nameBuf)

    offset += lh.length + nameBuf.length + data.length
  }

  const centralSize = central.reduce((sum, b) => sum + b.length, 0)

  // 中央目录结束记录
  const eocd = Buffer.alloc(22)
  eocd.writeUInt32LE(0x06054b50, 0) // signature
  eocd.writeUInt16LE(0, 4) // disk number
  eocd.writeUInt16LE(0, 6) // cd start disk
  eocd.writeUInt16LE(entries.length, 8)
  eocd.writeUInt16LE(entries.length, 10)
  eocd.writeUInt32LE(centralSize, 12)
  eocd.writeUInt32LE(offset, 16)
  eocd.writeUInt16LE(0, 20) // comment len

  return Buffer.concat([...chunks, ...central, eocd])
}

// ==================== 命令：create ====================

function cmdCreate(positional, flags) {
  const [id, name] = positional
  if (!id || !name) {
    console.error('用法: bedcode-plugin create <id> <name> [--author <author>] [--dir <dir>]')
    console.error('示例: bedcode-plugin create com.example.my-plugin "My Plugin" --author "me"')
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

  // 插件类型：默认 wasm（前端 + WASM 后端）；--ts-only 生成纯前端插件
  const tsOnly = flags['ts-only'] === true
  const pluginType = tsOnly ? 'ts-only' : 'wasm'
  // 注意：值内直接嵌入实际 crate 名（fillPlaceholders 按序替换，不能引用占位符本身）
  const rustLibraryLine = tsOnly ? '' : `  "rustLibrary": "${crate}",\n`

  // SDK 依赖标识：--registry 引用已发布版本（npm + crates.io），默认引用本地 SDK 相对路径
  const sdkPkg = readJson(join(SDK_ROOT, 'package.json'), 'SDK package.json')
  const sdkJs = flags.registry === true
    ? `^${sdkPkg.version}`
    : `file:${toPosix(relative(outDir, SDK_ROOT))}`
  const sdkRust = flags.registry === true
    ? `"${sdkPkg.version}"`
    : `{ path = "${toPosix(relative(join(outDir, 'rust'), join(SDK_ROOT, 'rust')))}" }`

  // 复制模板并填充占位符
  cpSync(TEMPLATE_DIR, outDir, { recursive: true })
  if (tsOnly) {
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

  console.log(`\n[bedcode-plugin] 已生成插件工程: ${outDir}`)
  console.log(`  id:         ${id}`)
  console.log(`  name:       ${name}`)
  console.log(`  pluginType: ${pluginType}${tsOnly ? '' : ` (crate: ${crate})`}`)
  console.log(`\n下一步：`)
  console.log(`  cd ${toPosix(relative(process.cwd(), outDir)) || '.'}`)
  console.log(`  npm install`)
  console.log(`  npm run dev            # 浏览器开发环境（HMR，无需真机）`)
  console.log(`  npm run build        # 构建（vite + WASM）`)
  console.log(`  npm run package      # 打包 dist/${id}.zip 插件包`)
}

// ==================== 命令：dev（浏览器开发环境） ====================

/** 启动 dev-shell：缺依赖时自动安装，然后以长驻 vite 进程运行 */
function cmdDev(positional, flags) {
  const cwd = process.cwd()
  const pluginDir = resolve(cwd, positional[0] || '.')
  const entry = flags.entry ? resolve(cwd, flags.entry) : resolve(pluginDir, 'src/index.ts')

  if (!existsSync(join(pluginDir, 'plugin.json')) && !existsSync(entry)) {
    console.error(`[bedcode-plugin] 目标不是插件工程（缺少 plugin.json 与 ${entry}）: ${pluginDir}`)
    console.error('用法: bedcode-plugin dev [pluginDir] [--entry <file>] [--port <port>] [--open]')
    process.exit(1)
  }

  const devShellDir = join(SDK_ROOT, 'dev-shell')
  if (!existsSync(devShellDir)) {
    console.error(`[bedcode-plugin] dev-shell 不存在: ${devShellDir}（SDK 包不完整）`)
    process.exit(1)
  }

  // dev-shell 首次运行需要安装自身依赖（vue / vite / tailwind 等）
  const viteBin = join(devShellDir, 'node_modules/vite/bin/vite.js')
  if (!existsSync(viteBin)) {
    console.log('[bedcode-plugin] dev-shell 依赖缺失，正在安装（仅首次）…')
    run('npm', ['install', '--no-audit', '--no-fund'], devShellDir)
  }

  const args = [
    viteBin,
    '--config',
    join(devShellDir, 'vite.config.ts'),
    '--port',
    String(flags.port || 5173),
  ]
  // --host：监听局域网（vite 默认仅 localhost），手机浏览器可访问查看页面
  if (flags.host) args.push('--host', typeof flags.host === 'string' ? flags.host : '0.0.0.0')
  if (flags.open) args.push('--open')

  console.log(`[bedcode-plugin] 启动 dev-shell（插件: ${pluginDir}）`)
  if (flags.host) {
    console.log(`[bedcode-plugin] 已监听局域网 — 手机与电脑同一 WiFi 时，手机浏览器打开 http://<电脑IP>:${flags.port || 5173}/ 查看（建议关掉手机框开关）`)
  }
  console.log(`[bedcode-plugin] 浏览器打开 http://localhost:${flags.port || 5173}/ 预览（Ctrl+C 退出）`)
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
  try {
    const { changed, report } = generateManifest(cwd, { check })
    if (!changed) {
      console.log('[bedcode-plugin] plugin.json 已是最新，无需更新')
      return
    }
    for (const line of report) console.log(`[bedcode-plugin]   ${line}`)
    if (check) {
      console.log('[bedcode-plugin] --check 模式：plugin.json 与源码不一致（未写入）')
      process.exit(1)
    }
    console.log('[bedcode-plugin] plugin.json 已根据源码自动填充')
  } catch (e) {
    console.error(`[bedcode-plugin] manifest 生成失败: ${e.message}`)
    process.exit(1)
  }
}

/** 从目录向上查找 vite 可执行文件（兼容 npm workspace 依赖 hoist 到仓库根的场景） */
function findViteBin(startDir) {
  let dir = startDir
  for (let i = 0; i < 6; i++) {
    const candidate = join(dir, 'node_modules/vite/bin/vite.js')
    if (existsSync(candidate)) return candidate
    const parent = resolve(dir, '..')
    if (parent === dir) break
    dir = parent
  }
  return null
}

// ==================== 命令：build（watch 模式） ====================

/**
 * watch 模式：vite build --watch 长驻构建前端，重建完成后自动复制产物到宿主资源目录。
 *
 * 只 watch 前端：WASM 产物不参与（已构建时随复制一并带上；Rust 改动需一次性 `bedcode-plugin build`）。
 */
function startBuildWatch(cwd, resourcesDir, manifest) {
  const { id, main = 'index.js', rustLibrary, pluginType } = manifest
  const hasWasm = pluginType === 'wasm' && rustLibrary
  const viteBin = findViteBin(cwd)
  if (!viteBin) {
    console.error('[bedcode-plugin] vite 未找到（自插件目录向上查找 node_modules/vite 均无）— 先在插件目录或仓库根运行 npm install')
    process.exit(1)
  }

  const dest = join(resolve(cwd, resourcesDir), id)
  const distDir = join(cwd, 'dist')
  const wasmPath = hasWasm
    ? join(cwd, 'rust/target/wasm32-unknown-unknown/release', `${rustLibrary}.wasm`)
    : null

  console.log('\n[bedcode-plugin] ====== watch 前端构建（vite build --watch） ======')
  console.log(`[bedcode-plugin] 产物自动复制到: ${dest}`)
  console.log('[bedcode-plugin] 修改插件前端源码后自动重建 + 复制（WASM 改动需一次性 npm run build）')
  console.log('[bedcode-plugin] Ctrl+C 退出\n')

  // WASM 缺失预检：全新 resources 目录（从未跑过全量 build）时，复制完成后插件
  // 加载仍会因缺 .wasm 失败，提前给出可执行指引
  if (hasWasm && !existsSync(wasmPath)) {
    console.warn(`[bedcode-plugin] ⚠ WASM 产物缺失：${wasmPath}`)
    console.warn('[bedcode-plugin]   watch 只重建前端。首次请先执行一次全量构建：')
    console.warn('[bedcode-plugin]   npm run build（含 cargo wasm32 构建）')
  }

  // 长驻 vite watch 构建（首次启动即完整构建一次）
  const child = spawn(process.execPath, [viteBin, 'build', '--watch'], { cwd, stdio: 'inherit' })
  child.on('exit', (code) => process.exit(code ?? 0))

  // 复制产物到宿主资源目录（覆盖式，不删目录：宿主运行时可能正持有文件句柄）
  const copy = () => {
    try {
      if (!existsSync(join(distDir, main))) return
      mkdirSync(dest, { recursive: true })
      copyDirContents(distDir, dest)
      copyFileSync(join(cwd, 'plugin.json'), join(dest, 'plugin.json'))
      // WASM 已构建则一并带上（缺失不报错：watch 场景以前端热更为主）
      if (hasWasm && wasmPath && existsSync(wasmPath)) {
        copyFileSync(wasmPath, join(dest, `${rustLibrary}.wasm`))
      }
      console.log(`[bedcode-plugin] ${new Date().toLocaleTimeString()} 产物已复制 → ${dest}`)
    } catch (e) {
      console.error(`[bedcode-plugin] 复制失败: ${e.message}`)
    }
  }

  // dist 任意文件事件都防抖复制（Windows 上 fs.watch 的 filename 常为 null，按时间聚合最稳）
  let timer = null
  const schedule = () => {
    clearTimeout(timer)
    timer = setTimeout(copy, 500)
  }

  if (!existsSync(distDir)) mkdirSync(distDir, { recursive: true })
  const watcher = watch(distDir, { persistent: true }, schedule)
  watcher.on('error', (e) => console.error(`[bedcode-plugin] dist 监听错误: ${e.message}`))
}

// ==================== 命令：build ====================

function cmdBuild(flags) {
  const cwd = process.cwd()
  const manifestPath = join(cwd, 'plugin.json')
  if (!existsSync(manifestPath)) {
    console.error(`[bedcode-plugin] 当前目录不是插件工程（缺少 plugin.json）: ${cwd}`)
    console.error('请在插件目录内运行，或先用 bedcode-plugin create 生成工程')
    process.exit(1)
  }
  const manifest = readJson(manifestPath, 'plugin.json')
  const { id, main, rustLibrary, pluginType } = manifest
  const hasWasm = pluginType === 'wasm' && rustLibrary
  const frontendOnly = flags['frontend-only'] === true
  const rustOnly = flags['rust-only'] === true
  const resourcesDir = flags['resources-dir']

  // 0. 根据源码自动填充 contributes/permissions（构建前同步，保证产物与源码一致）
  try {
    const { changed, report } = generateManifest(cwd)
    if (changed) {
      for (const line of report) console.log(`[bedcode-plugin]   ${line}`)
      console.log('[bedcode-plugin] plugin.json 已自动填充')
    }
  } catch (e) {
    console.error(`[bedcode-plugin] manifest 自动填充失败: ${e.message}`)
    process.exit(1)
  }

  // 0.5 watch 模式：vite 长驻构建 + 产物变化自动复制（需 --resources-dir 指定复制目标）
  if (flags.watch) {
    if (!resourcesDir) {
      console.error('[bedcode-plugin] --watch 需要 --resources-dir <宿主资源目录>（前端产物复制目标）')
      process.exit(1)
    }
    startBuildWatch(cwd, resourcesDir, manifest)
    return
  }

  const distMain = join(cwd, 'dist', main || 'index.js')
  const wasmPath = hasWasm
    ? join(cwd, 'rust/target/wasm32-unknown-unknown/release', `${rustLibrary}.wasm`)
    : null

  // 1. 前端构建
  if (!rustOnly) {
    const viteBin = join(cwd, 'node_modules/vite/bin/vite.js')
    if (!existsSync(viteBin)) {
      console.error(`[bedcode-plugin] vite 未安装 — 先在插件目录运行 npm install`)
      process.exit(1)
    }
    console.log('\n[bedcode-plugin] ====== 构建前端 (vite) ======')
    run(process.execPath, [viteBin, 'build'], cwd)
    if (!existsSync(distMain)) {
      console.error(`[bedcode-plugin] vite 构建未产出 dist/${main || 'index.js'}`)
      process.exit(1)
    }
  }

  // 2. WASM 构建
  if (hasWasm && !frontendOnly) {
    console.log('\n[bedcode-plugin] ====== 构建 WASM (cargo) ======')
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
      console.error(`[bedcode-plugin] WASM 构建未产出 ${rustLibrary}.wasm`)
      console.error('  若提示 target 缺失: rustup target add wasm32-unknown-unknown')
      process.exit(1)
    }

    // 组件化：将 wit-bindgen 产出的 core module（含 component-type 自定义段）
    // 编码为 Component Model 组件（等价 wasm-tools component new）。
    // 工具幂等——产物已是组件（0d 00 01 00）时直接复制，支持增量构建。
    // 迁移 spec §4 S2：产物字节形态 `00 61 73 6d 0d 00 01 00`（模块段在组件头前）
    console.log('\n[bedcode-plugin] ====== 组件化 WASM (Component Model) ======')
    const componentizeManifest = join(SDK_ROOT, 'rust/tools/componentize/Cargo.toml')
    run('cargo', [
      'run',
      '--release',
      '--manifest-path',
      componentizeManifest,
      '--',
      wasmPath,
      '-o',
      wasmPath,
    ], cwd)
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
    console.log(`\n[bedcode-plugin] 产物已复制到: ${dest}`)
  }

  console.log('\n[bedcode-plugin] 构建完成')
}

// ==================== 命令：package ====================

function cmdPackage(flags) {
  const cwd = process.cwd()
  const manifestPath = join(cwd, 'plugin.json')
  if (!existsSync(manifestPath)) {
    console.error(`[bedcode-plugin] 当前目录不是插件工程（缺少 plugin.json）: ${cwd}`)
    process.exit(1)
  }
  const manifest = readJson(manifestPath, 'plugin.json')
  const { id, main, rustLibrary, pluginType } = manifest
  const hasWasm = pluginType === 'wasm' && rustLibrary

  // 打包前自动填充，保证 zip 内 plugin.json 与源码一致
  try {
    const { changed, report } = generateManifest(cwd)
    if (changed) {
      for (const line of report) console.log(`[bedcode-plugin]   ${line}`)
      console.log('[bedcode-plugin] plugin.json 已自动填充')
    }
  } catch (e) {
    console.error(`[bedcode-plugin] manifest 自动填充失败: ${e.message}`)
    process.exit(1)
  }

  const distDir = join(cwd, 'dist')
  const distMain = join(distDir, main || 'index.js')
  const wasmPath = hasWasm
    ? join(cwd, 'rust/target/wasm32-unknown-unknown/release', `${rustLibrary}.wasm`)
    : null

  if (!existsSync(distMain)) {
    console.error(`[bedcode-plugin] dist/${main || 'index.js'} 缺失 — 先运行 npm run build`)
    process.exit(1)
  }
  if (hasWasm && !existsSync(wasmPath)) {
    console.error(`[bedcode-plugin] ${rustLibrary}.wasm 缺失 — 先运行 npm run build`)
    process.exit(1)
  }

  // --hash：计算 WASM SHA256 写入 plugin.json（安装时宿主按 wasmHash 校验完整性）
  if (hasWasm && flags.hash) {
    const hash = `sha256-${createHash('sha256').update(readFileSync(wasmPath)).digest('hex')}`
    const current = readJson(manifestPath, 'plugin.json')
    if (current.wasmHash !== hash) {
      current.wasmHash = hash
      writeFileSync(manifestPath, JSON.stringify(current, null, 2) + '\n', 'utf-8')
      console.log(`[bedcode-plugin] 已写入 wasmHash: ${hash.slice(0, 20)}…`)
    }
  }

  // 打包条目：plugin.json + dist 全部产物 + wasm，全部位于 zip 根目录
  const entries = [
    { name: 'plugin.json', data: readFileSync(manifestPath) },
  ]
  for (const f of readdirSync(distDir)) {
    const full = join(distDir, f)
    if (statSync(full).isFile()) entries.push({ name: f, data: readFileSync(full) })
  }
  if (hasWasm) entries.push({ name: `${rustLibrary}.wasm`, data: readFileSync(wasmPath) })

  const outFile = flags.o ? resolve(cwd, flags.o) : join(distDir, `${id}.zip`)
  writeFileSync(outFile, buildZip(entries))

  console.log(`\n[bedcode-plugin] 已打包 ${entries.length} 个文件 -> ${outFile}`)
}

// ==================== 命令：validate（清单校验） ====================

/** 合法权限列表（与宿主 src/plugin/permission.ts 权限映射键同步） */
const VALID_PERMISSIONS = new Set([
  'terminal:input',
  'terminal:output',
  'session:read',
  'session:write',
  'ui:toolbox',
  'ui:navtab',
  'ui:settings',
  'ui:input',
  'ui:route',
  'network:http',
  'storage',
  'fs:read',
  'fs:write',
  'bus',
  'fileservice',
])

const VALID_PLUGIN_TYPES = new Set(['wasm', 'ts-only', 'rust', 'rust-ts'])
const WASM_HASH_RE = /^sha256-[0-9a-f]{64}$/i

function cmdValidate(flags) {
  const dir = resolve(process.cwd(), flags.dir || '.')
  const manifestPath = join(dir, 'plugin.json')
  const errors = []
  const warnings = []

  if (!existsSync(manifestPath)) {
    console.error(`[bedcode-plugin] 缺少 plugin.json: ${dir}`)
    process.exit(1)
  }

  let manifest
  try {
    manifest = JSON.parse(readFileSync(manifestPath, 'utf-8'))
  } catch (e) {
    console.error(`[bedcode-plugin] plugin.json 不是合法 JSON: ${e.message}`)
    process.exit(1)
  }

  // id：反域名风格
  if (
    typeof manifest.id !== 'string' ||
    !/^[a-zA-Z0-9]+([._-][a-zA-Z0-9]+)*$/.test(manifest.id) ||
    !manifest.id.includes('.')
  ) {
    errors.push(`id 非法: "${manifest.id}" — 使用反域名风格，如 com.example.my-plugin`)
  }

  // 必填字段
  for (const field of ['name', 'version', 'main', 'pluginType', 'permissions', 'contributes']) {
    if (manifest[field] === undefined || manifest[field] === null || manifest[field] === '') {
      errors.push(`缺少必填字段: ${field}`)
    }
  }

  // pluginType / rustLibrary / wasmHash
  if (manifest.pluginType && !VALID_PLUGIN_TYPES.has(manifest.pluginType)) {
    errors.push(`pluginType 非法: "${manifest.pluginType}"（允许: ${[...VALID_PLUGIN_TYPES].join(' / ')}）`)
  }
  if (manifest.pluginType === 'wasm' && !manifest.rustLibrary) {
    errors.push('pluginType=wasm 时必须提供 rustLibrary（与 Cargo.toml 包名一致）')
  }
  if (manifest.wasmHash !== undefined && !WASM_HASH_RE.test(manifest.wasmHash)) {
    errors.push(`wasmHash 格式非法: "${manifest.wasmHash}"（应为 sha256-<64位hex>）`)
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
    console.error(`\n[bedcode-plugin] validate 失败: ${errors.length} 个错误`)
    process.exit(1)
  }
  console.log(`\n[bedcode-plugin] validate 通过${warnings.length ? `（${warnings.length} 个警告）` : ''}`)
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
  const manifestPath = join(process.cwd(), 'plugin.json')
  if (existsSync(manifestPath)) {
    try {
      const m = readJson(manifestPath, 'plugin.json')
      add('当前目录是插件工程', true, `${m.id} (${m.pluginType})`)
    } catch {
      add('当前目录是插件工程', false, 'plugin.json 解析失败')
    }
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
  console.log(`\n[bedcode-plugin] doctor 完成: ${checks.length - failed}/${checks.length} 通过`)
  if (failed) process.exit(1)
}

// ==================== 入口 ====================

function main() {
  const { positional, flags } = parseArgs(process.argv.slice(2))
  const [cmd] = positional
  const rest = positional.slice(1)
  const pkg = readJson(join(SDK_ROOT, 'package.json'), 'SDK package.json')

  if (flags.version || flags.v) {
    console.log(pkg.version)
    process.exit(0)
  }

  if (flags.help || flags.h || !cmd) {
    console.log(`bedcode-plugin v${pkg.version}`)
    console.log('\nBedCode 移动端插件开发工具包\n')
    console.log('用法:')
    console.log('  bedcode-plugin create <id> <name> [--author <author>] [--dir <dir>] [--ts-only] [--registry]')
    console.log('  bedcode-plugin build [--resources-dir <dir>] [--frontend-only] [--rust-only] [--watch]')
    console.log('  bedcode-plugin package [-o <file>] [--hash]   # --hash 写入 wasmHash 完整性校验')
    console.log('  bedcode-plugin dev [pluginDir] [--entry <file>] [--port <port>] [--host] [--open]   # 浏览器开发环境（HMR；--host 供手机访问）')
    console.log('  bedcode-plugin manifest [--check]   # 按源码自动填充 contributes/permissions')
    console.log('  bedcode-plugin validate [--dir <dir>]   # 校验 plugin.json 结构')
    console.log('  bedcode-plugin doctor   # 环境自检（Node/Rust/wasm32/dev-shell/SDK）')
    process.exit(0)
  }

  switch (cmd) {
    case 'create':
      cmdCreate(rest, flags)
      break
    case 'build':
      cmdBuild(flags)
      break
    case 'package':
      cmdPackage(flags)
      break
    case 'dev':
      cmdDev(rest, flags)
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
      console.error(`未知命令: ${cmd}（运行 bedcode-plugin --help 查看用法）`)
      process.exit(1)
  }
}

main()
