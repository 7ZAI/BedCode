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
 * - C5 一致性锁：宿主内边距常量与插件 `TerminalPreview.vue` 的
 *   `:deep(.xterm) { padding-inline: … }` 实际值一致（2026-09-27 加呼吸位后，
 *   预测必须同步扣掉，否则预测列数比实际网格多约 2 列）
 * - C6 正例：`resolveTerminalGrid` 真的把内边距计入列数（扣掉 gutter 后少 2 列）
 * - C7 边界：退化输入（宽高 / cell 非正）→ null，不返回半成品网格
 */
import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'
import {
  resolveTerminalCellFont,
  computeDesktopInitialTerminalSize,
  resolveTerminalGrid,
  resolveGutterPx,
  TERMINAL_HOST_GUTTER_REM,
  TERMINAL_WINDOW_CHROME_PX,
} from '@/utils/terminalInitialSize'
import { PLATFORM_UI_SCALE } from '@/composables/useFontSize'

/** 插件渲染端字体栈真源（宿主不 import 插件，此处按文件读取做一致性锁） */
const PLUGIN_THEMES_PATH = 'wasm-apps/terminal-session/src/utils/terminal/terminalThemes.ts'
const HOST_INITIAL_SIZE_PATH = 'src/utils/terminalInitialSize.ts'
/** 插件终端组件（xterm 宿主内边距真源） */
const PLUGIN_PREVIEW_PATH = 'wasm-apps/terminal-session/src/components/terminal/TerminalPreview.vue'

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

describe('终端初始网格：宿主内边距口径', () => {
  /** 从插件组件源码抓 `:deep(.xterm)` 规则里的 `padding-inline: <值>` */
  function extractXtermPaddingInline(source: string): string {
    const rule = source.match(/:deep\(\.xterm\)\s*\{([\s\S]*?)\n\}/)
    if (!rule) throw new Error('未找到 :deep(.xterm) 规则（正则失配 = 锁失效）')
    const pad = rule[1].match(/padding-inline:\s*([\d.]+rem)/)
    if (!pad) throw new Error('未找到 :deep(.xterm) 的 padding-inline rem 值（锁失效）')
    return pad[1]
  }

  it('C5 一致性锁：宿主内边距常量 = 插件 .xterm 的 padding-inline × 2', () => {
    const pluginPadding = extractXtermPaddingInline(readFileSync(PLUGIN_PREVIEW_PATH, 'utf-8'))
    const perSide = Number.parseFloat(pluginPadding)

    expect(perSide, 'padding-inline 必须是 rem（写死 px 会与 --ui-scale 脱钩）').toBeGreaterThan(0)
    expect(TERMINAL_HOST_GUTTER_REM).toBe(perSide * 2)
  })

  it('C6 正例：预测列数扣掉宿主内边距（16px gutter + 7.4px cell → 少 2 列）', () => {
    const termW = 690
    const termH = 798
    const cellW = 7.4
    const cellH = 14.07

    const withGutter = resolveTerminalGrid(termW, termH, cellW, cellH, 16)
    const withoutGutter = resolveTerminalGrid(termW, termH, cellW, cellH, 0)

    expect(withGutter).not.toBeNull()
    expect(withoutGutter).not.toBeNull()
    // 行数与内边距无关（gutter 只在水平方向）
    expect(withGutter!.rows).toBe(withoutGutter!.rows)
    // 16px ÷ 7.4px ≈ 2 列：漏算内边距就会多预测 2 列 → PTY 与网格列数不一致
    expect(withoutGutter!.cols - withGutter!.cols).toBe(2)
  })

  it('C6c 正例：字间距叠加进格宽（cellW + letterSpacing，列数变少）', () => {
    // 与插件渲染端口径一致：xterm device.cell.width = char.width + letterSpacing。
    // 预测必须叠加同一增量，否则字间距 > 0 时预测列数偏多 → PTY 起步网格与渲染不一致
    const termW = 690
    const termH = 798
    const cellW = 10
    const cellH = 20
    const base = resolveTerminalGrid(termW, termH, cellW, cellH, 16)
    const spaced = resolveTerminalGrid(termW, termH, cellW + 2, cellH, 16)

    expect(base!.cols).toBe(Math.floor((690 - 16 - 14) / 10))
    // 间距只膨胀格宽（横向）：行数不变
    expect(spaced!.rows).toBe(base!.rows)
    expect(spaced!.cols).toBe(Math.floor((690 - 16 - 14) / 12))
  })

  it('C6b 正例：内边距随根字号缩放（--ui-scale 变化时换算跟着变）', () => {
    expect(resolveGutterPx(16)).toBe(TERMINAL_HOST_GUTTER_REM * 16)
    // 根字号带小数时取整到整像素（亚像素差异对列数无影响，但必须是整数）
    expect(Number.isInteger(resolveGutterPx(22.8))).toBe(true)
    expect(resolveGutterPx(22.8)).toBeCloseTo(TERMINAL_HOST_GUTTER_REM * 22.8, 0)
    // 根字号取不到时按 16px 兜底，不返回 NaN
    expect(resolveGutterPx(Number.NaN)).toBe(TERMINAL_HOST_GUTTER_REM * 16)
  })

  it('C7 边界：退化输入返回 null（不把半成品网格发给 PTY）', () => {
    expect(resolveTerminalGrid(0, 798, 7.4, 14, 16)).toBeNull()
    expect(resolveTerminalGrid(690, 0, 7.4, 14, 16)).toBeNull()
    expect(resolveTerminalGrid(690, 798, 0, 14, 16)).toBeNull()
    expect(resolveTerminalGrid(690, 798, 7.4, 0, 16)).toBeNull()
    expect(resolveTerminalGrid(690, 798, 7.4, 14, -1)).toBeNull()
  })

  it('C7b 边界：chrome 常量仍是工具条+状态条（40+24）', () => {
    expect(TERMINAL_WINDOW_CHROME_PX).toBe(64)
  })
})
