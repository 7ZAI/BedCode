#!/usr/bin/env node
/**
 * 构建资源自适应 profile —— 跨平台采样系统状态 → 分档 → 生成编译 env 增补项
 *
 * 设计动机（根因见根 .cargo/config.toml 注释）：wasmtime / cranelift 等巨型 crate
 * 单路 rustc 峰值约 1~2GiB，16 路并发需要 20GiB+，无 swap 机器直接 OOM kill。
 * 静态配置（jobs=4）只能按「典型状态」取折中；本模块在每次构建启动前采样
 * 「CPU 负载 + 可用内存 + swap 抖动」，按双指标约束动态分档：
 *   - CPU 空闲 & 内存充裕 → parallel（cargo 多 jobs / gradle 多 workers / Node 大堆）
 *   - 任一指标紧张 → 降档（保守优先，防 OOM 优先于提速）
 * 内存是硬约束：jobs 永远 ≤ min(cores, floor(availGiB / 预算))，即使 CPU 全空闲。
 *
 * 平台策略：
 *   - Linux（开发机）: /proc/loadavg、/proc/meminfo、/sys/.../cpu/online、/proc/vmstat（swap 双采样）
 *   - macOS: os.loadavg() / os.freemem()（swap 检测不做，内存阈值兜底）
 *   - Windows: PowerShell Win32_Processor.LoadPercentage（Node 的 os.loadavg 在 Windows 恒返回 [0,0,0]）；
 *     采样失败/超时 → 负载按「未知」处理，仅按内存分档（多保守一档，不会 OOM）
 *   - 其他平台: 不注入任何覆盖（行为等同原生命令）
 *
 * 本模块纯函数均可注入依赖（readFile / exec / sleep / platform），便于跨平台单测。
 */

import os from 'node:os'
import { readFileSync } from 'node:fs'
import { spawnSync } from 'node:child_process'

// ==================== 常量 ====================

/** 每 GiB 可用内存允许的 rustc 并发预算（与根 .cargo/config.toml 经验公式一致） */
export const JOBS_PER_GIB_DEFAULT = 1.5

/** 分档默认阈值（全部可经 overrides 覆盖） */
export const PROFILE_DEFAULTS = {
  jobsPerGiB: JOBS_PER_GIB_DEFAULT,
  loadSerial: 1.5, // load1/nproc 达到该值 → 串行（CPU 满载）
  loadBalanced: 1.0, // 达到该值 → 均衡
  loadParallel: 0.5, // 低于该值 → 视为空闲（并行档候选）
  memSerialGiB: 1.5, // 可用内存低于该值 → 串行（OOM 高危）
  memBalancedGiB: 3.0, // 低于该值 → 均衡
  memParallelGiB: 4.0, // 达到该值 → 并行档候选（配合 CPU 空闲）
}

export const PROFILES = ['serial', 'balanced', 'parallel']

/** swap 双采样窗口（ms）与判定阈值：窗口内 pswpin+pswpout 增量 > 该值视为 swap 抖动 */
export const SWAP_SAMPLE_MS = 500
export const SWAP_PAGE_DELTA_THRESHOLD = 1
/** 可用内存低于该值才做 swap 双采样（内存充裕时 swap 抖动几乎不可能，省一次采样开销） */
export const MEM_SWAP_PROBE_GATE_KB = 8 * 1024 * 1024 // 8 GiB

/** Windows CPU 负载采样命令（LoadPercentage = 当前忙碌百分比，0~100） */
export const WIN_LOAD_CMD =
  'powershell.exe -NoProfile -NonInteractive -Command "(Get-CimInstance Win32_Processor | Measure-Object LoadPercentage -Average).Average"'

// ==================== 纯解析函数（平台无关，可单测） ====================

/** /proc/loadavg 首字段 = 1 分钟负载；解析失败返回 null */
export function parseLoadavg(text) {
  const m = /^([\d.]+)/.exec(String(text).trim())
  return m ? Number(m[1]) : null
}

/**
 * /proc/meminfo → { memTotalKb, memAvailableKb, swapTotalKb }（kB）
 * 缺 MemAvailable 字段（老内核）时用 MemFree + Cached + Buffers 兜底（近似可分配量）
 */
export function parseMeminfo(text) {
  const out = {}
  for (const line of String(text).split('\n')) {
    const m = /^(MemTotal|MemFree|MemAvailable|SwapTotal|Cached|Buffers):\s+(\d+)\s*kB/.exec(line)
    if (m) out[m[1]] = Number(m[2])
  }
  const memAvailableKb =
    out.MemAvailable ?? (out.MemFree ?? 0) + (out.Cached ?? 0) + (out.Buffers ?? 0)
  return {
    memTotalKb: out.MemTotal ?? 0,
    memAvailableKb,
    swapTotalKb: out.SwapTotal ?? 0,
  }
}

/** /sys/devices/system/cpu/online（如 "0-15" / "0-7,16-23"）→ 在线核数；解析失败返回 null */
export function parseCpuOnline(text) {
  const s = String(text).trim()
  if (!s) return null
  let n = 0
  for (const part of s.split(',')) {
    const range = /^(\d+)-(\d+)$/.exec(part)
    if (range) n += Number(range[2]) - Number(range[1]) + 1
    else if (/^\d+$/.test(part)) n += 1
    else return null
  }
  return n
}

/** /proc/vmstat → { pswpin, pswpout }（自开机累计页数；缺失字段补 0） */
export function parseVmstat(text) {
  const out = {}
  for (const line of String(text).split('\n')) {
    const m = /^(pswpin|pswpout)\s+(\d+)/.exec(line)
    if (m) out[m[1]] = Number(m[2])
  }
  return { pswpin: out.pswpin ?? 0, pswpout: out.pswpout ?? 0 }
}

/**
 * PowerShell LoadPercentage 输出 → 忙碌百分比（0~100）
 * 兼容小数点逗号/句点两种区域格式（"12,5" 与 "12.5" 均 → 12.5）；解析失败/越界返回 null
 */
export function parseWinLoadPercentage(text) {
  const m = /(\d+(?:[.,]\d+)?)/.exec(String(text))
  if (!m) return null
  const n = Number(m[1].replace(',', '.'))
  return Number.isFinite(n) && n >= 0 && n <= 100 ? n : null
}

// ==================== env 合并（保留用户自定义项，只增改本项目控制的参数） ====================

/** 合并 NODE_OPTIONS：原位替换/追加 --max-old-space-size，其余 token 原样保留 */
export function mergeNodeOptions(existing, heapMb) {
  const token = `--max-old-space-size=${heapMb}`
  const s = String(existing ?? '').trim()
  if (s === '') return token
  if (/--max-old-space-size=\d+/.test(s)) {
    return s.replace(/--max-old-space-size=\d+/g, token)
  }
  return `${s} ${token}`
}

/** 合并 GRADLE_OPTS：原位替换/追加 -Dorg.gradle.workers.max，其余 JVM 参数原样保留 */
export function mergeGradleOpts(existing, workers) {
  const token = `-Dorg.gradle.workers.max=${workers}`
  const s = String(existing ?? '').trim()
  if (s === '') return token
  if (/-Dorg\.gradle\.workers\.max=\d+/.test(s)) {
    return s.replace(/-Dorg\.gradle\.workers\.max=\d+/g, token)
  }
  return `${s} ${token}`
}

// ==================== 分档（纯计算，全平台共用） ====================

/**
 * 按采样指标分档并输出编译参数。
 *
 * @param {object} metrics
 *   - cores: 逻辑核数（>=1）
 *   - loadRatio: CPU 负载比（load1/cores 或 Windows 忙碌百分比/100）；未知传 null
 *   - loadKnown: 负载是否可信（Windows 采样失败时 false）
 *   - memAvailableGiB: 可用内存 GiB（未知传 null → 按 0 保守处理）
 *   - swapActive: 是否检测到 swap 抖动
 * @param {object} [overrides] 阈值/强制档位覆盖（PROFILE_DEFAULTS 的子集；profile 支持
 *   'serial' | 'balanced' | 'parallel'，显式强制档位优先于任何自动判定，但内存上限仍然生效）
 * @returns {{ profile, cargoJobs, gradleWorkers, nodeHeapMb, reasons, forced }}
 */
export function classify(metrics, overrides = {}) {
  const o = { ...PROFILE_DEFAULTS, ...overrides }
  const cores = Number.isInteger(metrics.cores) && metrics.cores >= 1 ? metrics.cores : 1
  const mem = Number.isFinite(metrics.memAvailableGiB) ? metrics.memAvailableGiB : 0
  const load = metrics.loadRatio
  const loadKnown = metrics.loadKnown !== false && Number.isFinite(load)

  // 内存硬约束：每路 rustc 峰值预算 ≈ jobsPerGiB，jobs 永不超 min(cores, floor(mem/预算))
  const formulaJobs = Math.max(1, Math.min(cores, Math.floor(mem / o.jobsPerGiB)))

  const reasons = []
  if (metrics.swapActive) reasons.push('swap-active')
  if (loadKnown && load >= o.loadSerial) reasons.push(`load=${load.toFixed(2)}`)
  if (mem < o.memSerialGiB) reasons.push(`mem=${mem.toFixed(1)}GiB`)

  const forced = overrides.profile && PROFILES.includes(overrides.profile) ? overrides.profile : null
  let profile
  if (forced) {
    profile = forced // 显式覆盖优先（用户明确知道自己在干什么）
  } else if (metrics.swapActive || (loadKnown && load >= o.loadSerial) || mem < o.memSerialGiB) {
    profile = 'serial'
  } else if ((loadKnown && load >= o.loadBalanced) || mem < o.memBalancedGiB) {
    profile = 'balanced'
  } else if (loadKnown && load < o.loadParallel && mem >= o.memParallelGiB) {
    profile = 'parallel'
  } else {
    profile = 'balanced' // 负载在 [0.5,1) 或内存 [3,4) 等中间地带 → 保守取均衡
  }

  const jobCap = profile === 'serial' ? 1 : profile === 'balanced' ? 2 : Infinity
  const cargoJobs = profile === 'serial' ? 1 : Math.min(formulaJobs, jobCap)
  const gradleWorkers = profile === 'serial' ? 1 : profile === 'balanced' ? 2 : 4
  const nodeHeapMb = profile === 'serial' ? 1024 : profile === 'balanced' ? 1536 : 4096

  return { profile, cargoJobs, gradleWorkers, nodeHeapMb, reasons, forced: !!forced }
}

// ==================== 系统采样（依赖可注入，跨平台） ====================

function tryRead(readFile, path) {
  try {
    return readFile(path, 'utf8')
  } catch {
    return null // 文件缺失/不可读（WSL 差异等）→ 走 os API 兜底
  }
}

/** Linux 采样：/proc 三件套 + cpu online + 条件式 swap 双采样 */
async function sampleLinux({ readFile, sleep, cores }) {
  const loadText = tryRead(readFile, '/proc/loadavg')
  const memText = tryRead(readFile, '/proc/meminfo')
  const onlineText = tryRead(readFile, '/sys/devices/system/cpu/online')

  const loadRatio = loadText ? parseLoadavg(loadText) : null
  let mem = memText ? parseMeminfo(memText) : null
  if (!mem) {
    // /proc 不可读（极端环境）→ Node os API 兜底
    mem = {
      memTotalKb: Math.round(os.totalmem() / 1024),
      memAvailableKb: Math.round(os.freemem() / 1024),
      swapTotalKb: 0,
    }
  }

  let swapActive = false
  // swap 双采样判定抖动：仅在「有 swap 且内存进入关注区」时做，避免每次启动白等 500ms
  if (mem.swapTotalKb > 0 && mem.memAvailableKb < MEM_SWAP_PROBE_GATE_KB) {
    const v1 = parseVmstat(tryRead(readFile, '/proc/vmstat') ?? '')
    await sleep(SWAP_SAMPLE_MS)
    const v2 = parseVmstat(tryRead(readFile, '/proc/vmstat') ?? '')
    const pageDelta = v2.pswpin - v1.pswpin + (v2.pswpout - v1.pswpout)
    swapActive = pageDelta > SWAP_PAGE_DELTA_THRESHOLD
  }

  return {
    platform: 'linux',
    cores: onlineText ? parseCpuOnline(onlineText) ?? cores : cores,
    loadRatio,
    loadKnown: Number.isFinite(loadRatio),
    memAvailableGiB: mem.memAvailableKb / 1024 / 1024,
    memTotalGiB: mem.memTotalKb / 1024 / 1024,
    swapActive,
  }
}

/** macOS 采样：os 原生 API（loadavg 在 Unix 上可用；swap 检测不做，内存阈值兜底） */
function sampleDarwin({ cores }) {
  return {
    platform: 'darwin',
    cores,
    loadRatio: os.loadavg()[0] / cores,
    loadKnown: true,
    memAvailableGiB: os.freemem() / 1024 ** 3,
    memTotalGiB: os.totalmem() / 1024 ** 3,
    swapActive: false,
  }
}

/** Windows 采样：内存走 os API；负载走 PowerShell（失败/超时 → 负载未知，仅按内存分档） */
function sampleWin32({ exec, cores }) {
  const memAvailableGiB = os.freemem() / 1024 ** 3
  let loadRatio = null
  try {
    const out = exec(WIN_LOAD_CMD, { timeout: 3000 })
    if (out && out.status === 0 && out.stdout) {
      const busyPct = parseWinLoadPercentage(out.stdout)
      if (busyPct !== null) loadRatio = busyPct / 100
    }
  } catch {
    loadRatio = null // PowerShell 不可用/被策略禁用 → 负载未知
  }
  return {
    platform: 'win32',
    cores,
    loadRatio,
    loadKnown: loadRatio !== null,
    memAvailableGiB,
    memTotalGiB: os.totalmem() / 1024 ** 3,
    swapActive: false,
  }
}

/**
 * 采样当前系统资源状态（异步；swap 双采样需要一次短暂等待）。
 *
 * @param {object} [options] 注入点（单测用）：platform / readFile / exec / sleep
 * @returns {Promise<{platform, cores, loadRatio, loadKnown, memAvailableGiB, memTotalGiB, swapActive}>}
 */
export async function sampleSystem(options = {}) {
  const {
    platform = process.platform,
    // 注意：必须是同步函数——tryRead 按同步调用；测试注入的 mock 也是同步形态
    readFile = readFileSync,
    exec = (cmd, opts) => spawnSync(cmd, { shell: true, encoding: 'utf8', ...opts }),
    sleep = (ms) => new Promise((r) => setTimeout(r, ms)),
  } = options
  const cores = typeof os.availableParallelism === 'function' ? os.availableParallelism() : os.cpus().length

  if (platform === 'linux') return sampleLinux({ readFile, sleep, cores })
  if (platform === 'darwin') return sampleDarwin({ cores })
  if (platform === 'win32') return sampleWin32({ exec, cores })
  // 其他平台：不注入覆盖（行为等同原生命令），全字段置未知/空
  return {
    platform,
    cores,
    loadRatio: null,
    loadKnown: false,
    memAvailableGiB: null,
    memTotalGiB: null,
    swapActive: false,
  }
}

// ==================== env 增补 ====================

/**
 * 组合分档决策 → env 增补项。
 *
 * @param {object} metrics sampleSystem() 的返回
 * @param {object} [overrides] classify() 的 overrides
 * @param {object} [baseEnv] 现有环境（默认 process.env），用于合并 NODE_OPTIONS / GRADLE_OPTS
 * @returns {{ profile, cargoJobs, gradleWorkers, nodeHeapMb, reasons, forced, additions }}
 *   additions 为可直接 Object.assign 到 process.env 的增补项
 */
export function buildEnv(metrics, overrides = {}, baseEnv = process.env) {
  const decision = classify(metrics, overrides)
  const additions = {
    CARGO_BUILD_JOBS: String(decision.cargoJobs),
    GRADLE_OPTS: mergeGradleOpts(baseEnv.GRADLE_OPTS ?? '', decision.gradleWorkers),
    NODE_OPTIONS: mergeNodeOptions(baseEnv.NODE_OPTIONS ?? '', decision.nodeHeapMb),
  }
  return { ...decision, additions }
}
