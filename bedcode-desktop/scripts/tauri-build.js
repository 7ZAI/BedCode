#!/usr/bin/env node

/**
 * tauri build 包装脚本：按环境解析 updater 签名密钥
 *
 * 背景：
 * tauri.conf.json 配置了 `bundle.createUpdaterArtifacts` 与 `plugins.updater.pubkey`，
 * 生成升级包时 Tauri 强制要求 TAURI_SIGNING_PRIVATE_KEY，否则报错
 * "A public key has been found, but no private key"。
 * 正式发布构建在 GitHub Actions 中通过仓库 Secret 注入密钥（见 .github/workflows/release.yml）。
 *
 * 密钥解析优先级：
 * 1. TAURI_SIGNING_PRIVATE_KEY 环境变量（CI 或手动设置），原样使用
 * 2. TAURI_SIGNING_PRIVATE_KEY_FILE 环境变量指向的密钥文件
 * 3. 项目根 .env 文件（已 gitignore）中的 TAURI_SIGNING_PRIVATE_KEY
 *    或 TAURI_SIGNING_PRIVATE_KEY_FILE（推荐，密钥文件存放在仓库外）
 * 4. 均未提供：通过 --config 覆盖 createUpdaterArtifacts=false，
 *    构建不含升级包的本地安装包（自动更新功能不受影响，只是本次产物不作为升级源）
 *
 * 注意：签名私钥必须与 tauri.conf.json 中的 pubkey 配对（正式发布密钥），
 * 否则客户端校验更新签名会失败。
 */

import { spawnSync } from 'child_process'
import { existsSync, readFileSync, readdirSync, renameSync } from 'fs'
import { join, dirname } from 'path'
import { fileURLToPath } from 'url'
import { createRequire } from 'module'

const projectRoot = join(dirname(fileURLToPath(import.meta.url)), '..')
// 直接调用本地 tauri CLI 入口，避免经 shell 转发导致参数引号丢失（Windows cmd 会吞掉 JSON 中的双引号）
const require = createRequire(import.meta.url)
const tauriCli = require.resolve('@tauri-apps/cli/tauri.js')

/**
 * 从 .env 文件中提取 TAURI_SIGNING_* 变量
 * .env 已由 .gitignore 排除，仅支持单行 KEY=VALUE 格式（私钥为单行 base64）
 */
function loadSigningEnvFromFile() {
  const envPath = join(projectRoot, '.env')
  if (!existsSync(envPath)) return {}
  const result = {}
  for (const line of readFileSync(envPath, 'utf8').split(/\r?\n/)) {
    const match = line.match(/^\s*(TAURI_SIGNING_[A-Z_]+)\s*=\s*(.*)\s*$/)
    if (!match) continue
    result[match[1]] = match[2].replace(/^(['"])(.*)\1$/, '$2')
  }
  return result
}

/**
 * 解析签名密钥，返回注入的环境变量；无法解析时返回 null
 */
function resolveSigningEnv() {
  if (process.env.TAURI_SIGNING_PRIVATE_KEY) {
    return {}
  }

  const fromFile = loadSigningEnvFromFile()
  const keyFile = process.env.TAURI_SIGNING_PRIVATE_KEY_FILE || fromFile.TAURI_SIGNING_PRIVATE_KEY_FILE
  if (keyFile) {
    if (!existsSync(keyFile)) {
      console.error(`[tauri-build] TAURI_SIGNING_PRIVATE_KEY_FILE 指向的文件不存在: ${keyFile}`)
      process.exit(1)
    }
    // fromFile 中的 TAURI_SIGNING_PRIVATE_KEY_PASSWORD 等变量一并透传
    return { ...fromFile, TAURI_SIGNING_PRIVATE_KEY: readFileSync(keyFile, 'utf8') }
  }

  if (fromFile.TAURI_SIGNING_PRIVATE_KEY) {
    return fromFile
  }

  return null
}

const signingEnv = resolveSigningEnv()
const extraArgs = []

if (signingEnv) {
  console.log('[tauri-build] 已检测到 updater 签名密钥，构建将生成签名的升级包')
} else {
  console.warn('[tauri-build] 未检测到 updater 签名密钥（TAURI_SIGNING_PRIVATE_KEY / TAURI_SIGNING_PRIVATE_KEY_FILE / .env）')
  console.warn('[tauri-build] 本次构建禁用升级包生成（createUpdaterArtifacts=false），产物不含自动更新签名')
  extraArgs.push('--config', JSON.stringify({ bundle: { createUpdaterArtifacts: false } }))
}

/**
 * 转义正则特殊字符，用于以字面量匹配 productName/version
 */
function escapeRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/**
 * NSIS 安装包重命名为带 release 标记的格式
 *
 * Tauri 2 打包器固定用 {productName}_{version}_{arch}-setup.exe 命名（BundleConfig
 * 无 fileName 字段），与移动端 APK 的 {name}-{version}-release.apk 风格不一致。
 * 本步骤在构建成功后重命名安装包及其 .sig（签名覆盖文件内容、与文件名无关，同步改名即可）。
 * CI（GitHub Actions）走 tauri-action 发布链路不经本脚本，重命名仅影响本地/自定义构建产物。
 */
function renameInstallerWithReleaseSuffix() {
  if (process.env.GITHUB_ACTIONS) return

  let productName, version
  try {
    const config = JSON.parse(readFileSync(join(projectRoot, 'src-tauri/tauri.conf.json'), 'utf8'))
    productName = config.productName
    version = config.version
  } catch (err) {
    console.warn(`[tauri-build] 读取 tauri.conf.json 失败，跳过安装包重命名: ${err.message}`)
    return
  }

  const nsisDir = join(projectRoot, 'src-tauri/target/release/bundle/nsis')
  if (!existsSync(nsisDir)) return

  const pattern = new RegExp(`^${escapeRegExp(productName)}_${escapeRegExp(version)}_(\\w+)-setup\\.exe$`)
  for (const file of readdirSync(nsisDir)) {
    const match = file.match(pattern)
    if (!match) continue
    const arch = match[1]
    const renamed = `${productName}-${version}-release-${arch}-setup.exe`
    for (const suffix of ['', '.sig']) {
      const from = join(nsisDir, file + suffix)
      const to = join(nsisDir, renamed + suffix)
      if (!existsSync(from)) continue
      try {
        renameSync(from, to)
        console.log(`[tauri-build] 安装包已重命名: ${file}${suffix} -> ${renamed}${suffix}`)
      } catch (err) {
        console.warn(`[tauri-build] 重命名 ${file}${suffix} 失败: ${err.message}`)
      }
    }
  }
}

const args = process.argv.slice(2)
const result = spawnSync(process.execPath, [tauriCli, 'build', ...extraArgs, ...args], {
  stdio: 'inherit',
  env: { ...process.env, ...signingEnv },
})

const status = result.status ?? 1
if (status === 0) {
  renameInstallerWithReleaseSuffix()
}
process.exit(status)
