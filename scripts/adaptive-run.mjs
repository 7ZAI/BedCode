#!/usr/bin/env node
/**
 * 构建资源自适应包装器 —— 在原生命令前注入按系统状态分档的编译 env 参数
 *
 * 用法（脚本位于仓库根 scripts/，在对应应用目录执行以继承正确 cwd；-- 之后接命令与参数）：
 *   cd bedcode-desktop && node ../scripts/adaptive-run.mjs -- pnpm run tauri:build
 *   cd bedcode-mobile && node ../scripts/adaptive-run.mjs -- pnpm run tauri:android:build
 *   cd bedcode-desktop/src-tauri && node ../../scripts/adaptive-run.mjs -- cargo test
 *   node scripts/adaptive-run.mjs -- pnpm run test:run   # 仓库根目录（根 package.json 有 test:run）
 *   node ../scripts/adaptive-run.mjs --cmd "pnpm run tauri:build"  # --cmd 字符串形态
 *
 * 行为：
 *   1. 采样系统资源（CPU 负载 / 可用内存 / swap 抖动，见 build-profile.mjs）
 *   2. 分档并注入 CARGO_BUILD_JOBS / GRADLE_OPTS / NODE_OPTIONS（保留用户自定义项）
 *   3. spawn 原命令，stdio 原样透传，退出码透传（CI 门禁依赖）
 *
 * 逃生阀（env）：
 *   BEDCODE_ADAPTIVE=0                  完全禁用自适应，行为等同原生命令
 *   BEDCODE_BUILD_PROFILE=parallel|balanced|serial|auto   手动强制档位（默认 auto）
 *   BEDCODE_JOBS_PER_GIB=<数>           每 GiB 可用内存的 rustc 并发预算（默认 1.5）
 *
 * 平台：Windows 下经 cmd.exe（pnpm 是 .cmd shim，不开 shell 会 ENOENT）；
 *   Unix 下 shell:false 数组形式传参，避免引号转义问题。
 */

import { spawnSync } from 'node:child_process'
import { pathToFileURL } from 'node:url'
import { sampleSystem, buildEnv, PROFILES } from './build-profile.mjs'

const USAGE = `用法:
  在对应应用目录执行（脚本位于仓库根 scripts/，cwd 继承保证子命令在应用目录运行）：
  cd bedcode-desktop && node ../scripts/adaptive-run.mjs -- pnpm run tauri:build
  cd bedcode-mobile && node ../scripts/adaptive-run.mjs -- pnpm run tauri:android:build
  cd bedcode-desktop/src-tauri && node ../../scripts/adaptive-run.mjs -- cargo test
  仓库根目录（跑根测试命令）：
  node scripts/adaptive-run.mjs -- pnpm run test:run
  字符串形态（--cmd）：
  cd bedcode-desktop && node ../scripts/adaptive-run.mjs --cmd "pnpm run tauri:build"`

/**
 * 解析 CLI 参数 → 要执行的命令。
 * 三种形态：`-- cmd args`（推荐，数组保真）、`--cmd "str"`（字符串按空白拆分）、
 * 无分隔符时整段视为命令（兼容误用，文档以 -- 为准）。
 * @param {string[]} argv 不含 node/脚本路径的原始参数
 * @returns {{ cmdArgs: string[] }}
 */
export function parseCliArgs(argv) {
  const sep = argv.indexOf('--')
  if (sep !== -1) return { cmdArgs: argv.slice(sep + 1) }
  if (argv[0] === '--cmd') return { cmdArgs: (argv[1] ?? '').split(/\s+/).filter(Boolean) }
  return { cmdArgs: argv }
}

/** 从 env 读覆盖项（只读，值非法时打警告并回退默认） */
function readOverrides(env) {
  const overrides = {}
  const profile = env.BEDCODE_BUILD_PROFILE
  if (profile && profile !== 'auto') {
    if (PROFILES.includes(profile)) overrides.profile = profile
    else console.error(`[adaptive] 警告: 未知 BEDCODE_BUILD_PROFILE="${profile}"，按 auto 处理`)
  }
  const jobsPerGiB = Number(env.BEDCODE_JOBS_PER_GIB)
  if (Number.isFinite(jobsPerGiB) && jobsPerGiB > 0) overrides.jobsPerGiB = jobsPerGiB
  return overrides
}

const fmtLoad = (v) => (Number.isFinite(v) ? v.toFixed(2) : '?')

async function main() {
  const argv = process.argv.slice(2)
  if (argv[0] === '-h' || argv[0] === '--help') {
    console.log(USAGE)
    process.exit(0)
  }
  const { cmdArgs } = parseCliArgs(argv)
  if (cmdArgs.length === 0) {
    console.error(USAGE)
    process.exit(2)
  }

  const env = { ...process.env }
  if (env.BEDCODE_ADAPTIVE === '0') {
    console.error('[adaptive] 已禁用（BEDCODE_ADAPTIVE=0），原样执行')
  } else {
    const overrides = readOverrides(env)
    const metrics = await sampleSystem()
    const decision = buildEnv(metrics, overrides, process.env)
    Object.assign(env, decision.additions)
    const reason = decision.reasons.length ? ` (${decision.reasons.join(', ')})` : ''
    const forcedNote = decision.forced ? ' [forced]' : ''
    console.error(
      `[adaptive] profile=${decision.profile}${forcedNote} cargoJobs=${decision.cargoJobs}` +
        ` gradleWorkers=${decision.gradleWorkers} nodeHeap=${decision.nodeHeapMb}MB` +
        ` load=${fmtLoad(metrics.loadRatio)} mem=${fmtLoad(metrics.memAvailableGiB)}GiB${reason}`,
    )
  }

  const IS_WIN = process.platform === 'win32'
  // Windows：拼成字符串经 cmd.exe（.cmd shim 必需）；Unix：数组 + shell:false（引号保真）
  const child = IS_WIN
    ? spawnSync(cmdArgs.join(' '), { shell: true, stdio: 'inherit', env })
    : spawnSync(cmdArgs[0], cmdArgs.slice(1), { shell: false, stdio: 'inherit', env })

  if (child.error) {
    console.error(`[adaptive] 启动失败: ${child.error.message}`)
    process.exit(1)
  }
  process.exit(child.status ?? 1)
}

// 仅作为入口执行时运行 main（被测试 import 时不产生副作用）
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((err) => {
    console.error('[adaptive] 采样失败，按原生命令继续:', err?.message ?? err)
    process.exit(1)
  })
}
