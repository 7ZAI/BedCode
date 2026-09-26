#!/usr/bin/env node

/**
 * 检查并清理 Rust target 目录
 *
 * 功能：
 * - 检查 src-tauri/target 目录大小
 * - 超过阈值时自动执行 cargo clean
 * - 防止增量编译缓存无限增长
 */

import { execSync } from 'child_process'
import { existsSync, readdirSync } from 'fs'
import { join } from 'path'

// 配置
const CONFIG = {
  // target 目录最大允许大小 (GB)
  // 阈值 15GB：与 AGENTS.md「构建前检查 src-tauri/target 目录大小，超过 15GB 执行 cargo clean」一致。
  // 历史值 10GB 已不适用——desktop 完整增量缓存（wasmtime/actix 等）实测 ~15~20GB，
  // 阈值过低会误清缓存反而拖慢增量构建
  maxSizeGB: 15,
  // 宿主 target 目录（唯一受阈值约束 + 自动 cargo clean 的目录）
  targetDir: join(process.cwd(), 'src-tauri', 'target'),
  // 是否自动清理 (设为 false 仅警告)
  autoClean: true,
  // 共享 target 目录（相对包根）：夹具与 wasm 应用刻意收敛的单一落点，
  // 路径真源见 .scratch/2026-09-26-cargo-target-space/spec.md。只报告不自动删——
  // 删掉等于丢掉共享编译缓存（下次跑测试会重编整份依赖图）
  sharedTargetDirs: ['target/fixtures', 'target/wasm-apps'],
  // 遗留的 per-crate target 目录（改造前的独立落点）：报告时标注可删
  legacyTargetParents: ['packages', 'wasm-apps'],
}

/**
 * 获取目录大小 (字节)
 */
function getDirectorySize(dirPath) {
  if (!existsSync(dirPath)) {
    return 0
  }

  try {
    // 使用 du 命令获取目录大小 (Linux/macOS)
    if (process.platform !== 'win32') {
      const output = execSync(`du -sb "${dirPath}" 2>/dev/null`, {
        encoding: 'utf-8',
      })
      return parseInt(output.split('\t')[0], 10)
    }

    // Windows: 使用 PowerShell
    const output = execSync(
      `powershell -Command "(Get-ChildItem -Path '${dirPath}' -Recurse | Measure-Object -Property Length -Sum).Sum"`,
      { encoding: 'utf-8' },
    )
    return parseInt(output.trim(), 10)
  } catch (error) {
    console.warn('无法获取目录大小:', error.message)
    return 0
  }
}

/**
 * 格式化文件大小
 */
function formatSize(bytes) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let size = bytes
  let unitIndex = 0

  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024
    unitIndex++
  }

  return `${size.toFixed(2)} ${units[unitIndex]}`
}

/**
 * 执行 cargo clean
 */
function cargoClean() {
  console.log('\n🧹 正在清理 target 目录...')
  try {
    execSync('cargo clean', {
      cwd: join(process.cwd(), 'src-tauri'),
      stdio: 'inherit',
    })
    console.log('✅ target 目录已清理\n')
  } catch (error) {
    console.error('❌ 清理失败:', error.message)
    process.exit(1)
  }
}

/**
 * 枚举除宿主外的其它 target 目录
 *
 * 本仓库无根 workspace（30+ 个独立 Cargo.toml），历史上每个 crate 各写一份
 * `target/`，整仓 Rust 产物一度达 15.3G。现已收敛为宿主 + 两个共享目录，
 * 但改造前残留的 per-crate 目录可能仍在盘上——列出来供人工判断删除。
 */
function collectOtherTargetDirs() {
  const found = []

  for (const rel of CONFIG.sharedTargetDirs) {
    const dir = join(process.cwd(), rel)
    if (existsSync(dir)) {
      found.push({ rel, dir, size: getDirectorySize(dir), kind: 'shared' })
    }
  }

  for (const parent of CONFIG.legacyTargetParents) {
    const parentDir = join(process.cwd(), parent)
    if (!existsSync(parentDir)) continue
    for (const entry of readdirSync(parentDir, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue
      for (const suffix of ['/target', '/rust/target']) {
        const rel = `${parent}/${entry.name}${suffix}`
        const dir = join(process.cwd(), rel)
        if (existsSync(dir)) {
          found.push({ rel, dir, size: getDirectorySize(dir), kind: 'legacy' })
        }
      }
    }
  }

  return found.sort((a, b) => b.size - a.size)
}

/**
 * 主函数
 */
function main() {
  console.log('📦 检查 target 目录大小...\n')

  const sizeBytes = getDirectorySize(CONFIG.targetDir)
  const sizeGB = sizeBytes / (1024 * 1024 * 1024)

  if (sizeBytes === 0) {
    console.log('✅ 宿主 target 目录不存在或为空\n')
  } else {
    console.log(`📊 宿主 target 目录: ${formatSize(sizeBytes)} (${sizeGB.toFixed(2)} GB)`)
    console.log(`📋 阈值限制: ${CONFIG.maxSizeGB} GB\n`)

    if (sizeGB > CONFIG.maxSizeGB) {
      console.log(`⚠️  警告: 宿主 target 目录已超过 ${CONFIG.maxSizeGB} GB!`)

      if (CONFIG.autoClean) {
        cargoClean()
      } else {
        console.log('💡 建议运行: pnpm run target:clean\n')
        process.exit(1)
      }
    } else {
      console.log('✅ 宿主 target 目录大小正常\n')
    }
  }

  // ==================== 共享 / 遗留 target 目录（只报告） ====================
  const others = collectOtherTargetDirs()
  if (others.length === 0) return

  console.log('📦 其它 target 目录（共享编译落点 / 改造前残留）:\n')
  for (const { rel, size, kind } of others) {
    const tag = kind === 'shared' ? '共享' : '遗留'
    const note = kind === 'shared' ? '保留（删除会丢失共享编译缓存）' : '可安全删除'
    console.log(`  [${tag}] ${rel} — ${formatSize(size)}  ${note}`)
  }
  const legacyBytes = others.filter((d) => d.kind === 'legacy').reduce((a, d) => a + d.size, 0)
  if (legacyBytes > 0) {
    console.log(`\n💡 遗留目录可回收 ${formatSize(legacyBytes)}（rm -rf 后重建为共享目录）`)
  }
  console.log()
}

main()
