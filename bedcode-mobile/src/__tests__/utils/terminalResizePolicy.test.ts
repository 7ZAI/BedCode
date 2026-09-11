/**
 * 终端网格 resize 触发策略测试（列 ±1 漂移钳制 / 行缩小立即生效 / ATLAS_PREHEAT_DELAY_MS）
 */
import { describe, it, expect } from 'vitest'
import { shouldApplyGridResize, RESIZE_GRID_TOLERANCE, ATLAS_PREHEAT_DELAY_MS } from '@/utils/terminalResizePolicy'

describe('shouldApplyGridResize', () => {
  it('±1 列偏差 / 行增大 1 行视为测量漂移，不触发 resize', () => {
    expect(shouldApplyGridResize(80, 24, 79, 24)).toBe(false)
    expect(shouldApplyGridResize(80, 24, 81, 24)).toBe(false)
    // 行增大 ±1：仍受钳制（防测量漂移 resize 风暴）
    expect(shouldApplyGridResize(80, 24, 80, 25)).toBe(false)
    // 列+行各差 1：每维度独立判定，均在阈值/钳制内
    expect(shouldApplyGridResize(80, 24, 79, 25)).toBe(false)
  })

  it('高度缩小立即生效（不再受 ±1 钳制延迟：最后一行被裁/底部空带肉眼可见）', () => {
    // 行缩小 1：立即 resize
    expect(shouldApplyGridResize(80, 24, 80, 23)).toBe(true)
    // 行缩小 2：resize
    expect(shouldApplyGridResize(80, 24, 80, 22)).toBe(true)
    // 列漂移内 + 行缩小 1：行方向缩小豁免，仍触发
    expect(shouldApplyGridResize(80, 24, 79, 23)).toBe(true)
  })

  it('偏差 > 1 时触发 resize（列或行增大任一超过）', () => {
    expect(shouldApplyGridResize(80, 24, 78, 24)).toBe(true)
    expect(shouldApplyGridResize(80, 24, 82, 24)).toBe(true)
    expect(shouldApplyGridResize(80, 24, 80, 22)).toBe(true)
    expect(shouldApplyGridResize(80, 24, 80, 26)).toBe(true)
  })

  it('同尺寸不触发', () => {
    expect(shouldApplyGridResize(80, 24, 80, 24)).toBe(false)
  })

  it('阈值常量为 1', () => {
    expect(RESIZE_GRID_TOLERANCE).toBe(1)
  })

  it('atlas 预热延迟为 700ms', () => {
    expect(ATLAS_PREHEAT_DELAY_MS).toBe(700)
  })
})