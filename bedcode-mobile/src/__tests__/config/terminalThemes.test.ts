/**
 * resolveTerminalTheme 单元测试
 *
 * 覆盖：'system' 按系统明暗解析为具体色板（xterm 不接受 var() 串）、
 * 具名主题原样透传、未知主题名回退 dark。
 */
import { describe, it, expect } from 'vitest'
import { TERMINAL_THEMES, resolveTerminalTheme } from '@/config/terminalThemes'

describe('resolveTerminalTheme', () => {
  it('resolves system to dark/light concrete palettes (never var() strings)', () => {
    expect(resolveTerminalTheme('system', true)).toBe(TERMINAL_THEMES.dark)
    expect(resolveTerminalTheme('system', false)).toBe(TERMINAL_THEMES.light)
    // 解析结果必须可直接交给 xterm：颜色值为可解析字面量
    expect(resolveTerminalTheme('system', true).background).toMatch(/^#[0-9a-f]{6}$/i)
  })

  it('passes named themes through unchanged', () => {
    expect(resolveTerminalTheme('nord', true)).toBe(TERMINAL_THEMES.nord)
    expect(resolveTerminalTheme('dracula', false)).toBe(TERMINAL_THEMES.dracula)
  })

  it('falls back to dark for unknown theme names', () => {
    expect(resolveTerminalTheme('nonexistent', false)).toBe(TERMINAL_THEMES.dark)
  })
})
