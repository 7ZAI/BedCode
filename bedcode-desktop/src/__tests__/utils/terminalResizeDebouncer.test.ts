/**
 * TerminalResizeDebouncer 单元测试（Seam A：纯逻辑 + vitest fake timers）
 *
 * 覆盖 resize 分层契约：高度变化立即应用；仅宽度变化 100ms 防抖合并；
 * 宽高同时变化高度优先立即；flush 最终尺寸必达且不重复触发；
 * dispose 后挂起防抖不再触发；等值喂入（subpixel 抖动）不触发。
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { TerminalResizeDebouncer } from '@/utils/terminalResizeDebouncer'

describe('TerminalResizeDebouncer', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('高度变化立即触发 onApply（无需推进计时器）', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply })

    // 首次喂入视为高度确立 → 立即应用
    d.resize(800, 600)
    expect(onApply).toHaveBeenCalledTimes(1)

    // 高度增加 → 立即
    d.resize(800, 700)
    expect(onApply).toHaveBeenCalledTimes(2)

    // 高度减小（双向变化均立即）→ 立即
    d.resize(800, 500)
    expect(onApply).toHaveBeenCalledTimes(3)
    vi.advanceTimersByTime(500)
    expect(onApply).toHaveBeenCalledTimes(3) // 无挂起的防抖补发
  })

  it('仅宽度变化按默认 100ms 防抖：99ms 不触发，100ms 触发一次', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply })
    d.resize(800, 600)
    onApply.mockClear()

    d.resize(900, 600)
    vi.advanceTimersByTime(99)
    expect(onApply).not.toHaveBeenCalled()
    vi.advanceTimersByTime(1)
    expect(onApply).toHaveBeenCalledTimes(1)
  })

  it('窗口内连续多次宽度变化只触发一次（最后一次生效）', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply })
    d.resize(800, 600)
    onApply.mockClear()

    d.resize(820, 600)
    vi.advanceTimersByTime(60)
    d.resize(860, 600)
    vi.advanceTimersByTime(60)
    d.resize(900, 600)
    // 前两次计时器已被重置，未到期
    expect(onApply).not.toHaveBeenCalled()
    vi.advanceTimersByTime(100)
    expect(onApply).toHaveBeenCalledTimes(1)
    // 继续推进无补发
    vi.advanceTimersByTime(500)
    expect(onApply).toHaveBeenCalledTimes(1)
  })

  it('宽高同时变化 → 高度优先立即触发，取消挂起的水平防抖', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply })
    d.resize(800, 600)
    onApply.mockClear()

    // 先挂起一个水平防抖
    d.resize(820, 600)
    // 宽高同时变化 → 立即应用并取消挂起防抖
    d.resize(900, 700)
    expect(onApply).toHaveBeenCalledTimes(1)
    vi.advanceTimersByTime(500)
    expect(onApply).toHaveBeenCalledTimes(1) // 被取消的防抖不补发
  })

  it('宽度变化后 flush() 立即触发；再推进计时器不重复触发', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply })
    d.resize(800, 600)
    onApply.mockClear()

    d.resize(1000, 600)
    d.flush()
    expect(onApply).toHaveBeenCalledTimes(1)
    vi.advanceTimersByTime(500)
    expect(onApply).toHaveBeenCalledTimes(1)
  })

  it('无挂起防抖时 flush() 为 no-op', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply })
    d.resize(800, 600)
    onApply.mockClear()
    d.flush()
    expect(onApply).not.toHaveBeenCalled()
  })

  it('dispose() 后挂起的防抖不再触发，resize/flush 均为 no-op', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply })
    d.resize(800, 600)
    onApply.mockClear()

    d.resize(900, 600)
    d.dispose()
    vi.advanceTimersByTime(500)
    expect(onApply).not.toHaveBeenCalled()

    d.resize(900, 700)
    d.flush()
    vi.advanceTimersByTime(500)
    expect(onApply).not.toHaveBeenCalled()
  })

  it('等值喂入（subpixel 抖动）不触发也不重置挂起计时器', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply })
    d.resize(800, 600)
    onApply.mockClear()

    // 挂起水平防抖
    d.resize(820, 600)
    vi.advanceTimersByTime(60)
    // 等值重复喂入：不重置计时器
    d.resize(820, 600)
    vi.advanceTimersByTime(40)
    expect(onApply).toHaveBeenCalledTimes(1) // 60+40=100，按原计时器到期
  })

  it('horizontalDelayMs 可自定义', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply, horizontalDelayMs: 250 })
    d.resize(800, 600)
    onApply.mockClear()

    d.resize(900, 600)
    vi.advanceTimersByTime(249)
    expect(onApply).not.toHaveBeenCalled()
    vi.advanceTimersByTime(1)
    expect(onApply).toHaveBeenCalledTimes(1)
  })
})
