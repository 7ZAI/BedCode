#!/usr/bin/env node

/**
 * 基准夹具打包 —— 把 `packages/plugin-bench-test` 编成可 zip 安装的 wasm 应用包
 *
 * 供 e2e webview 层使用（`e2e/specs/bench.spec.ts`）：真实应用里没有 bench 夹具，
 * 基准要能在**真 webview** 里跑，就得按生产路径把它装进去 ——
 * `plugin_install_from_file`（zip 安装器）+ `plugin_approve`（ADR 0020 审批门禁）。
 *
 * 产物：`bench/dist/com.bedcode.bench.zip`（含 plugin.json / wasm / index.js）
 *
 * 摘要口径：与打包链同源（`packages/plugin-sdk-desktop/bin/wasm-hash.js` 的
 * `sha256Hex`）——宿主安装期会比对 zip 内 wasm 的 SHA-256（`downloader.rs`），
 * 摘要缺失会被当作「不校验」而放行，故这里显式注入。
 *
 * 用法：
 *   node bench/scripts/build-bench-zip.mjs           # 产物最新则跳过编译
 *   node bench/scripts/build-bench-zip.mjs --force   # 强制重编
 */

import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const DESKTOP = resolve(__dirname, '../..')
const FIXTURE_DIR = join(DESKTOP, 'packages/plugin-bench-test')
const OUT_DIR = join(DESKTOP, 'bench/dist')
const OUT_ZIP = join(OUT_DIR, 'com.bedcode.bench.zip')

/** wasip3 工具链 pin（与 `runtime.rs::WASIP3_NIGHTLY` / bench support.rs 同值） */
const WASIP3_TOOLCHAIN = process.env.BENCH_WASIP3_TOOLCHAIN ?? 'nightly-2026-09-16'
/** 夹具共享 target 目录（与 `fixture_target.rs::dir()` 同目录） */
const TARGET_DIR = join(DESKTOP, 'target/fixtures')
const LIB_NAME = 'bedcode_plugin_bench_test'
const ARTIFACT = join(TARGET_DIR, 'wasm32-wasip3/release', `${LIB_NAME}.wasm`)

const force = process.argv.includes('--force')

/** 唯一出口：失败写 stderr 并退非 0（脚本内不散落 console 调用） */
function fail(message, error) {
  process.stderr.write(`[bench-zip] ${message}\n`)
  if (error) process.stderr.write(`[bench-zip] ${error instanceof Error ? error.stack : String(error)}\n`)
  process.exit(1)
}

function note(message) {
  process.stdout.write(`[bench-zip] ${message}\n`)
}

/** 读夹具 manifest（非法 JSON 是打包配置错误，直接失败，不静默跳过） */
function readManifest() {
  const raw = readFileSync(join(FIXTURE_DIR, 'plugin.json'), 'utf8')
  try {
    return JSON.parse(raw)
  } catch (error) {
    fail('plugin.json 解析失败', error)
  }
}

/** 源码/SDK/WIT 任一较新则重编（与宿主内联 fixture 构建器同口径） */
function needsBuild() {
  if (force || !existsSync(ARTIFACT)) return true
  const built = statSync(ARTIFACT).mtimeMs
  const watch = [
    join(FIXTURE_DIR, 'src/lib.rs'),
    join(FIXTURE_DIR, 'plugin.json'),
    join(FIXTURE_DIR, 'Cargo.toml'),
    join(DESKTOP, 'packages/plugin-sdk-desktop/rust/wit/bedcode.wit'),
    join(DESKTOP, 'packages/plugin-sdk-desktop/rust/src/wasm_host.rs'),
  ]
  return watch.some((f) => !existsSync(f) || statSync(f).mtimeMs > built)
}

function buildFixture() {
  note(`编译夹具（wasm32-wasip3/release, toolchain=${WASIP3_TOOLCHAIN}）…`)
  try {
    execFileSync(
      'cargo',
      ['build', '--target', 'wasm32-wasip3', '--release', '--manifest-path', join(FIXTURE_DIR, 'Cargo.toml')],
      { stdio: 'inherit', env: { ...process.env, RUSTUP_TOOLCHAIN: WASIP3_TOOLCHAIN, CARGO_TARGET_DIR: TARGET_DIR } },
    )
  } catch (error) {
    fail('夹具编译失败', error)
  }
}

function packZip(staging) {
  rmSync(OUT_ZIP, { force: true })
  try {
    execFileSync('zip', [
      '-r', '-q', '-j', OUT_ZIP,
      join(staging, 'plugin.json'),
      join(staging, `${LIB_NAME}.wasm`),
      join(staging, 'index.js'),
    ])
  } catch (error) {
    fail('zip 打包失败（需要系统 zip 命令）', error)
  }
}

try {
  if (needsBuild()) buildFixture()
  else note('夹具产物已最新，跳过编译')

  const wasm = readFileSync(ARTIFACT)
  const manifest = readManifest()
  manifest.wasmHash = createHash('sha256').update(wasm).digest('hex')

  mkdirSync(OUT_DIR, { recursive: true })
  const staging = join(OUT_DIR, 'staging/com.bedcode.bench')
  mkdirSync(staging, { recursive: true })
  writeFileSync(join(staging, 'plugin.json'), `${JSON.stringify(manifest, null, 2)}\n`)
  writeFileSync(join(staging, `${LIB_NAME}.wasm`), wasm)
  writeFileSync(join(staging, 'index.js'), '// bench fixture: e2e 层只跑命令面，前端为空壳\n')

  packZip(staging)
  note(`就绪：${OUT_ZIP}（wasm ${(wasm.length / 1024).toFixed(0)} KiB, sha256=${manifest.wasmHash.slice(0, 12)}…）`)
} catch (error) {
  fail('打包中断', error)
}
