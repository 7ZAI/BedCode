/**
 * 终端色板单元测试
 *
 * 覆盖两块行为契约：
 * 1. resolveTerminalTheme —— 'system' 按系统明暗解析为具体色板（xterm 不接受 var() 串）、
 *    具名主题原样透传、未知主题名回退 dark
 * 2. 选区色（selectionBackground）—— 长按拖选的可见性与可读性矩阵：
 *    - 必须是带 alpha 的 rgba()（不透明 hex 会被 xterm 强制改写成固定 30%，
 *      各主题选区强度被锁死成同一档，浅色主题调不出合适的强度）
 *    - 合成后的选区块必须明显区别于画布底色（否则「看不出选中」）
 *    - 选区块上的默认前景色必须仍然可读（否则「选中看不到字」）
 *
 * 选区合成口径与 xterm 6 一致：ThemeService 把 selectionBackground 按 alpha
 * 压在画布底色上（blend），文字再画在选区之上。
 */
import { describe, it, expect } from 'vitest'
import { TERMINAL_THEMES, resolveTerminalTheme, selectionFrameColor, type TerminalTheme } from '../config/themes'

// ==================== 颜色工具（测试侧独立实现，不复用被测代码） ====================

type Rgb = [number, number, number]

function parseHex(color: string): Rgb | null {
  const hex = color.trim().replace(/^#/, '')
  if (!/^[0-9a-fA-F]+$/.test(hex)) return null
  if (hex.length === 3) {
    return [0, 1, 2].map((i) => parseInt(hex[i] + hex[i], 16)) as Rgb
  }
  if (hex.length === 6) {
    return [
      parseInt(hex.slice(0, 2), 16),
      parseInt(hex.slice(2, 4), 16),
      parseInt(hex.slice(4, 6), 16),
    ]
  }
  return null
}

function parseRgba(color: string): { rgb: Rgb; alpha: number } | null {
  const match = color.match(
    /^rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*(?:,\s*(0|1|0?\.\d+)\s*)?\)$/i,
  )
  if (!match) return null
  const alpha = match[4] === undefined ? 1 : Number.parseFloat(match[4])
  return { rgb: [Number(match[1]), Number(match[2]), Number(match[3])], alpha }
}

/** 把 rgba 选区色按其 alpha 压在画布底色上（xterm selectionBackgroundOpaque 的等价计算） */
function blendOverCanvas(canvas: Rgb, overlay: Rgb, alpha: number): Rgb {
  return [0, 1, 2].map((i) => Math.round(canvas[i] * (1 - alpha) + overlay[i] * alpha)) as Rgb
}

function channelLuminance(channel: number): number {
  const c = channel / 255
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
}

function relativeLuminance(rgb: Rgb): number {
  const [r, g, b] = rgb.map(channelLuminance)
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}

/** WCAG 对比度（1~21） */
function contrastRatio(a: Rgb, b: Rgb): number {
  const la = relativeLuminance(a)
  const lb = relativeLuminance(b)
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05)
}

/** 两色的 RGB 欧氏距离：选区可见性的粗筛指标 */
function colorDistance(a: Rgb, b: Rgb): number {
  return Math.sqrt(a.reduce((sum, channel, i) => sum + (channel - b[i]) ** 2, 0))
}

// ==================== 契约阈值（与 terminalThemes.ts 文件头口径一致） ====================

/** 选区块与画布底色的最小可辨距离：此前 solarized-light 仅 7.5、claude-code-light 15.3 */
const MIN_SELECTION_VISIBILITY = 40
/** 选区块上默认前景色的最小对比度（WCAG AA 正文） */
const MIN_TEXT_CONTRAST = 4.5
/** 主题自身前景色本就低于 AA 时的下限（Solarized 前景色设计如此，只有 4.13:1） */
const MIN_TEXT_CONTRAST_FALLBACK = 3

/** 需要交付到 xterm 的具体色板（'system' 是占位条目，由 resolveTerminalTheme 解析掉） */
const CONCRETE_THEMES = Object.entries(TERMINAL_THEMES).filter(([name]) => name !== 'system')

/** 解析某主题的画布底色 + 合成后的选区块 */
function resolveSlab(theme: TerminalTheme): { canvas: Rgb; slab: Rgb } {
  const canvas = parseHex(theme.background)
  if (!canvas) throw new Error(`主题 ${theme.label} 的 background 不是可解析的 hex：${theme.background}`)
  const selection = parseRgba(theme.selectionBackground)
  if (!selection) throw new Error(`主题 ${theme.label} 的 selectionBackground 不是 rgba：${theme.selectionBackground}`)
  return { canvas, slab: blendOverCanvas(canvas, selection.rgb, selection.alpha) }
}

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

// ==================== 选区色契约 ====================

describe('selectionBackground 选区可见性', () => {
  it.each(CONCRETE_THEMES)('theme %s declares selectionBackground as rgba with alpha', (_name, theme) => {
    const selection = parseRgba(theme.selectionBackground)
    // 反例守门：不透明 hex 会被 xterm 强制改写成固定 30%，选区强度无法按主题调节
    expect(selection, `selectionBackground 必须是 rgba()：${theme.selectionBackground}`).not.toBeNull()
    expect(selection!.alpha).toBeGreaterThan(0)
    expect(selection!.alpha).toBeLessThanOrEqual(1)
  })

  it.each(CONCRETE_THEMES)('theme %s selection slab is clearly distinguishable from canvas', (_name, theme) => {
    const { canvas, slab } = resolveSlab(theme)
    // 反例守门：solarized-light 曾用 #eee8d5（Δ=7.5），浅色主题上等于没有选中反馈
    expect(colorDistance(slab, canvas)).toBeGreaterThanOrEqual(MIN_SELECTION_VISIBILITY)
  })

  it.each(CONCRETE_THEMES)('theme %s keeps default foreground readable on selection slab', (_name, theme) => {
    const { canvas, slab } = resolveSlab(theme)
    const foreground = parseHex(theme.foreground)
    expect(foreground, `前景色不是可解析 hex：${theme.foreground}`).not.toBeNull()

    const onCanvas = contrastRatio(foreground!, canvas)
    const onSlab = contrastRatio(foreground!, slab)
    // 默认前景色本就低于 AA 的主题（Solarized）只守住 3:1，其余必须守住 4.5:1
    const floor = onCanvas < MIN_TEXT_CONTRAST ? MIN_TEXT_CONTRAST_FALLBACK : MIN_TEXT_CONTRAST
    expect(
      onSlab,
      `前景色 ${theme.foreground} 在选区块上对比度 ${onSlab.toFixed(2)}（画布上 ${onCanvas.toFixed(2)}），低于下限 ${floor}`,
    ).toBeGreaterThanOrEqual(floor)
  })

  it('every shipped theme selection is at least 3:1 away from the canvas in luminance', () => {
    // 可见性的亮度口径复核：ΔRGB 达标的同时，选区块不能与画布同亮度（否则淡色主题糊成一片）
    for (const [name, theme] of CONCRETE_THEMES) {
      const { canvas, slab } = resolveSlab(theme)
      expect(contrastRatio(slab, canvas), `${name} 选区块与画布底色亮度过于接近`).toBeGreaterThanOrEqual(1.15)
    }
  })

  it('system placeholder palette mirrors dark (it is never handed to xterm, kept for parity)', () => {
    // 'system' 由 resolveTerminalTheme 解析为 dark/light，本条目的值不生效；
    // 仍要求与 dark 一致，避免色板自相矛盾误导后续维护
    expect(TERMINAL_THEMES.system.selectionBackground).toBe(TERMINAL_THEMES.dark.selectionBackground)
  })

  it('rejects an opaque selection color (regression witness for the old palettes)', () => {
    // 反例：修复前的 solarized-light 选区色。若哪天把断言阈值放松到能放过它，
    // 这条会先红——证明可见性断言不是恒真
    const legacy = { ...TERMINAL_THEMES['solarized-light'], selectionBackground: '#eee8d5' }
    const canvas = parseHex(legacy.background)!
    const legacySlab = blendOverCanvas(canvas, parseHex(legacy.selectionBackground)!, 0.3)
    expect(colorDistance(legacySlab, canvas)).toBeLessThan(MIN_SELECTION_VISIBILITY)
  })
})

describe('selectionFrameColor', () => {
  it('derives the selection-mode frame from the theme cursor color at fixed opacity', () => {
    expect(selectionFrameColor(TERMINAL_THEMES.nord)).toBe('rgba(216, 222, 233, 0.45)')
    expect(selectionFrameColor(TERMINAL_THEMES['claude-code-light'])).toBe('rgba(217, 119, 6, 0.45)')
  })

  it('resolves shorthand hex cursors to their expanded channels', () => {
    const theme = { ...TERMINAL_THEMES.dark, cursor: '#0af' }
    expect(selectionFrameColor(theme)).toBe('rgba(0, 170, 255, 0.45)')
  })

  it('falls back to transparent when cursor is not a hex color (no throw)', () => {
    // 反例：未解析的 var() 串（system 占位条目）不得让终端页渲染抛错
    const theme = { ...TERMINAL_THEMES.system, cursor: 'var(--mobile-accent)' }
    expect(selectionFrameColor(theme)).toBe('transparent')
  })
})
