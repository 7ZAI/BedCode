/**
 * reconnectCountdown 行为契约（M-02，2026-10-04 OCR）
 *
 * 背景：OCR 发现重连倒计时从不递减——`sync()` 每秒重读同一个静态
 * `reconnectInMs` 并赋 `ceil(reconnectInMs/1000)`，显示秒数恒定（如永远
 * 「30s 后重连」）。修复 = 记录排期**到达时刻**，剩余 = 等待时长 − 已流逝。
 *
 * 契约清单：
 * - C-1 正例：未开始流逝 → 显示满值（ceil 到秒）
 * - C-2 正例：流逝到剩余不足整秒 → 向上取整（1s 粒度，退避以秒计）
 * - C-3 边界：剩余恰好整秒 → 该秒数
 * - C-4 边界：已过期（now ≥ receivedAt + reconnectInMs）→ null（显示
 *   「正在重连…」而非编造「0 秒后」）
 * - C-5 反例：未排期（reconnectInMs/receivedAt 任一为空）→ null
 * - C-6 边界：reconnectInMs ≤ 0 / 非有限 → 剩余 0 → null（不出现负数）
 * - C-7 边界：now < receivedAt（时钟回拨）不产生秒数膨胀（钳到满值）
 */

import { describe, it, expect } from 'vitest'
import {
  reconnectRemainingMs,
  reconnectDisplaySeconds,
} from '@/utils/reconnectCountdown'

describe('reconnectRemainingMs', () => {
  it('C-1/C-2 正例：剩余毫秒 = 等待时长 − 已流逝，过期钳 0', () => {
    const at = 1_000
    expect(reconnectRemainingMs(4000, at, at)).toBe(4000)
    expect(reconnectRemainingMs(4000, at, at + 500)).toBe(3500)
    expect(reconnectRemainingMs(4000, at, at + 4000)).toBe(0)
    expect(reconnectRemainingMs(4000, at, at + 9999)).toBe(0)
  })

  it('C-7 边界：now < receivedAt（时钟回拨）不膨胀，钳到满值', () => {
    const at = 1_000
    expect(reconnectRemainingMs(4000, at, at - 5000)).toBe(4000)
  })

  it('C-6 边界：非有限 / ≤0 的等待时长 → 0（防 NaN 外溢）', () => {
    expect(reconnectRemainingMs(0, 1, 1)).toBe(0)
    expect(reconnectRemainingMs(-5, 1, 1)).toBe(0)
    expect(reconnectRemainingMs(Number.NaN, 1, 1)).toBe(0)
  })
})

describe('reconnectDisplaySeconds（视图用的显示秒数）', () => {
  it('C-1 正例：排期刚到达 → 显示满值秒（向上取整）', () => {
    const at = 1_000
    expect(reconnectDisplaySeconds(4000, at, at)).toBe(4)
    // 4001ms → ceil → 5？不：remaining=4001 → ceil(4.001)=5 会放大一秒
    // ——排期 4001ms 本就该显示 5s（向上取整契约：不足 5s 的等待显示 5）
    expect(reconnectDisplaySeconds(30_000, at, at + 500)).toBe(30)
  })

  it('C-2 正例：流逝后秒数递减（M-02 修复本体：不再恒定）', () => {
    const at = 1_000
    expect(reconnectDisplaySeconds(30_000, at, at)).toBe(30)
    expect(reconnectDisplaySeconds(30_000, at, at + 5_000)).toBe(25)
    expect(reconnectDisplaySeconds(30_000, at, at + 29_500)).toBe(1)
  })

  it('C-3 边界：剩余恰好整秒 → 该秒数（不放大）', () => {
    const at = 1_000
    expect(reconnectDisplaySeconds(30_000, at, at + 10_000)).toBe(20)
  })

  it('C-4 边界：已过期 → null（显示「正在重连…」，不编造「0 秒后」）', () => {
    const at = 1_000
    expect(reconnectDisplaySeconds(4000, at, at + 4000)).toBeNull()
    expect(reconnectDisplaySeconds(4000, at, at + 9999)).toBeNull()
  })

  it('C-5 反例：未排期（任一为空）→ null', () => {
    expect(reconnectDisplaySeconds(null, 1, 1)).toBeNull()
    expect(reconnectDisplaySeconds(4000, null, 1)).toBeNull()
    expect(reconnectDisplaySeconds(undefined, 1, 1)).toBeNull()
  })

  it('C-6 边界：非法等待时长 → null', () => {
    expect(reconnectDisplaySeconds(0, 1, 1)).toBeNull()
    expect(reconnectDisplaySeconds(Number.NaN, 1, 1)).toBeNull()
  })
})