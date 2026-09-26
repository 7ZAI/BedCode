/**
 * 输出拉取节奏策略单测（F5：轮询节奏对齐迁移前引擎常量）
 *
 * 被测对象：`src/utils/terminal/terminalPullPolicy.ts`（纯函数 + 常量）。
 *
 * 行为契约来源：`.scratch/2026-09-26-output-ack-backpressure/spec.md` §6 F5
 * 「兜底轮询保持唯一唤醒源；可选把 100/500 ms 对齐旧引擎 50/250 ms
 * （ENGINE_POLL_FAST/IDLE）」+ §12.1 迁移前常量表
 * （`ENGINE_POLL_FAST_INTERVAL = 50` / `ENGINE_POLL_IDLE = 250` /
 * `ENGINE_IDLE_THRESHOLD = 5` / `FETCH_BUDGET = 64 KiB`）。
 *
 * 为何用纯函数测而不是组件级定时器测：节奏值与降档/回快档的优先级是本模块的
 * 全部契约；组件里的 `setInterval` 只能给出「大概拉了几次」这类依赖机器负载的
 * 弱断言。组件侧只保留一条「50 ms 内出现第二次拉取」的接线断言（在
 * terminalPreview.test.ts）。
 */

import { describe, it, expect } from 'vitest'
import {
  OUTPUT_FETCH_BATCH_BYTES,
  OUTPUT_IDLE_INTERVAL_MS,
  OUTPUT_IDLE_THRESHOLD,
  OUTPUT_PULL_BUDGET_BYTES,
  OUTPUT_PULL_INTERVAL_MS,
  OUTPUT_PULL_MAX_BATCHES,
  decidePollIntervalMs,
  nextIdleStreak,
} from '../utils/terminal/terminalPullPolicy'

describe('terminalPullPolicy 常量对齐（迁移前引擎节奏）', () => {
  // P1 正例：单 tick 预算 = 64 KiB（FETCH_BUDGET），与 ack 阈值同源
  it('单 tick 字节预算 = 64 KiB（4 批 × 16 KiB，对齐 FETCH_BUDGET）', () => {
    expect(OUTPUT_FETCH_BATCH_BYTES).toBe(16 * 1024)
    expect(OUTPUT_PULL_MAX_BATCHES).toBe(4)
    expect(OUTPUT_PULL_BUDGET_BYTES).toBe(64 * 1024)
  })

  // P1 正例：双档间隔与空闲阈值取迁移前引擎值（不是宿主桥时代的 100/500）
  it('快档 50 ms / 慢档 250 ms / 空闲阈值 5 次（对齐 ENGINE_POLL_FAST/IDLE）', () => {
    expect(OUTPUT_PULL_INTERVAL_MS).toBe(50)
    expect(OUTPUT_IDLE_INTERVAL_MS).toBe(250)
    expect(OUTPUT_IDLE_THRESHOLD).toBe(5)
  })

  // P2 不变量：快档必须严格快于慢档（否则「降档」是反向的）
  it('不变量：快档间隔 < 慢档间隔', () => {
    expect(OUTPUT_PULL_INTERVAL_MS).toBeLessThan(OUTPUT_IDLE_INTERVAL_MS)
  })
})

describe('nextIdleStreak 连续空闲计数推进', () => {
  // P1 正例：追平 +1
  it('无数据时计数 +1（0 → 1，4 → 5）', () => {
    expect(nextIdleStreak(0, false)).toBe(1)
    expect(nextIdleStreak(4, false)).toBe(5)
  })

  // P1 反例：拉到数据清零（无论此前计数多高）
  it('有数据（hasMore）时计数清零', () => {
    expect(nextIdleStreak(9, true)).toBe(0)
  })
})

describe('decidePollIntervalMs 快/慢档裁决', () => {
  // P1 正例：未达阈值保持快档
  it('idleStreak 未达阈值 → 快档 50 ms', () => {
    expect(decidePollIntervalMs({ idleStreak: 0, hasMore: false })).toBe(50)
  })

  // P1 边界：恰好达阈值即降慢档（阈值语义是 >=，不是 >）
  it('idleStreak 恰好达阈值 → 降为慢档 250 ms', () => {
    expect(decidePollIntervalMs({ idleStreak: OUTPUT_IDLE_THRESHOLD, hasMore: false })).toBe(250)
  })

  // P1 反例：阈值前 1 次不得降档（提前降档会把空闲期 invoke 频率抬高一倍）
  it('idleStreak = 阈值 − 1 → 仍快档', () => {
    expect(decidePollIntervalMs({ idleStreak: OUTPUT_IDLE_THRESHOLD - 1, hasMore: false })).toBe(50)
  })

  // P1 边界：任一 tick 有余量立即回快档（数据来了不继续慢档）
  it('hasMore 覆盖慢档：计数已超阈值但本 tick 有数据 → 快档', () => {
    expect(decidePollIntervalMs({ idleStreak: 99, hasMore: true })).toBe(50)
  })

  // P1 反例（核心冲突组合）：驻留中的 tick 既无数据又在涨计数，仍必须快档
  it('驻留态覆盖空闲退避：计数已超阈值 → 仍快档（抑制态等 ack 不许慢下来）', () => {
    expect(decidePollIntervalMs({ idleStreak: 99, hasMore: false, parked: true })).toBe(50)
  })

  // P1 边界：驻留 + 有余量同值（优先级不改变结果，防止后续重构改动语义）
  it('驻留 + 有余量 → 仍快档（两个覆盖源同值）', () => {
    expect(decidePollIntervalMs({ idleStreak: 0, hasMore: true, parked: true })).toBe(50)
  })
})
