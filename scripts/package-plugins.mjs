#!/usr/bin/env node
/**
 * package-plugins — 桌面端 + 移动端插件统一构建与打包脚本
 *
 * 功能：
 *   1. 按插件列表（默认 scripts/plugin-package-list.json）构建两端插件的前后端：
 *      - 前端：vite build（各插件 build 脚本串联，产物 dist/index.js）
 *      - 后端：cargo build wasm32 + Componentize 组件化（Component Model）
 *      构建委托给两端既有构建机制（desktop: manifest-gen + pnpm run build；
 *      mobile: scripts/plugin-build.js），行为与 CI 宿主构建一致。
 *   2. 收集产物（src-tauri/resources/plugins/{desktop,mobile}/<id>/）为每个插件各打一个 zip：
 *      - <out>/<target>/<id>.zip（zip 根 = 插件文件，与移动端 SDK package 分发格式一致），
 *        一个插件一个包，发布后按需单独安装/更新。
 *
 * 产物即 release 的独立附件（见 .github/workflows/release.yml 的 package-plugins job）。
 *
 * 用法：
 *   node scripts/package-plugins.mjs [options]
 *
 * 选项：
 *   --target <desktop|mobile|all>  打包目标端（默认 all）
 *   --config <path>                插件列表配置文件（默认 scripts/plugin-package-list.json），
 *                                  在此文件增删插件名即控制打包范围（添加/删除插件）
 *   --plugin <name|id>             额外追加打包插件，可重复；目录名或 plugin.json id，两端自动匹配
 *   --only <name|id>               忽略配置文件列表，只打包指定插件，可重复（与 --plugin 的区别：
 *                                  不叠加 config 中其余插件）
 *   --exclude <name|id>            从打包列表中排除，可重复
 *   --out <dir>                    输出目录（默认 dist/plugin-packages，相对仓库根）
 *   --version <ver>                zip 文件名版本号（默认读 bedcode-desktop/src-tauri/tauri.conf.json）
 *   --no-zip                       只构建收集产物，不打包 zip
 *   --skip-build                   跳过构建，直接打包 src-tauri/resources/plugins 已有产物
 *   --keep-stage                   保留中间 stage 目录（默认完成后清理）
 *   --list                         仅打印解析后的插件清单，不构建不打包
 *
 * 本地示例：
 *   node scripts/package-plugins.mjs --list
 *   node scripts/package-plugins.mjs --target desktop --only file-transfer
 *   node scripts/package-plugins.mjs --target all --exclude auto-task --version 2.1.0
 */

import { execSync } from 'node:child_process'
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { deflateRawSync } from 'node:zlib'
import { join, relative, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import { dirname } from 'node:path'
import { platform } from 'node:os'
// 桌面端 manifest 自动填充（与 bedcode-desktop/scripts/plugin-build.js 同源）
import { generateManifest } from '../bedcode-desktop/packages/plugin-sdk-desktop/bin/manifest-gen.js'

const __dirname = dirname(fileURLToPath(import.meta.url))
const ROOT = resolve(__dirname, '..')
const DESKTOP_ROOT = resolve(ROOT, 'bedcode-desktop')
const MOBILE_ROOT = resolve(ROOT, 'bedcode-mobile')
const DESKTOP_PLUGINS = resolve(DESKTOP_ROOT, 'plugins')
const MOBILE_PLUGINS = resolve(MOBILE_ROOT, 'plugins')
const DEFAULT_CONFIG = resolve(__dirname, 'plugin-package-list.json')
const IS_WIN = platform() === 'win32'

// ==================== 参数解析 ====================

const TARGETS = ['desktop', 'mobile', 'all']

function parseArgs(argv) {
  const args = {
    target: 'all',
    config: null,
    plugin: [],
    only: [],
    exclude: [],
    out: 'dist/plugin-packages',
    version: null,
    noZip: false,
    skipBuild: false,
    keepStage: false,
    list: false,
  }
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]
    if (!a.startsWith('--')) {
      console.error(`[package-plugins] 意外的位置参数: ${a}`)
      process.exit(1)
    }
    const eq = a.indexOf('=')
    let key, val
    if (eq !== -1) {
      key = a.slice(2, eq)
      val = a.slice(eq + 1)
    } else {
      key = a.slice(2)
      val = argv[i + 1] && !argv[i + 1].startsWith('--') ? argv[++i] : true
    }
    switch (key) {
      case 'target':
        if (!TARGETS.includes(val)) {
          console.error(`[package-plugins] --target 取值非法: ${val}（允许 ${TARGETS.join('|')}）`)
          process.exit(1)
        }
        args.target = val
        break
      case 'config':
        args.config = String(val)
        break
      case 'plugin':
        args.plugin.push(String(val))
        break
      case 'only':
        args.only.push(String(val))
        break
      case 'exclude':
        args.exclude.push(String(val))
        break
      case 'out':
        args.out = String(val)
        break
      case 'version':
        args.version = String(val)
        break
      case 'no-zip':
        args.noZip = true
        break
      case 'skip-build':
        args.skipBuild = true
        break
      case 'keep-stage':
        args.keepStage = true
        break
      case 'list':
        args.list = true
        break
      default:
        console.error(`[package-plugins] 未知参数: --${key}`)
        process.exit(1)
    }
  }
  return args
}

const args = parseArgs(process.argv.slice(2))

// ==================== 工具函数 ====================

function readJson(path, what) {
  try {
    return JSON.parse(readFileSync(path, 'utf-8'))
  } catch (e) {
    console.error(`[package-plugins] 解析 ${what} 失败（${path}）: ${e.message}`)
    process.exit(1)
  }
}

function warn(msg) {
  console.warn(`[package-plugins] 警告: ${msg}`)
}

/** 执行命令（stdio 透传，失败即终止）；Windows 下 pnpm 需 pnpm.cmd */
function toExecutable(c) {
  return IS_WIN && c === 'pnpm' ? 'pnpm.cmd' : c
}
function run(cmdArray, { cwd, label }) {
  const cmd = cmdArray.map(toExecutable)
  console.log(`\n[package-plugins] ${label || cmd.join(' ')}\n  $ ${cmd.join(' ')}  (cwd=${cwd})`)
  try {
    execSync(cmd.join(' '), { cwd, stdio: 'inherit' })
  } catch (e) {
    console.error(`[package-plugins] 命令失败（${label || cmd.join(' ')}）: ${e.message}`)
    process.exit(1)
  }
}

/** 递归收集目录下所有文件（绝对路径） */
function walk(dir) {
  const out = []
  for (const d of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, d.name)
    if (d.isDirectory()) out.push(...walk(full))
    else out.push(full)
  }
  return out
}

function fmtSize(n) {
  return n > 1024 * 1024 ? `${(n / 1024 / 1024).toFixed(1)} MB` : `${(n / 1024).toFixed(1)} KB`
}

// ==================== 插件列表解析 ====================

/**
 * 解析单个插件名（目录名或 plugin.json id）在指定端的实际目录。
 * 优先目录名直配，其次扫描所有插件目录按 manifest.id 匹配。
 */
function resolvePlugin(target, name) {
  const pluginsDir = target === 'desktop' ? DESKTOP_PLUGINS : MOBILE_PLUGINS
  const dirHit = resolve(pluginsDir, name)
  if (existsSync(join(dirHit, 'plugin.json'))) {
    return { dir: name, id: readJson(join(dirHit, 'plugin.json'), `插件 ${name} 的 plugin.json`).id }
  }
  for (const d of readdirSync(pluginsDir, { withFileTypes: true })) {
    if (!d.isDirectory() || d.name.startsWith('.') || d.name.startsWith('_')) continue
    const manifestPath = join(pluginsDir, d.name, 'plugin.json')
    if (!existsSync(manifestPath)) continue
    let m
    try {
      m = JSON.parse(readFileSync(manifestPath, 'utf-8'))
    } catch {
      continue
    }
    if (m.id === name) return { dir: d.name, id: m.id }
  }
  return null
}

/** 解析打包插件清单：--only（忽略配置）或 默认配置文件 + --plugin 追加，再 --exclude 排除 + --target 过滤 */
function loadPlugins() {
  // 1. 插件名集合：--only 优先（忽略 config），否则默认配置文件
  //    （支持 {desktop:[], mobile:[]} 或平铺数组——两端同名匹配）
  const list = { desktop: new Set(), mobile: new Set() }
  if (args.only.length > 0) {
    for (const name of args.only) {
      list.desktop.add(name)
      list.mobile.add(name)
    }
  } else {
    const configPath = args.config ? resolve(ROOT, args.config) : DEFAULT_CONFIG
    const raw = readJson(configPath, '插件列表配置文件')
    if (Array.isArray(raw)) {
      for (const name of raw) {
        if (typeof name === 'string' && name.trim()) {
          list.desktop.add(name)
          list.mobile.add(name)
        }
      }
    } else {
      for (const t of ['desktop', 'mobile']) {
        for (const name of raw[t] || []) {
          if (typeof name === 'string' && name.trim()) list[t].add(name)
        }
      }
    }
    // 2. --plugin 追加（同名插件两端都尝试，resolve 后去重）
    for (const name of args.plugin) {
      list.desktop.add(name)
      list.mobile.add(name)
    }
  }
  // 3. --exclude 排除（按目录名或 id 删除）
  for (const name of args.exclude) {
    list.desktop.delete(name)
    list.mobile.delete(name)
  }
  // 4. 解析为具体插件条目
  const plugins = []
  for (const t of ['desktop', 'mobile']) {
    if (args.target !== 'all' && args.target !== t) continue
    for (const name of list[t]) {
      const hit = resolvePlugin(t, name)
      if (hit) plugins.push({ target: t, ...hit })
      else warn(`插件「${name}」在 ${t} 端不存在，已跳过（--config / --plugin 名应为插件目录名或 plugin.json id）`)
    }
  }
  // 按 target 去重（--plugin 与配置文件可能重复指定）
  const seen = new Set()
  return plugins.filter((p) => {
    const key = `${p.target}/${p.id}`
    if (seen.has(key)) return false
    seen.add(key)
    return true
  })
}

const plugins = loadPlugins()

if (plugins.length === 0) {
  console.error('[package-plugins] 无插件可打包（检查 scripts/plugin-package-list.json 或 --plugin 参数）')
  process.exit(1)
}

if (args.list) {
  console.log('[package-plugins] 打包插件清单：')
  for (const p of plugins) {
    console.log(`  ${p.target.padEnd(8)} ${p.id}  (dir: plugins/${p.dir})`)
  }
  process.exit(0)
}

// ==================== 构建 ====================

function buildDesktopPlugin({ dir, id }) {
  const pluginDir = resolve(DESKTOP_PLUGINS, dir)
  // manifest 自动填充（与 bedcode-desktop/scripts/plugin-build.js 同源，保证产物与源码一致）
  try {
    const { changed, report } = generateManifest(pluginDir)
    if (changed) for (const line of report) console.log(`  [manifest] ${line}`)
    else console.log('  [manifest] plugin.json 已是最新，无需更新')
  } catch (e) {
    console.error(`[package-plugins] ${id} manifest 自动填充失败: ${e.message}`)
    process.exit(1)
  }
  // 插件 build 脚本 = vite build + cargo wasm + componentize + 复制到 resources
  run(['pnpm', 'run', 'build'], { cwd: pluginDir, label: `构建桌面端插件 ${id}` })
}

function buildMobilePlugin({ id }) {
  // 移动端既有批量脚本：SDK CLI（bedcode-plugin build --resources-dir）+ dev 副本刷新
  run(['node', 'scripts/plugin-build.js', '--plugin', id], { cwd: MOBILE_ROOT, label: `构建移动端插件 ${id}` })
}

if (!args.skipBuild) {
  console.log(`\n========== 开始构建 ${plugins.length} 个插件（前端 + WASM 后端） ==========`)
  for (const p of plugins) {
    console.log(`\n========== [${p.target}] ${p.id} ==========`)
    if (p.target === 'desktop') buildDesktopPlugin(p)
    else buildMobilePlugin(p)
  }
} else {
  console.log('\n[package-plugins] --skip-build：跳过构建，直接打包已有产物')
}

// ==================== 收集产物 ====================

const outDir = resolve(ROOT, args.out)
const stageDir = join(outDir, 'stage')
const version = args.version || readJson(join(DESKTOP_ROOT, 'src-tauri/tauri.conf.json'), 'tauri.conf.json').version

function collectArtifacts() {
  for (const p of plugins) {
    const src = resolve(ROOT, `bedcode-${p.target}/src-tauri/resources/plugins/${p.target}/${p.id}`)
    const dst = join(stageDir, p.target, p.id)
    if (!existsSync(src)) {
      console.error(`[package-plugins] 产物缺失: ${src}\n  先运行构建（或确认去掉 --skip-build）`)
      process.exit(1)
    }
    rmSync(dst, { recursive: true, force: true })
    mkdirSync(dst, { recursive: true })
    cpSync(src, dst, { recursive: true })
  }
}

// ==================== ZIP 打包 ====================

// CRC32（zip 规范校验和；不依赖 Node 版本内置的 zlib.crc32）
const CRC_TABLE = (() => {
  const t = new Uint32Array(256)
  for (let n = 0; n < 256; n++) {
    let c = n
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
    t[n] = c >>> 0
  }
  return t
})()

function crc32(buf) {
  let c = 0xffffffff
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8)
  return (c ^ 0xffffffff) >>> 0
}

/**
 * 构建 zip 二进制（store/deflate 自适应，目录条目显式写入）。
 * entries: [{ name: 'dir/...', data: Buffer }]；data 为 null/undefined 时写为目录条目。
 * 参考 bedcode-mobile SDK cli.js 的 buildZip，增强目录层级 + deflate 压缩。
 */
function buildZip(entries) {
  const chunks = []
  const central = []
  let offset = 0
  const now = new Date()
  const dosTime = (now.getHours() << 11) | (now.getMinutes() << 5) | (now.getSeconds() >> 1)
  const dosDate = ((now.getFullYear() - 1980) << 9) | ((now.getMonth() + 1) << 5) | now.getDate()

  for (const entry of entries) {
    const nameBuf = Buffer.from(entry.name, 'utf-8')
    const data = entry.data || Buffer.alloc(0)
    const crc = crc32(data)
    let payload = data
    let method = 0
    if (entry.data && data.length > 0) {
      const deflated = deflateRawSync(data, { level: 9 })
      if (deflated.length < data.length) {
        payload = deflated
        method = 8
      }
    }

    // 本地文件头
    const lh = Buffer.alloc(30)
    lh.writeUInt32LE(0x04034b50, 0) // signature
    lh.writeUInt16LE(20, 4) // version needed
    lh.writeUInt16LE(0, 6) // flags
    lh.writeUInt16LE(method, 8) // compression method
    lh.writeUInt16LE(dosTime, 10)
    lh.writeUInt16LE(dosDate, 12)
    lh.writeUInt32LE(crc, 14)
    lh.writeUInt32LE(payload.length, 18) // compressed size
    lh.writeUInt32LE(data.length, 22) // uncompressed size
    lh.writeUInt16LE(nameBuf.length, 26)
    lh.writeUInt16LE(0, 28) // extra len
    chunks.push(lh, nameBuf, payload)

    // 中央目录项
    const ch = Buffer.alloc(46)
    ch.writeUInt32LE(0x02014b50, 0) // signature
    ch.writeUInt16LE(20, 4) // version made by
    ch.writeUInt16LE(20, 6) // version needed
    ch.writeUInt16LE(0, 8) // flags
    ch.writeUInt16LE(method, 10)
    ch.writeUInt16LE(dosTime, 12)
    ch.writeUInt16LE(dosDate, 14)
    ch.writeUInt32LE(crc, 16)
    ch.writeUInt32LE(payload.length, 20)
    ch.writeUInt32LE(data.length, 24)
    ch.writeUInt16LE(nameBuf.length, 28)
    ch.writeUInt16LE(0, 30) // extra len
    ch.writeUInt16LE(0, 32) // comment len
    ch.writeUInt16LE(0, 34) // disk start
    ch.writeUInt16LE(0, 36) // internal attrs
    ch.writeUInt32LE(0, 38) // external attrs
    ch.writeUInt32LE(offset, 42) // local header offset
    central.push(ch, nameBuf)

    offset += lh.length + nameBuf.length + payload.length
  }

  const centralSize = central.reduce((sum, b) => sum + b.length, 0)
  const eocd = Buffer.alloc(22)
  eocd.writeUInt32LE(0x06054b50, 0) // signature
  eocd.writeUInt16LE(0, 4) // disk number
  eocd.writeUInt16LE(0, 6) // cd start disk
  eocd.writeUInt16LE(entries.length, 8) // entries on this disk
  eocd.writeUInt16LE(entries.length, 10) // total entries
  eocd.writeUInt32LE(centralSize, 12)
  eocd.writeUInt32LE(offset, 16) // cd offset
  eocd.writeUInt16LE(0, 20) // comment len

  return Buffer.concat([...chunks, ...central, eocd])
}

/** 从文件条目中补全目录条目（zip 内目录路径显式声明，兼容严格解压器） */
function withDirEntries(entries) {
  const out = []
  const seenDirs = new Set()
  const pushDir = (dir) => {
    if (!dir || seenDirs.has(dir)) return
    const parts = dir.split('/')
    for (let i = 1; i <= parts.length; i++) {
      const prefix = parts.slice(0, i).join('/') + '/'
      if (!seenDirs.has(prefix)) {
        seenDirs.add(prefix)
        out.push({ name: prefix, data: null })
      }
    }
  }
  for (const e of entries) {
    const idx = e.name.lastIndexOf('/')
    if (idx !== -1) pushDir(e.name.slice(0, idx))
    out.push(e)
  }
  return out
}

function toPosix(p) {
  return p.split(sep).join('/')
}

/** 读取 stage 下某插件目录，生成 zip 条目（name 以给定 prefix 打头） */
function pluginEntries(target, id, prefix = '') {
  const dir = join(stageDir, target, id)
  return walk(dir).map((f) => ({
    name: `${prefix}${toPosix(relative(dir, f))}`,
    data: readFileSync(f),
  }))
}

function makeZip(entries, outFile, label) {
  const buf = buildZip(withDirEntries(entries))
  mkdirSync(dirname(outFile), { recursive: true })
  writeFileSync(outFile, buf)
  console.log(`  ${label}: ${outFile} (${fmtSize(buf.length)}, ${entries.length} 文件)`)
}

function zipPlugins() {
  // 一个插件一个 zip：<out>/<target>/<id>.zip，zip 根 = 插件文件（与移动端 SDK package 分发格式一致）
  for (const p of plugins) {
    makeZip(
      pluginEntries(p.target, p.id),
      join(outDir, p.target, `${p.id}.zip`),
      `[${p.target}/${p.id}]`,
    )
  }
}

// ==================== 主流程 ====================

console.log(`\n[package-plugins] 打包版本: ${version}，共 ${plugins.length} 个插件`)
for (const p of plugins) console.log(`  - [${p.target}] ${p.id} (${p.dir})`)

collectArtifacts()
if (!args.noZip) zipPlugins()
else console.log('\n[package-plugins] --no-zip：跳过 zip 打包，仅收集产物')

if (!args.keepStage) {
  rmSync(stageDir, { recursive: true, force: true })
  console.log(`\n[package-plugins] 已清理中间 stage 目录: ${stageDir}`)
}

console.log(`\n[package-plugins] 完成。产物目录: ${outDir}`)
console.log('  - 插件包: <target>/<plugin-id>.zip（一个插件一个 zip）')
