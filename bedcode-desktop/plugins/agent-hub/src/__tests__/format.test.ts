/**
 * 展示层格式化纯函数单测（票据 06）
 *
 * 关键约束：数字抽查与源数据一致——缩写不得产生「假零」（0.04k 显示 0k
 * 会与源数据矛盾），进位方向向上；null 不估算显示 —。
 */
import { describe, expect, it } from 'vitest'
import {
  abbreviateProject,
  formatCost,
  formatDuration,
  formatEventTime,
  formatSessionTime,
  formatTokens,
} from '../utils/format'

describe('formatTokens', () => {
  it('千以下原样', () => {
    expect(formatTokens(0)).toBe('0')
    expect(formatTokens(42)).toBe('42')
    expect(formatTokens(999)).toBe('999')
  })

  it('k / M 分级一位小数', () => {
    expect(formatTokens(1000)).toBe('1.0k')
    expect(formatTokens(8_400_000)).toBe('8.4M')
    expect(formatTokens(1_050_000)).toBe('1.1M')
  })

  it('非零小量不产生假零', () => {
    // 40 tokens → 0.04k → 向上进位为 0.1k（显示 0k 与源数据矛盾）
    expect(formatTokens(40)).toBe('40') // 千以下原样
    expect(formatTokens(1004)).toBe('1.1k')
    expect(formatTokens(10_000_400)).toBe('10M') // ≥10M 取整（0.1% 级误差非假零）
  })

  it('空值与非法值显示 —', () => {
    expect(formatTokens(null)).toBe('—')
    expect(formatTokens(undefined)).toBe('—')
    expect(formatTokens(Number.NaN)).toBe('—')
    expect(formatTokens(-5)).toBe('0')
  })
})

describe('formatDuration', () => {
  it('分段缩写', () => {
    expect(formatDuration(500)).toBe('500ms')
    expect(formatDuration(42_000)).toBe('42s')
    expect(formatDuration(90_000)).toBe('1.5min')
    expect(formatDuration(96_000)).toBe('1.6min') // 向上取整到 0.1
    expect(formatDuration(21.4 * 3600 * 1000)).toBe('21.4h')
    expect(formatDuration(30 * 24 * 3600 * 1000)).toBe('30d')
  })

  it('空值显示 —', () => {
    expect(formatDuration(null)).toBe('—')
    expect(formatDuration(undefined)).toBe('—')
  })
})

describe('formatSessionTime / formatEventTime', () => {
  it('本地时区标签（用固定本地时间构造验证格式）', () => {
    const d = new Date(2026, 8, 12, 21, 47, 5) // 本地 2026-09-12 21:47:05
    expect(formatSessionTime(d.getTime())).toBe('09-12 21:47')
    expect(formatEventTime(d.getTime())).toBe('21:47:05')
  })

  it('空值：列表 — / 事件空串', () => {
    expect(formatSessionTime(null)).toBe('—')
    expect(formatEventTime(null)).toBe('')
  })
})

describe('abbreviateProject', () => {
  it('家目录折叠为 ~', () => {
    expect(abbreviateProject('/home/u/proj', '/home/u')).toBe('~/proj')
    expect(abbreviateProject('/home/u', '/home/u')).toBe('~')
  })

  it('非家目录与空值原样', () => {
    expect(abbreviateProject('/var/data', '/home/u')).toBe('/var/data')
    expect(abbreviateProject(null, '/home/u')).toBe('—')
  })
})

describe('formatCost', () => {
  it('有则存两位小数，null 不估算', () => {
    expect(formatCost(12.970714)).toBe('$12.97')
    expect(formatCost(0)).toBe('$0.00')
    expect(formatCost(null)).toBe('—')
  })
})
