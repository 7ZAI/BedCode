/**
 * wasm-hash — 桌面插件产物 WASM 内容摘要的生产者（审计票 14）
 *
 * 口径（用户裁决：只写产物）：源 `plugin.json` **不带** `wasmHash`、构建也不改写源文件；
 * 摘要在产物组装完成后由产物目录内的 `<rustLibrary>.wasm` 现算，写进**产物** `plugin.json`。
 * 因此「源 manifest ≡ 产物 manifest」这条既有口径收窄为「除注入的 `wasmHash` 外一致」，
 * 构建后 `git status` 仍然干净（产物目录本身被 .gitignore 忽略）。
 *
 * 与宿主校验端同源（`src-tauri/src/plugin/manager/downloader.rs`）：同一个文件
 * （`<rust_library>.wasm`）、同一算法（SHA-256）、同一形态（小写 64 位十六进制，宿主比对
 * 时忽略大小写）。两侧各有一条同一字节向量的已知答案测试把这份对齐钉住，改任一端即转红。
 */

import { createHash } from 'node:crypto'
import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

/** 宿主 manifest 字段 `wasm_hash` 的形态：小写 64 位十六进制 */
export const WASM_HASH_PATTERN = /^[0-9a-f]{64}$/

/** 产物目录内的清单文件名（与宿主 PLUGIN_MANIFEST_FILE 同值） */
export const ARTIFACT_MANIFEST_FILE = 'plugin.json'

/** 清单里 wasm 文件名的推导规则（与宿主 `{rust_library}.wasm` 同形） */
export function wasmFileName(manifest) {
  const lib = typeof manifest?.rustLibrary === 'string' ? manifest.rustLibrary.trim() : ''
  return lib ? `${lib}.wasm` : null
}

export function sha256Hex(data) {
  return createHash('sha256').update(data).digest('hex')
}

/**
 * 把 WASM 摘要注入产物 manifest。
 *
 * @param {string} artifactDir 产物目录（含 plugin.json 与 wasm）
 * @param {{log?: (msg: string) => void}} [opts] log 缺省静默（供打包链在校验前后复用本函数）
 * @returns {{injected: boolean, wasmFile: string | null, hash: string | null, reason: string}}
 * @throws 产物清单缺失/非法 JSON，或声明了 rustLibrary 却找不到对应 wasm 文件（fail-visible）
 */
export function injectWasmHash(artifactDir, { log = () => {} } = {}) {
  const manifestPath = join(artifactDir, ARTIFACT_MANIFEST_FILE)
  if (!existsSync(manifestPath)) {
    throw new Error(`产物目录缺少 ${ARTIFACT_MANIFEST_FILE}: ${manifestPath}`)
  }
  let manifest
  try {
    manifest = JSON.parse(readFileSync(manifestPath, 'utf-8'))
  } catch (e) {
    throw new Error(`产物 ${ARTIFACT_MANIFEST_FILE} 不是合法 JSON（${manifestPath}）: ${e.message}`)
  }

  const file = wasmFileName(manifest)
  if (!file) {
    log('无 rustLibrary 声明（纯前端插件），跳过 WASM 摘要注入')
    return { injected: false, wasmFile: null, hash: null, reason: 'no-rust-library' }
  }

  const wasmPath = join(artifactDir, file)
  if (!existsSync(wasmPath)) {
    throw new Error(
      `manifest 声明 rustLibrary=${manifest.rustLibrary} 但产物缺少 ${file}（${artifactDir}）——` +
        '宿主安装端同样会因缺文件拒装，请在构建脚本里先复制 wasm 再注入摘要',
    )
  }

  const hash = sha256Hex(readFileSync(wasmPath))
  if (manifest.wasmHash === hash) {
    log(`wasmHash 已是最新（${file} → ${hash.slice(0, 12)}…）`)
    return { injected: true, wasmFile: file, hash, reason: 'unchanged' }
  }

  manifest.wasmHash = hash
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, 'utf-8')
  log(`已注入 wasmHash（${file} → ${hash}）`)
  return { injected: true, wasmFile: file, hash, reason: 'written' }
}

/**
 * 校验产物 manifest 的 wasmHash 与实际字节一致（分发链用：构建被绕过、产物被手工替换都要转红）。
 *
 * @param {string} artifactDir 产物目录
 * @returns {{ok: true, hash: string} | {ok: false, error: string}}
 */
export function verifyWasmHash(artifactDir) {
  let manifest
  try {
    manifest = JSON.parse(readFileSync(join(artifactDir, ARTIFACT_MANIFEST_FILE), 'utf-8'))
  } catch (e) {
    return { ok: false, error: `产物清单不可读（${artifactDir}）: ${e.message}` }
  }

  const file = wasmFileName(manifest)
  if (!file) return { ok: true, hash: null }

  const declared = typeof manifest.wasmHash === 'string' ? manifest.wasmHash.trim() : ''
  if (!declared) {
    return {
      ok: false,
      error: `产物缺少 wasmHash（${artifactDir}）—— 重新构建插件（node scripts/build.js）以注入摘要`,
    }
  }
  if (!WASM_HASH_PATTERN.test(declared)) {
    return {
      ok: false,
      error: `wasmHash 形态非法（须为小写 64 位十六进制）: ${JSON.stringify(declared)}（${artifactDir}）`,
    }
  }

  const wasmPath = join(artifactDir, file)
  if (!existsSync(wasmPath)) return { ok: false, error: `产物缺少 ${file}（${artifactDir}）` }

  const actual = sha256Hex(readFileSync(wasmPath))
  if (actual !== declared) {
    return {
      ok: false,
      error:
        `产物 wasmHash 与实际字节不符（${file}）: manifest=${declared} actual=${actual}` +
        ' —— 产物被替换过或构建后未重出清单，请重新构建',
    }
  }
  return { ok: true, hash: actual }
}
