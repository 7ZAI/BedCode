/**
 * 终端初始网格预测：字体口径契约（2026-09-26 修复）
 *
 * 背景：`computeDesktopInitialTerminalSize` 给 PTY 定初始行列。旧实现用
 * 「设置原值字号 + Windows 字体栈」测量，而插件渲染端（TerminalPreview.initTerminal）
 * 在 Linux 上实际用「字号 × 1.15 + 系统等宽字体栈」→ 预测 cell 偏小 → 行数偏多
 * （实测 84 行 vs 实际 ~73 行）→ 首帧内容高于容器、视口"滚动到底" → 终端第一行被
 * 顶部 40px 工具条裁掉（用户现象：终端内容侵占窗体标题栏）。
 *
 * 行为契约：
 * - C1 正例：Linux → 字号叠加 PLATFORM_UI_SCALE，字体栈 = 系统等宽栈
 * - C2 反例：非 Linux → 原字号、默认栈（Linux 因子不得泄漏到其它平台）
 * - C3 一致性锁：宿主 LINUX_FONT_STACK 与插件渲染端（terminalThemes.ts）逐字相同
 *   （宿主不 import 插件，双份定义必须同步，否则口径再次漂移且无感知）
 * - C4 边界：非法字号（0 / 负数 / NaN）直接返回 null，不进入测量
 */
import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'
import { resolveTerminalCellFont, computeDesktopInitialTerminalSize } from '@/utils/terminalInitialSize'
import { PLATFORM_UI_SCALE } from '@/composables/useFontSize'

/** 插件渲染端字体栈真源（宿主不 import 插件，此处按文件读取做一致性锁） */
const PLUGIN_THEMES_PATH = 'wasm-apps/terminal-session/src/utils/terminal/terminalThemes.ts'
const HOST_INITIAL_SIZE_PATH = 'src/utils/terminalInitialSize.ts'

/** 从源码文本抓 `LINUX_FONT_STACK = "<字面量>"` 的值（跨行赋值） */
function extractLinuxFontStack(source: string): string {
  const match = source.match(/LINUX_FONT_STACK\s*=\s*(["'])([\s\S]*?)\1/)
  if (!match) throw new Error('源码中未找到 LINUX_FONT_STACK 字面量（正则失配 = 锁失效）')
  return match[2]
}

describe('终端初始网格字体口径', () => {
  it('C1 Linux：字号叠加 PLATFORM_UI_SCALE 且使用系统等宽字体栈（正例）', () => {
    const cell = resolveTerminalCellFont(12, true)

    expect(cell.fontSize).toBeCloseTo(12 * PLATFORM_UI_SCALE, 5)
    expect(cell.fontFamily).toContain('DejaVu Sans Mono')
  })

  it('C2 非 Linux：字号取设置原值 + 默认栈（反例，Linux 因子不得泄漏）', () => {
    const cell = resolveTerminalCellFont(12, false)

    expect(cell.fontSize).toBe(12)
    expect(cell.fontFamily).toContain('Cascadia Mono')
    expect(cell.fontFamily).not.toContain('DejaVu Sans Mono')
    // 两平台字体栈必须不同（写成一样 = 口径漂移，锁在 C1/C2 双双失守前先拦住）
    expect(resolveTerminalCellFont(12, false).fontFamily).not.toBe(
      resolveTerminalCellFont(12, true).fontFamily,
    )
  })

  it('C3 一致性锁：宿主与插件渲染端的 LINUX_FONT_STACK 逐字相同', () => {
    const hostStack = extractLinuxFontStack(readFileSync(HOST_INITIAL_SIZE_PATH, 'utf-8'))
    const pluginStack = extractLinuxFontStack(readFileSync(PLUGIN_THEMES_PATH, 'utf-8'))

    expect(hostStack.length, '两侧都必须抓到真实字面量（防空串恒真）').toBeGreaterThan(20)
    expect(hostStack).toBe(pluginStack)
  })

  it('C4 边界：非法字号直接返回 null，不落到窗口/字体测量', async () => {
    await expect(computeDesktopInitialTerminalSize(0)).resolves.toBeNull()
    await expect(computeDesktopInitialTerminalSize(-3)).resolves.toBeNull()
    await expect(computeDesktopInitialTerminalSize(Number.NaN)).resolves.toBeNull()
  })
})
