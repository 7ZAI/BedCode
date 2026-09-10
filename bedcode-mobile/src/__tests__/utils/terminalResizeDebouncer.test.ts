/**
 * TerminalResizeDebouncer 分层防抖测试（对齐桌面端 terminalResizeDebouncer 语义）
 *
 * 覆盖：非法尺寸（≤0/NaN）忽略、小缓冲（<200）免防抖、高度变化立即、仅宽度
 * 变化防抖合并、等值不触发、flush/dispose。
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

  it('高度变化立即 onApply（含首次喂入）', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply })
    d.resize(800, 500)
    expect(onApply).toHaveBeenCalledTimes(1)
  })

  it('仅宽度变化防抖合并：窗口期内最后一次生效', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply })
    d.resize(800, 500)
    onApply.mockClear()
    d.resize(900, 500) // 仅宽度变化 → 挂起
    d.resize(1000, 500) // 重置计时器
    d.resize(1100, 500)
    expect(onApply).not.toHaveBeenCalled() // 防抖期内不触发
    vi.advanceTimersByTime(100)
    expect(onApply).toHaveBeenCalledTimes(1)
  })

  it('小缓冲（<200 行）时连纯宽度变化也立即应用', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply, getBufferLength: () => 10 })
    d.resize(800, 500)
    onApply.mockClear()
    d.resize(900, 500)
    expect(onApply).toHaveBeenCalledTimes(1) // 不防抖，立即
  })

  it('buffer 行数不可知（null）时保守走丢防抖路径', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply, getBufferLength: () => null })
    d.resize(800, 500)
    onApply.mockClear()
    d.resize(900, 500)
    expect(onApply).not.toHaveBeenCalled()
    vi.advanceTimersByTime(100)
    expect(onApply).toHaveBeenCalledTimes(1)
  })

  it('非法尺寸（≤0 / NaN）忽略：不记录不触发', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply })
    d.resize(0, 500)
    d.resize(800, 0)
    d.resize(NaN, 500)
    expect(onApply).not.toHaveBeenCalled()
    // 非法尺寸不会记录 last 值：首次合法喂入仍作首次触发
    d.resize(800, 500)
    expect(onApply).toHaveBeenCalledTimes(1)
  })

  it('等值喂入不触发、不重置计时器', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply })
    d.resize(800, 500)
    onApply.mockClear()
    d.resize(900, 500)
    d.resize(900, 500) // 等值
    vi.advanceTimersByTime(100)
    expect(onApply).toHaveBeenCalledTimes(1)
  })

  it('宽高同时变化视为高度变化：立即应用并取消挂起水平防抖', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply })
    d.resize(800, 500)
    onApply.mockClear()
    d.resize(900, 500) // 挂起
    d.resize(1000, 600) // 宽高都变 → 立即
    expect(onApply).toHaveBeenCalledTimes(1)
    vi.advanceTimersByTime(200)
    expect(onApply).toHaveBeenCalledTimes(1) // 挂起的水平防抖被取消
  })

  it('flush 立即兑现挂起的防抖尺寸', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply })
    d.resize(800, 500)
    onApply.mockClear()
    d.resize(900, 500)
    d.flush()
    expect(onApply).toHaveBeenCalledTimes(1)
    vi.advanceTimersByTime(200)
    expect(onApply).toHaveBeenCalledTimes(1) // 已兑现，后续无重复
  })

  it('无挂起时 flush 为 no-op', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply })
    d.flush()
    expect(onApply).not.toHaveBeenCalled()
  })

  it('dispose 后拒绝新输入并不触发挂起应用', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply })
    d.resize(800, 500)
    onApply.mockClear()
    d.resize(900, 500)
    d.dispose()
    d.resize(1000, 500)
    d.flush()
    vi.advanceTimersByTime(200)
    expect(onApply).not.toHaveBeenCalled()
  })
})