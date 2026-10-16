/**
 * isWithinRestoreWindow 纯函数测试（快照恢复窗口）
 *
 * 窗口语义：仅「近期可见」条目（真实 lastSeen 在 DEVICE_TTL_MS 内）可被
 * 快照恢复为拨号候选；陈旧条目（旧网络 / 旧身份残留的旧地址）不得上屏——
 * 2026-10-16 实机：9 天前的 192.168.1.4:47613 被按当前网络拨号，TCP 超时
 * 表现为「对端无反应」。
 */
import { describe, it, expect } from 'vitest'
import {
  DEVICE_TTL_MS,
  isWithinRestoreWindow,
} from '../../../../wasm-apps/file-transfer/src/composables/deviceState'

const NOW = Date.now()

describe('isWithinRestoreWindow', () => {
  it('accepts entries seen within the TTL window', () => {
    expect(isWithinRestoreWindow(NOW, NOW)).toBe(true)
    expect(isWithinRestoreWindow(NOW - DEVICE_TTL_MS, NOW)).toBe(true)
    expect(isWithinRestoreWindow(NOW - 1, NOW)).toBe(true)
  })

  it('rejects entries older than the TTL window (stale network residue)', () => {
    expect(isWithinRestoreWindow(NOW - DEVICE_TTL_MS - 1, NOW)).toBe(false)
    expect(isWithinRestoreWindow(NOW - 9 * 24 * 60 * 60 * 1000, NOW)).toBe(false)
  })

  it('rejects future timestamps (clock corruption / bogus data)', () => {
    expect(isWithinRestoreWindow(NOW + 1, NOW)).toBe(false)
    expect(isWithinRestoreWindow(Number.MAX_SAFE_INTEGER, NOW)).toBe(false)
  })

  it('rejects missing / non-numeric lastSeen (unprovable visibility)', () => {
    expect(isWithinRestoreWindow(undefined, NOW)).toBe(false)
    expect(isWithinRestoreWindow(null, NOW)).toBe(false)
    expect(isWithinRestoreWindow(Number.NaN, NOW)).toBe(false)
    expect(isWithinRestoreWindow('2026-10-16', NOW)).toBe(false)
  })
})
