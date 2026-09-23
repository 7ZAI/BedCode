/**
 * manifest-validate — 桌面端 plugin.json 清单校验（可导入）
 *
 * 由两处共用，保证「CI/构建链里跑的就是 CLI 里那套规则」：
 * - `bedcode-plugin-desktop validate`（本包 CLI）
 * - `bedcode-desktop/scripts/plugin-build.js`（插件生产构建链，构建前拦截）
 *
 * 权限词汇不在此手抄：读 SDK 生成物 `bin/permission-vocabulary.json`
 * （真源 `rust/src/permission.rs`，重跑 SDK 的 `pnpm run gen:permissions`）。
 * `wasmHash` 的形态正则亦不手抄：取自 `bin/wasm-hash.js`（与产物注入、分发校验同一真源）。
 */

import { existsSync, readFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { WASM_HASH_PATTERN } from './wasm-hash.js'

const SDK_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const VOCABULARY_PATH = join(SDK_ROOT, 'bin', 'permission-vocabulary.json')

const VALID_PLUGIN_TYPES = new Set(['ts-only', 'rust-ts', 'rust'])

/** 权限词汇生成物（惰性读：缺文件时给出可操作的错误，而非 CLI 启动即崩） */
let vocabulary = null
export function permissionVocabulary() {
  if (vocabulary) return vocabulary
  let parsed
  try {
    parsed = JSON.parse(readFileSync(VOCABULARY_PATH, 'utf-8'))
  } catch (e) {
    throw new Error(
      `权限词汇生成物不可读: ${VOCABULARY_PATH} — 在 SDK 包目录跑 pnpm run gen:permissions 重出（${e.message}）`,
    )
  }
  if (!Array.isArray(parsed?.permissions)) {
    throw new Error(`权限词汇生成物形状不符（缺 permissions 数组）: ${VOCABULARY_PATH}`)
  }
  vocabulary = { permissions: new Set(parsed.permissions), apiMap: parsed.apiMap || {} }
  return vocabulary
}

/**
 * 校验插件目录的 plugin.json
 * @param {string} dir 插件工程目录
 * @returns {{manifest: object, errors: string[], warnings: string[]}} errors 非空即不合法
 */
export function validateManifest(dir) {
  const manifestPath = join(dir, 'plugin.json')
  const errors = []
  const warnings = []

  if (!existsSync(manifestPath)) {
    return { manifest: null, errors: [`缺少 plugin.json: ${dir}`], warnings }
  }

  let manifest
  try {
    manifest = JSON.parse(readFileSync(manifestPath, 'utf-8'))
  } catch (e) {
    return { manifest: null, errors: [`plugin.json 不是合法 JSON: ${e.message}`], warnings }
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

  // pluginType / sandbox / rustLibrary
  if (manifest.pluginType && !VALID_PLUGIN_TYPES.has(manifest.pluginType)) {
    errors.push(
      `pluginType 非法: "${manifest.pluginType}"（允许: ${[...VALID_PLUGIN_TYPES].join(' / ')}）`,
    )
  }
  if (manifest.sandbox && !['inline', 'isolated'].includes(manifest.sandbox)) {
    errors.push(`sandbox 非法: "${manifest.sandbox}"（允许: inline / isolated）`)
  }
  if (manifest.pluginType === 'rust-ts' && !manifest.rustLibrary) {
    errors.push('pluginType=rust-ts 时必须提供 rustLibrary（与 Cargo.toml 包名一致）')
  }

  // wasmHash（票 14）：形态非法即拒；**值由构建注入产物**，源清单带它属于误用——
  // manifest-gen 只回填 contributes/permissions，陈旧摘要会一路活到分发链（该链按字节复核后拒发）。
  if (manifest.wasmHash !== undefined && manifest.wasmHash !== null && manifest.wasmHash !== '') {
    if (typeof manifest.wasmHash !== 'string' || !WASM_HASH_PATTERN.test(manifest.wasmHash)) {
      errors.push(
        `wasmHash 形态非法（须为小写 64 位十六进制 SHA-256）: ${JSON.stringify(manifest.wasmHash)}`,
      )
    }
    warnings.push(
      'wasmHash 由构建链注入产物 plugin.json，源清单不必手写该键（写了也不会被刷新）',
    )
  }

  // ptyQuota（会话引擎下沉 P1 / H1）：可选，声明该插件可同时在册的 host-pty 句柄数。
  // 本处只校**形态**（正整数）——区间上限是内核常量
  // `PLUGIN_PTY_SESSIONS_CEILING_PER_PLUGIN`，在此复刻一份数字就是把配额判据拆成两处
  // （改内核常量不会同步到这里）；越界由宿主加载期拒绝（manager/validation.rs），
  // 构建期误写 0 / 小数 / 字符串则在这里当场拒绝，不必等到装包才暴露。
  if (manifest.ptyQuota !== undefined && manifest.ptyQuota !== null) {
    if (!Number.isInteger(manifest.ptyQuota) || manifest.ptyQuota <= 0) {
      errors.push(
        `ptyQuota 形态非法（须为正整数；上限由宿主内核常量仲裁）: ${JSON.stringify(manifest.ptyQuota)}`,
      )
    }
  }

  // 权限：SDK 词汇真源之外的声明会在授权时被静默过滤，等于装饰词汇 → 直接拒绝
  if (Array.isArray(manifest.permissions)) {
    const { permissions } = permissionVocabulary()
    const unknown = manifest.permissions.filter((p) => !permissions.has(p))
    if (unknown.length) {
      errors.push(
        `未知权限: ${unknown.join(', ')}（真源 SDK rust/src/permission.rs，声明未列入的权限会被宿主静默过滤）`,
      )
    }
  }

  // main 产物存在性（未构建仅警告）
  const distMain = join(dir, 'dist', manifest.main || 'index.js')
  if (!existsSync(distMain)) {
    warnings.push(`dist/${manifest.main || 'index.js'} 不存在 — 尚未构建（pnpm run build）`)
  }

  // contributes 结构
  if (manifest.contributes && typeof manifest.contributes !== 'object') {
    errors.push('contributes 必须是对象')
  }

  // contributes.httpEndpoints（票 16 路径白名单、票 08 追加认证档位）
  // 宿主**只认声明**：未声明路径 404，未声明清单等于没有 HTTP 面。条目两形态并存——
  // 纯路径段（档位 = 宿主最严缺省 jwt）或 `{ path, auth }`。条目非法会让声明侧
  // 「看起来有清单」而实际永远匹配不上，故构建期就判死；`auth` 取值真源是
  // rust/src/types.rs 的 EndpointAuth（none|jwt），写错的条目在宿主侧不登记（端点不可达）。
  const HTTP_ENDPOINT_AUTH_TIERS = ['none', 'jwt']
  const httpEndpoints = manifest.contributes?.httpEndpoints
  if (httpEndpoints !== undefined && httpEndpoints !== null) {
    if (!Array.isArray(httpEndpoints)) {
      errors.push('contributes.httpEndpoints 必须是「路径段字符串」或「{path, auth} 对象」的数组')
    } else {
      for (const entry of httpEndpoints) {
        const isString = typeof entry === 'string'
        const isObject = entry !== null && typeof entry === 'object' && !Array.isArray(entry)
        if (!isString && !isObject) {
          errors.push(
            `contributes.httpEndpoints 条目形态非法（须为字符串或 {path, auth} 对象）: ${JSON.stringify(entry)}`,
          )
          continue
        }
        const path = isString ? entry : entry.path
        if (typeof path !== 'string' || path.trim() === '' || path.includes('..')) {
          errors.push(
            `contributes.httpEndpoints 条目 path 非法（须为非空相对路径段、不含 ..）: ${JSON.stringify(entry)}`,
          )
        }
        if (isString) continue
        const extraKeys = Object.keys(entry).filter((k) => k !== 'path' && k !== 'auth')
        if (extraKeys.length) {
          errors.push(
            `contributes.httpEndpoints 条目含未知字段（只允许 path / auth）: ${extraKeys.join(', ')} → ${JSON.stringify(entry)}`,
          )
        }
        if (entry.auth !== undefined && !HTTP_ENDPOINT_AUTH_TIERS.includes(entry.auth)) {
          errors.push(
            `contributes.httpEndpoints 条目 auth 取值非法（须为 ${HTTP_ENDPOINT_AUTH_TIERS.join(' | ')}，缺省即最严档 jwt）: ${JSON.stringify(entry)}`,
          )
        }
      }
      const normalized = httpEndpoints
        .map((entry) => (typeof entry === 'string' ? entry : entry?.path))
        .filter((p) => typeof p === 'string')
        .map((p) => p.trim().replace(/^\/+/, ''))
      const dup = normalized.filter((p, i) => normalized.indexOf(p) !== i)
      if (dup.length) {
        errors.push(`contributes.httpEndpoints 重复声明: ${[...new Set(dup)].join(', ')}`)
      }
    }
  }

  // wasiPreopenDirs（票 07 只读档）：条目两形态——裸路径字符串（可写，既有形态）
  // 或 `{ path, readonly }` 对象。这里管形态，宿主 Rust 端（SDK
  // rust/src/types.rs::WasiPreopenDir::from_json）管仲裁：未知键 / 非布尔 readonly
  // 若被静默忽略，只读声明会退化成可写挂载，所以两侧都不放过。
  const preopenDirs = manifest.wasiPreopenDirs
  if (preopenDirs !== undefined && preopenDirs !== null) {
    if (!Array.isArray(preopenDirs)) {
      errors.push('wasiPreopenDirs 必须是「路径字符串」或「{path, readonly} 对象」的数组')
    } else {
      for (const entry of preopenDirs) {
        const isString = typeof entry === 'string'
        const isObject = entry !== null && typeof entry === 'object' && !Array.isArray(entry)
        if (!isString && !isObject) {
          errors.push(
            `wasiPreopenDirs 条目形态非法（须为路径字符串或 {path, readonly} 对象）: ${JSON.stringify(entry)}`,
          )
          continue
        }
        const path = isString ? entry : entry.path
        if (typeof path !== 'string' || path.trim() === '') {
          errors.push(
            `wasiPreopenDirs 条目 path 非法（须为非空字符串，支持 \${home} 前缀）: ${JSON.stringify(entry)}`,
          )
        }
        if (isString) continue
        const extraKeys = Object.keys(entry).filter((k) => k !== 'path' && k !== 'readonly')
        if (extraKeys.length) {
          errors.push(
            `wasiPreopenDirs 条目含未知字段（只允许 path / readonly）: ${extraKeys.join(', ')} → ${JSON.stringify(entry)}`,
          )
        }
        if (entry.readonly !== undefined && typeof entry.readonly !== 'boolean') {
          errors.push(
            `wasiPreopenDirs 条目 readonly 必须是布尔（缺省即可写；写错不会静默降级）: ${JSON.stringify(entry)}`,
          )
        }
      }
    }
  }

  return { manifest, errors, warnings }
}
