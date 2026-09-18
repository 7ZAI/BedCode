#!/usr/bin/env node
/**
 * package-sdks — 桌面端 + 移动端插件 SDK 统一构建与打包脚本
 *
 * 功能：
 *   1. 构建两端 SDK 的双栈产物：
 *      - 前端 TS 包：tsup 构建（dist/，SDK CLI / 插件 vite 插件引用入口）+ vitest 单测
 *      - Rust crate：宿主 target 编译检查（--rust-wasm 时追加 wasm32 guest feature 检查）
 *   2. 打包为发布产物：
 *      - npm tarball：pnpm pack（.tgz，可离线 pnpm add 安装）
 *      - crates.io tarball：cargo package --no-verify（.crate，逐 crate）
 *      - 每端再聚合为一个 zip（npm + crates + README + WIT 契约 + SHA256 清单），
 *        作为 release 单文件附件下载。
 *
 * 产物即 release 的独立附件（见 .github/workflows/release.yml 的 package-sdks job），
 * 与 scripts/package-plugins.mjs 输出的插件 zip 平级，按端分目录：
 *   dist/sdk-packages/<target>/<sdk>-<sdk-version>.zip + 原始 .tgz / .crate + SHA256SUMS
 *
 * 版本约定：产物以各 SDK 自身版本命名（package.json version，CI 强制与 Cargo.toml 一致），
 * 与应用版本无关（sdk-publish.yml 的 verify job 同样做 npm/cargo 一致性校验）。
 *
 * 用法：
 *   node scripts/package-sdks.mjs [options]
 *
 * 选项：
 *   --target <desktop|mobile|all>  打包目标端（默认 all）
 *   --out <dir>                    输出目录（默认 dist/sdk-packages，相对仓库根）
 *   --no-zip                       只收集原始产物（.tgz/.crate/SHA256SUMS），不聚合 zip
 *   --skip-build                   跳过构建与测试（tsup / vitest / cargo check），
 *                                  仅重新打包（pnpm pack / cargo package）
 *   --skip-npm                     跳过 TS 包（build/test/pack）
 *   --skip-rust                    跳过 Rust crate（check/package）
 *   --skip-tests                   构建时跳过 vitest 单测（仅 npm 侧生效）
 *   --rust-wasm                    追加 wasm32-unknown-unknown guest 编译检查
 *                                  （需已安装该 target：rustup target add wasm32-unknown-unknown）
 *   --keep-stage                   保留中间 stage 目录（默认完成后清理）
 *   --list                         仅打印两端 SDK 目录与版本，不构建不打包
 *
 * 本地示例：
 *   node scripts/package-sdks.mjs --list
 *   node scripts/package-sdks.mjs --target desktop
 *   node scripts/package-sdks.mjs --target all --skip-tests
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
import { createHash } from 'node:crypto'
import { deflateRawSync } from 'node:zlib'
import { dirname, join, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import { platform } from 'node:os'

const __dirname = dirname(fileURLToPath(import.meta.url))
const ROOT = resolve(__dirname, '..')
const DESKTOP_ROOT = resolve(ROOT, 'bedcode-desktop')
const MOBILE_ROOT = resolve(ROOT, 'bedcode-mobile')
const IS_WIN = platform() === 'win32'

// 两端 SDK 元信息：目录（相对对应端根）+ npm 包名 + 参与的 crate（manifest 相对 rust 根）
const SDK_TARGETS = {
  desktop: {
    dir: 'packages/plugin-sdk-desktop',
    npmName: '@binblink/bedcode-plugin-sdk-desktop',
    crates: [
      { manifest: 'rust/Cargo.toml', name: 'bedcode-plugin-api' },
      { manifest: 'rust-macros/Cargo.toml', name: 'bedcode-plugin-api-macros' },
    ],
  },
  mobile: {
    dir: 'packages/plugin-sdk-mobile',
    npmName: '@binblink/bedcode-plugin-sdk-mobile',
    crates: [{ manifest: 'rust/Cargo.toml', name: 'bedcode-plugin-api-mobile' }],
  },
}

// ==================== 参数解析 ====================

const TARGETS = ['desktop', 'mobile', 'all']

function parseArgs(argv) {
  const args = {
    target: 'all',
    out: 'dist/sdk-packages',
    noZip: false,
    skipBuild: false,
    skipNpm: false,
    skipRust: false,
    skipTests: false,
    rustWasm: false,
    keepStage: false,
    list: false,
  }
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]
    if (!a.startsWith('--')) {
      console.error(`[package-sdks] 意外的位置参数: ${a}`)
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
          console.error(`[package-sdks] --target 取值非法: ${val}（允许 ${TARGETS.join('|')}）`)
          process.exit(1)
        }
        args.target = val
        break
      case 'out':
        args.out = String(val)
        break
      case 'no-zip':
        args.noZip = true
        break
      case 'skip-build':
        args.skipBuild = true
        break
      case 'skip-npm':
        args.skipNpm = true
        break
      case 'skip-rust':
        args.skipRust = true
        break
      case 'skip-tests':
        args.skipTests = true
        break
      case 'rust-wasm':
        args.rustWasm = true
        break
      case 'keep-stage':
        args.keepStage = true
        break
      case 'list':
        args.list = true
        break
      default:
        console.error(`[package-sdks] 未知参数: --${key}`)
        process.exit(1)
    }
  }
  if (args.skipBuild) {
    // 跳过构建时不允许跳过打包步骤（否则无事可做）
    if (args.skipNpm && args.skipRust) {
      console.error('[package-sdks] --skip-build 与 --skip-npm + --skip-rust 同时使用将无事可做')
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
    console.error(`[package-sdks] 解析 ${what} 失败（${path}）: ${e.message}`)
    process.exit(1)
  }
}

/** 执行命令（stdio 透传，失败即终止）；Windows 下 pnpm 需 pnpm.cmd */
function toExecutable(c) {
  return IS_WIN && c === 'pnpm' ? 'pnpm.cmd' : c
}
function run(cmdArray, { cwd, label }) {
  const cmd = cmdArray.map(toExecutable)
  console.log(`\n[package-sdks] ${label || cmd.join(' ')}\n  $ ${cmd.join(' ')}  (cwd=${cwd})`)
  try {
    execSync(cmd.join(' '), { cwd, stdio: 'inherit' })
  } catch (e) {
    console.error(`[package-sdks] 命令失败（${label || cmd.join(' ')}）: ${e.message}`)
    process.exit(1)
  }
}

/** 递归收集目录下所有文件（绝对路径） */
function fmtSize(n) {
  return n > 1024 * 1024 ? `${(n / 1024 / 1024).toFixed(1)} MB` : `${(n / 1024).toFixed(1)} KB`
}

function sha256File(p) {
  return createHash('sha256').update(readFileSync(p)).digest('hex')
}

// ==================== 目标解析 ====================

function targets() {
  return Object.keys(SDK_TARGETS).filter((t) => args.target === 'all' || args.target === t)
}

function sdkRoot(t) {
  return resolve(t === 'desktop' ? DESKTOP_ROOT : MOBILE_ROOT, SDK_TARGETS[t].dir)
}

const outDir = resolve(ROOT, args.out)
const stageDir = join(outDir, 'stage')

if (args.list) {
  console.log('[package-sdks] SDK 打包清单：')
  for (const t of targets()) {
    const dir = sdkRoot(t)
    const pkg = readJson(join(dir, 'package.json'), `${t} SDK package.json`)
    const crates = SDK_TARGETS[t].crates.map((c) => {
      const toml = readFileSync(join(dir, c.manifest), 'utf-8')
      const v = toml.match(/^version\s*=\s*"([^"]+)"/m)?.[1] || '?'
      return `${c.name}@${v}`
    })
    console.log(`  ${t.padEnd(8)} ${pkg.name}@${pkg.version}  crates: ${crates.join(', ')}  (dir: ${dir})`)
  }
  process.exit(0)
}

// ==================== 构建 ====================

function assertDeps(t, dir) {
  // pnpm pack / build 需要依赖安装；desktop SDK 的 devDeps（tsup 等）安装在其自身
  // node_modules（pnpm 局部安装），也在 bedcode-desktop 根（workspace 提升）。
  // mobile SDK 是独立 workspace，依赖只在自身 node_modules。
  // 这里做哨兵检查，缺失时给出明确指引而非静默失败。
  const candidates = [
    resolve(dir, 'node_modules', '.bin', 'tsup'),
    ...(t === 'desktop' ? [resolve(DESKTOP_ROOT, 'node_modules', '.bin', 'tsup')] : []),
  ]
  if (!candidates.some((p) => existsSync(p))) {
    console.error(
      `[package-sdks] ${t} SDK 依赖未安装（找不到 tsup）:\n` +
        (t === 'desktop'
          ? '  在 bedcode-desktop 下执行 pnpm install --frozen-lockfile 后重试'
          : '  在 bedcode-mobile/packages/plugin-sdk-mobile 下执行 pnpm install --frozen-lockfile 后重试'),
    )
    process.exit(1)
  }
}

function buildNpm(t) {
  const dir = sdkRoot(t)
  assertDeps(t, dir)
  const label = `${t} SDK TS 包`
  const testCmd = args.skipTests ? null : ['pnpm', 'run', 'test:run']
  run(['pnpm', 'run', 'build'], { cwd: dir, label: `${label} 构建（tsup → dist）` })
  if (testCmd) run(testCmd, { cwd: dir, label: `${label} 单测（vitest run）` })
}

function buildRust(t) {
  const dir = sdkRoot(t)
  const labelPrefix = `${t} SDK Rust`
  for (const c of SDK_TARGETS[t].crates) {
    const manifest = join(dir, c.manifest)
    run(['cargo', 'check', '--manifest-path', manifest], { cwd: ROOT, label: `${labelPrefix} ${c.name} 编译检查（宿主 target）` })
    if (args.rustWasm) {
      // wasm guest 检查要求 crate 声明 [features] wasm；proc-macro crate（如 rust-macros）
      // 只在宿主编译、无需 guest 产物，未声明时跳过而非硬性 --features wasm 报错
      if (hasFeature(manifest, 'wasm')) {
        run(
          ['cargo', 'check', '--manifest-path', manifest, '--features', 'wasm', '--target', 'wasm32-unknown-unknown'],
          { cwd: ROOT, label: `${labelPrefix} ${c.name} 编译检查（wasm guest / wasm feature）` },
        )
      } else {
        console.log(`[package-sdks] ${labelPrefix} ${c.name} 未声明 wasm feature，跳过 wasm guest 检查`)
      }
    }
  }
}

/** Cargo.toml 是否声明 [features] 下的指定 feature（按名精确匹配，兼容带引号写法） */
function hasFeature(manifest, feature) {
  const text = readFileSync(manifest, 'utf8')
  // features 段终止于下一个行首 `[`（新 section 头）；逐行捕获避免被值里的 [] 截断
  const section = text.match(/^\[features\]\r?\n((?:^[^\[\r].*\r?\n?)*)/m)
  if (!section) return false
  const names = ['"' + feature + '"', feature]
  return names.some((n) => new RegExp(`^\\s*${n}\\s*=`, 'm').test(section[1]))
}

if (!args.skipBuild) {
  console.log(`\n========== 开始构建 SDK（${targets().join(' / ')}） ==========`)
  for (const t of targets()) {
    console.log(`\n========== [${t}] ${SDK_TARGETS[t].npmName} ==========`)
    if (!args.skipNpm) buildNpm(t)
    if (!args.skipRust) buildRust(t)
  }
} else {
  console.log('\n[package-sdks] --skip-build：跳过构建与测试，直接打包已有产物')
}

// ==================== 打包 ====================

/** pnpm pack 到指定目录，返回生成的 .tgz 绝对路径（按文件名后缀识别，忽略其他文件） */
function npmTarball(t, packDest) {
  const dir = sdkRoot(t)
  const before = new Set(
    readdirSync(packDest).filter((f) => f.endsWith('.tgz')),
  )
  run(['pnpm', 'pack', '--pack-destination', packDest], { cwd: dir, label: `${t} SDK npm pack` })
  const after = readdirSync(packDest).filter((f) => f.endsWith('.tgz'))
  const created = after.find((f) => !before.has(f))
  if (!created) {
    console.error(`[package-sdks] ${t} SDK pnpm pack 未产生 .tgz（packDest=${packDest}）`)
    process.exit(1)
  }
  return join(packDest, created)
}

/** cargo package --no-verify 输出 .crate；crate 版本从 Cargo.toml 读取（与 package.json 一致性校验） */
function crateTarball(t, c, expectedVersion) {
  const dir = sdkRoot(t)
  const manifest = join(dir, c.manifest)
  const toml = readFileSync(manifest, 'utf-8')
  const crateVersion = toml.match(/^version\s*=\s*"([^"]+)"/m)?.[1]
  if (!crateVersion) {
    console.error(`[package-sdks] ${t} SDK ${c.name} Cargo.toml 未解析到 version（${manifest}）`)
    process.exit(1)
  }
  if (crateVersion !== expectedVersion) {
    console.error(
      `[package-sdks] ${t} SDK 版本不一致: package.json=${expectedVersion}，${c.name} Cargo.toml=${crateVersion}`,
    )
    process.exit(1)
  }
  run(['cargo', 'package', '--no-verify', '--manifest-path', manifest], { cwd: ROOT, label: `${t} SDK ${c.name} cargo package` })
  // cargo 的 target 目录相对 manifest 所在目录（rust/ 或 rust-macros/），不是 SDK 根
  const crateFile = join(dirname(manifest), 'target', 'package', `${c.name}-${crateVersion}.crate`)
  if (!existsSync(crateFile)) {
    console.error(`[package-sdks] cargo package 产物缺失: ${crateFile}`)
    process.exit(1)
  }
  return crateFile
}

// 与 package-plugins.mjs 同源的 zip 实现（CRC32 + store/deflate 自适应 + 目录条目显式）
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

    const lh = Buffer.alloc(30)
    lh.writeUInt32LE(0x04034b50, 0)
    lh.writeUInt16LE(20, 4)
    lh.writeUInt16LE(0, 6)
    lh.writeUInt16LE(method, 8)
    lh.writeUInt16LE(dosTime, 10)
    lh.writeUInt16LE(dosDate, 12)
    lh.writeUInt32LE(crc, 14)
    lh.writeUInt32LE(payload.length, 18)
    lh.writeUInt32LE(data.length, 22)
    lh.writeUInt16LE(nameBuf.length, 26)
    lh.writeUInt16LE(0, 28)
    chunks.push(lh, nameBuf, payload)

    const ch = Buffer.alloc(46)
    ch.writeUInt32LE(0x02014b50, 0)
    ch.writeUInt16LE(20, 4)
    ch.writeUInt16LE(20, 6)
    ch.writeUInt16LE(0, 8)
    ch.writeUInt16LE(method, 10)
    ch.writeUInt16LE(dosTime, 12)
    ch.writeUInt16LE(dosDate, 14)
    ch.writeUInt32LE(crc, 16)
    ch.writeUInt32LE(payload.length, 20)
    ch.writeUInt32LE(data.length, 24)
    ch.writeUInt16LE(nameBuf.length, 28)
    ch.writeUInt16LE(0, 30)
    ch.writeUInt16LE(0, 32)
    ch.writeUInt16LE(0, 34)
    ch.writeUInt16LE(0, 36)
    ch.writeUInt32LE(0, 38)
    ch.writeUInt32LE(offset, 42)
    central.push(ch, nameBuf)

    offset += lh.length + nameBuf.length + payload.length
  }

  const centralSize = central.reduce((sum, b) => sum + b.length, 0)
  const eocd = Buffer.alloc(22)
  eocd.writeUInt32LE(0x06054b50, 0)
  eocd.writeUInt16LE(0, 4)
  eocd.writeUInt16LE(0, 6)
  eocd.writeUInt16LE(entries.length, 8)
  eocd.writeUInt16LE(entries.length, 10)
  eocd.writeUInt32LE(centralSize, 12)
  eocd.writeUInt32LE(offset, 16)
  eocd.writeUInt16LE(0, 20)

  return Buffer.concat([...chunks, ...central, eocd])
}

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

function makeZip(entries, outFile, label) {
  const buf = buildZip(withDirEntries(entries))
  mkdirSync(dirname(outFile), { recursive: true })
  writeFileSync(outFile, buf)
  console.log(`  ${label}: ${outFile} (${fmtSize(buf.length)}, ${entries.length} 文件)`)
}

/** 生成 SHA256SUMS（文件名 + 空格 + 哈希，unix sha256sum -c 兼容），返回文件路径 */
function writeChecksums(files, destDir) {
  const lines = files
    .map((f) => `${sha256File(f)}  ${basename(f)}`)
    .sort()
    .join('\n')
  const out = join(destDir, 'SHA256SUMS')
  writeFileSync(out, lines + '\n')
  return out
}

function basename(p) {
  return p.split(sep).pop()
}

// ==================== 主流程 ====================

console.log(`\n[package-sdks] 开始打包（target: ${args.target}）`)
for (const t of targets()) console.log(`  - [${t}] ${SDK_TARGETS[t].npmName}`)

mkdirSync(stageDir, { recursive: true })
const zips = []

for (const t of targets()) {
  const dir = sdkRoot(t)
  const pkg = readJson(join(dir, 'package.json'), `${t} SDK package.json`)
  const tgtStage = join(stageDir, t)
  const tgtOut = join(outDir, t)
  mkdirSync(tgtStage, { recursive: true })
  mkdirSync(tgtOut, { recursive: true })

  const collected = [] // 该端原始发布产物（绝对路径）：npm tgz + crates
  const filenames = []

  // 1. npm tarball
  if (!args.skipNpm) {
    const tgz = npmTarball(t, tgtStage)
    collected.push(tgz)
    filenames.push(basename(tgz))
    console.log(`  npm: ${basename(tgz)} (${fmtSize(tgz.length > 0 ? readFileSync(tgz).length : 0)})`)
  }

  // 2. crates（无 --skip-build 时已先 cargo check）
  if (!args.skipRust) {
    for (const c of SDK_TARGETS[t].crates) {
      const crate = crateTarball(t, c, pkg.version)
      collected.push(crate)
      filenames.push(basename(crate))
      console.log(`  crate: ${basename(crate)}`)
    }
  }

  if (collected.length === 0) {
    console.error(`[package-sdks] ${t} 端无可打包产物（--skip-npm 与 --skip-rust 不能同时生效）`)
    process.exit(1)
  }

  // 3. 副本落到最终目录（stage 内原始产物保留给 zip 打包）
  for (const f of collected) {
    cpSync(f, join(tgtOut, basename(f)))
  }
  // 4. SHA256 校验和
  writeChecksums(collected, tgtOut)

  // 5. 聚合 zip（npm + crates + README + WIT + 校验和 + 索引说明）
  const zipEntries = collected.map((f) => ({
    name: `artifacts/${basename(f)}`,
    data: readFileSync(f),
  }))
  const sdkDirName = `plugin-sdk-${t}`
  const readmeSrc = join(dir, 'README.md')
  const witSrc = join(dir, 'rust', 'wit', 'bedcode.wit')
  if (existsSync(readmeSrc)) zipEntries.push({ name: 'README.md', data: readFileSync(readmeSrc) })
  if (existsSync(witSrc)) zipEntries.push({ name: 'wit/bedcode.wit', data: readFileSync(witSrc) })

  // 索引说明（版本 + 产物清单 + sha256），便于 release 附件的消费方核对
  const sumsLines = collected.map((f) => `${sha256File(f)}  artifacts/${basename(f)}`).join('\n')
  const indexMd = [
    `# ${pkg.name} v${pkg.version}`,
    '',
    `BedCode ${t} 端插件 SDK 发布包（${new Date().toISOString()}）。`,
    '',
    '## 产物清单',
    '',
    '| 文件 | 来源 | 用途 |',
    '| --- | --- | --- |',
    ...collected.map((f) => {
      const name = basename(f)
      const isTgz = name.endsWith('.tgz')
      return `| \`artifacts/${name}\` | ${isTgz ? 'pnpm pack（npm registry）' : 'cargo package（crates.io）'} | ${isTgz ? 'npm 包（TS 前端 + CLI + template + dev-shell）' : 'Rust crate（WIT 契约绑定 / API trait 与宏）'} |`
    }),
    '',
    '## SHA256',
    '',
    '```',
    sumsLines,
    '```',
    '',
    '安装与使用见 SDK 目录 README.md（本包内已附）与 `docs/knowledge/sdk-publish.md`。',
    '',
  ].join('\n')
  zipEntries.push({ name: 'index.md', data: Buffer.from(indexMd, 'utf-8') })

  if (!args.noZip) {
    const zipFile = join(tgtOut, `${sdkDirName}-${pkg.version}.zip`)
    makeZip(zipEntries, zipFile, `[${t}] ${basename(zipFile)}`)
    zips.push(zipFile)
  }
}

if (!args.keepStage) {
  rmSync(stageDir, { recursive: true, force: true })
  console.log(`\n[package-sdks] 已清理中间 stage 目录: ${stageDir}`)
}

console.log(`\n[package-sdks] 完成。产物目录: ${outDir}`)
for (const t of targets()) {
  const tgtOut = join(outDir, t)
  console.log(`  ${t}: ${tgtOut}（.tgz / .crate / SHA256SUMS${args.noZip ? '' : ' / 聚合 zip'}）`)
}
if (zips.length > 0) console.log(`  聚合 zip: ${zips.join(', ')}`)