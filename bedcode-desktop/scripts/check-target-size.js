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
  // ⚠ 「可删」是**有条件的**，不是无条件授权——见下方 legacyTargetLive 名单。
  // 历史教训：2026-10-06 发现 `packages/target/fixtures` 正被
  // `bedcode-wasm-core/src/test_support.rs` 按 `../target/fixtures` 路径引用
  // （与 `packages/.cargo/config.toml` 的 `../target/fixtures` 落到两处——历史坑见
  // build-process.md「Target 目录管理」）。当时的报告把它标成「可安全删除」，
  // 照做就会打断在途工作。故名单内的目录只报「有消费者」并排除出可回收统计。
  // **2026-10-08 已消解**：整核本体迁根时把读路径归一为
  // `../../bedcode-desktop/target/fixtures`（与 config / bench / 工具链同一目录），
  // `packages/target` 不再有消费者，回归可回收。
  legacyTargetParents: ['packages', 'wasm-apps'],
  // 已知**仍被代码引用**的遗留落点：路径 → 引用方。命中即不标「可删」、不计可回收。
  // 新增遗留目录时若被任何构建脚本 / 测试夹具按字面路径引用，必须登记到这里。
  legacyTargetLive: {},
  // 仓库根级 target 目录（在两端目录之外，故只报告不自动处理）：
  // `cross-end-tests/` 的依赖图是两端 lib 的**并集** + 自己的 dev 依赖，
  // 跟任何一端都不相同——并入端内目录会驱逐该端缓存，且端内 target 有 15G
  // 自动 clean 阈值，混在一起会统计失真；
  // `target/server-libs` 是 server-lib 拆出后 6 个 crate 的共享落点（仓库根）；
  // `target/host-kits` 是 wasm-core-lib-split 后「机制内核 + 能力域」同族的共享落点
  // （wasm-core-lib-split 票 03/05 + wasm-core-whole-crate 票 02）——**它是全仓最大的
  // 桶**，漏登记会让最大的 target 完全不出现在报告里（2026-10-06 实测 19.7G 未被报告）。
  rootTargetDirs: ['target/server-libs', 'target/host-kits', 'cross-end-tests/target'],
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
    // 先探 **parent 自身**的 target（如 `packages/target`）：遗留循环只看得到
    // `<parent>/<crate>/target`，而 parent 级落点（历史上两份 `.cargo/config.toml`
    // 多写一个 `..` 造成的错位产物）正好落在这里，不单独探就完全不出现在报告里。
    for (const rel of [`${parent}/target`, `${parent}/rust/target`]) {
      const dir = join(process.cwd(), rel)
      if (existsSync(dir)) {
        const liveRef = CONFIG.legacyTargetLive[rel]
        found.push({
          rel,
          dir,
          size: getDirectorySize(dir),
          kind: liveRef ? 'legacy-live' : 'legacy',
          liveRef,
        })
      }
    }
    for (const entry of readdirSync(parentDir, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue
      for (const suffix of ['/target', '/rust/target']) {
        const rel = `${parent}/${entry.name}${suffix}`
        const dir = join(process.cwd(), rel)
        if (existsSync(dir)) {
          const liveRef = CONFIG.legacyTargetLive[rel]
          found.push({
            rel,
            dir,
            size: getDirectorySize(dir),
            kind: liveRef ? 'legacy-live' : 'legacy',
            liveRef,
          })
        }
      }
    }
  }

  for (const rel of CONFIG.rootTargetDirs) {
    // 相对**仓库根**：本脚本 cwd 为 bedcode-desktop/ 或 bedcode-mobile/，仓库根 = cwd/..
    const dir = join(process.cwd(), '..', rel)
    if (existsSync(dir)) {
      found.push({ rel, dir, size: getDirectorySize(dir), kind: 'root' })
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

  console.log('📦 其它 target 目录（共享编译落点 / 改造前残留 / 仓库根工程）:\n')
  for (const { rel, size, kind, liveRef } of others) {
    const tag =
      kind === 'shared' ? '共享' : kind === 'root' ? '根级' : kind === 'legacy-live' ? '遗留·在用' : '遗留'
    const note =
      kind === 'shared'
        ? '保留（删除会丢失共享编译缓存）'
        : kind === 'root'
          ? '保留（依赖图独立；删除=下次重建十几分钟）'
          : kind === 'legacy-live'
            ? `⚠ **不可删**——被 ${liveRef} 按字面路径引用`
            : '可安全删除（删除后重建为共享目录）'
    console.log(`  [${tag}] ${rel} — ${formatSize(size)}  ${note}`)
  }
  const legacyBytes = others.filter((d) => d.kind === 'legacy').reduce((a, d) => a + d.size, 0)
  const liveBytes = others.filter((d) => d.kind === 'legacy-live').reduce((a, d) => a + d.size, 0)
  if (legacyBytes > 0) {
    console.log(`\n💡 遗留目录可回收 ${formatSize(legacyBytes)}（rm -rf 后重建为共享目录）`)
  }
  if (liveBytes > 0) {
    console.log(
      `⛔ 另有 ${formatSize(liveBytes)} 遗留目录仍被代码按字面路径引用，已排除在上行可回收之外`,
    )
  }
  console.log()
}

main()
