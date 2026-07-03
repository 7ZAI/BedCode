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
import { existsSync, statSync } from 'fs'
import { join } from 'path'

// 配置
const CONFIG = {
  // target 目录最大允许大小 (GB)
  maxSizeGB: 10,
  // target 目录路径
  targetDir: join(process.cwd(), 'src-tauri', 'target'),
  // 是否自动清理 (设为 false 仅警告)
  autoClean: true,
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
      { encoding: 'utf-8' }
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
 * 主函数
 */
function main() {
  console.log('📦 检查 target 目录大小...\n')

  const sizeBytes = getDirectorySize(CONFIG.targetDir)
  const sizeGB = sizeBytes / (1024 * 1024 * 1024)

  if (sizeBytes === 0) {
    console.log('✅ target 目录不存在或为空\n')
    return
  }

  console.log(`📊 target 目录大小: ${formatSize(sizeBytes)} (${sizeGB.toFixed(2)} GB)`)
  console.log(`📋 阈值限制: ${CONFIG.maxSizeGB} GB\n`)

  if (sizeGB > CONFIG.maxSizeGB) {
    console.log(`⚠️  警告: target 目录已超过 ${CONFIG.maxSizeGB} GB!`)

    if (CONFIG.autoClean) {
      cargoClean()
    } else {
      console.log('💡 建议运行: npm run target:clean\n')
      process.exit(1)
    }
  } else {
    console.log('✅ target 目录大小正常\n')
  }
}

main()
