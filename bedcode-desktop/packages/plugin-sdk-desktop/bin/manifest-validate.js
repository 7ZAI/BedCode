/**
 * manifest-validate — 桌面端 plugin.json 清单校验（可导入）
 *
 * 由两处共用，保证「CI/构建链里跑的就是 CLI 里那套规则」：
 * - `bedcode-plugin-desktop validate`（本包 CLI）
 * - `bedcode-desktop/scripts/plugin-build.js`（插件生产构建链，构建前拦截）
 *
 * 权限词汇不在此手抄：读 SDK 生成物 `bin/permission-vocabulary.json`
 * （真源 `rust/src/permission.rs`，重跑 SDK 的 `pnpm run gen:permissions`）。
 */

import { existsSync, readFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

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

  // contributes.httpEndpoints（票 16：_http_endpoint 的路径白名单声明）
  // 空/缺省 = 未声明，宿主按前缀内 ANY 放行（既有插件零迁移）；一旦声明就必须是
  // 可用的相对路径段——条目非法会让声明侧「看起来有清单」而实际永远匹配不上。
  const httpEndpoints = manifest.contributes?.httpEndpoints
  if (httpEndpoints !== undefined && httpEndpoints !== null) {
    if (!Array.isArray(httpEndpoints)) {
      errors.push('contributes.httpEndpoints 必须是字符串数组')
    } else {
      const bad = httpEndpoints.filter(
        (p) => typeof p !== 'string' || p.trim() === '' || p.includes('..'),
      )
      if (bad.length) {
        errors.push(
          `contributes.httpEndpoints 条目非法（须为非空相对路径段、不含 ..）: ${bad.join(', ')}`,
        )
      }
      const normalized = httpEndpoints
        .filter((p) => typeof p === 'string')
        .map((p) => p.trim().replace(/^\/+/, ''))
      const dup = normalized.filter((p, i) => normalized.indexOf(p) !== i)
      if (dup.length) {
        errors.push(`contributes.httpEndpoints 重复声明: ${[...new Set(dup)].join(', ')}`)
      }
    }
  }

  return { manifest, errors, warnings }
}
