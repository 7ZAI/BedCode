/**
 * TerminalResizePolicy 单元测试（Seam A：纯逻辑）
 *
 * 覆盖 shouldApplyGridResize 契约：±1 以内测量漂移不触发 resize；
 * >1 列/行真实变化触发；列与行独立判定。
 */

import { describe, it, expect } from 'vitest'
import { shouldApplyGridResize, RESIZE_GRID_TOLERANCE, ATLAS_PREHEAT_DELAY_MS } from '@/utils/terminalResizePolicy'

describe('shouldApplyGridResize', () => {
  it('完全一致：不触发', () => {
    expect(shouldApplyGridResize(100, 40, 100, 40)).toBe(false)
  })

  it('±1 列漂移（round/DPR/滚动条测量误差）：不触发', () => {
    expect(shouldApplyGridResize(100, 40, 101, 40)).toBe(false)
    expect(shouldApplyGridResize(100, 40, 99, 40)).toBe(false)
  })

  it('±1 行漂移：不触发', () => {
    expect(shouldApplyGridResize(100, 40, 100, 41)).toBe(false)
    expect(shouldApplyGridResize(100, 40, 100, 39)).toBe(false)
  })

  it('同向累计（§跨 2）：列/行任一差 2 即触发', () => {
    expect(shouldApplyGridResize(100, 40, 102, 40)).toBe(true)
    expect(shouldApplyGridResize(100, 40, 98, 40)).toBe(true)
    expect(shouldApplyGridResize(100, 40, 100, 42)).toBe(true)
    expect(shouldApplyGridResize(100, 40, 100, 38)).toBe(true)
  })

  it('列行独立判定：列差 1 但行差 2 时仍触发（只影响真实变化的一维）', () => {
    expect(shouldApplyGridResize(100, 40, 101, 42)).toBe(true)
    expect(shouldApplyGridResize(100, 40, 99, 38)).toBe(true)
  })

  it('真实拖窗大变化：触发', () => {
    expect(shouldApplyGridResize(100, 40, 120, 30)).toBe(true)
  })

  it('常量契约：钳制阈值为 1，预热延迟 700ms', () => {
    expect(RESIZE_GRID_TOLERANCE).toBe(1)
    expect(ATLAS_PREHEAT_DELAY_MS).toBe(700)
  })
})