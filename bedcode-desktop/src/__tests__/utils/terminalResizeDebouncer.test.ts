/**
 * TerminalResizeDebouncer 单元测试（Seam A：纯逻辑 + vitest fake timers）
 *
 * 覆盖 resize 分层契约：高度变化立即应用；仅宽度变化 100ms 防抖合并；
 * 宽高同时变化高度优先立即；flush 最终尺寸必达且不重复触发；
 * dispose 后挂起防抖不再触发；等值喂入（subpixel 抖动）不触发；
 * 小缓冲（<200 行）立即应用、大缓冲保持防抖、行数不可知保守处理；
 * 0/NaN/负尺寸（最小化、display:none）no-op 不污染状态；
 * isVisible 注入的不可见窗口分支：不可见时挂起（不触发、不启动计时器、
 * 取消遗留计时器）、恢复可见后 flush 一次性兑现最新尺寸、flush 不可见时不兑现。
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

  it('小缓冲（<200 行）：仅宽度变化也立即应用，不走防抖', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({
      onApply,
      getBufferLength: () => 100,
    })
    d.resize(800, 600)
    onApply.mockClear()

    d.resize(900, 600)
    expect(onApply).toHaveBeenCalledTimes(1)
    vi.advanceTimersByTime(500)
    expect(onApply).toHaveBeenCalledTimes(1) // 无挂起防抖补发
  })

  it('大缓冲（≥200 行）：仅宽度变化保持防抖', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({
      onApply,
      getBufferLength: () => 200,
    })
    d.resize(800, 600)
    onApply.mockClear()

    d.resize(900, 600)
    expect(onApply).not.toHaveBeenCalled()
    vi.advanceTimersByTime(100)
    expect(onApply).toHaveBeenCalledTimes(1)
  })

  it('buffer 行数不可知（null）：保守按防抖处理', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({
      onApply,
      getBufferLength: () => null,
    })
    d.resize(800, 600)
    onApply.mockClear()

    d.resize(900, 600)
    expect(onApply).not.toHaveBeenCalled()
    vi.advanceTimersByTime(100)
    expect(onApply).toHaveBeenCalledTimes(1)
  })

  it('0/NaN/负尺寸（最小化、display:none）：no-op 不记录；恢复后正常尺寸仍正确触发', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply })
    d.resize(800, 600)
    onApply.mockClear()

    // 非法喂入全部被忽略
    d.resize(0, 0)
    d.resize(-1, 600)
    d.resize(Number.NaN, 600)
    expect(onApply).not.toHaveBeenCalled()

    // 恢复真实尺寸：等值（与 last 一致）不触发
    d.resize(800, 600)
    expect(onApply).not.toHaveBeenCalled()
    // 恢复真实尺寸：变化正常触发（宽度防抖 / 高度立即）
    d.resize(900, 600)
    vi.advanceTimersByTime(100)
    expect(onApply).toHaveBeenCalledTimes(1)
    d.resize(900, 700)
    expect(onApply).toHaveBeenCalledTimes(2)
  })

  it('isVisible=true 时与无注入行为一致（高度立即 / 宽度 100ms 防抖）', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply, isVisible: () => true })

    // 首次喂入视为高度确立 → 立即应用
    d.resize(800, 600)
    expect(onApply).toHaveBeenCalledTimes(1)

    // 仅宽度变化 → 100ms 防抖：99ms 时仍是首次应用的 1 次
    d.resize(900, 600)
    vi.advanceTimersByTime(99)
    expect(onApply).toHaveBeenCalledTimes(1)
    vi.advanceTimersByTime(1)
    expect(onApply).toHaveBeenCalledTimes(2)

    // 高度变化 → 立即应用
    d.resize(900, 700)
    expect(onApply).toHaveBeenCalledTimes(3)
  })

  it('isVisible=false（不可见）：resize 不触发 onApply 且不启动防抖计时器', () => {
    const onApply = vi.fn()
    const d = new TerminalResizeDebouncer({ onApply, isVisible: () => false })

    // 宽度、高度、首次喂入均被挂起
    d.resize(800, 600)
    d.resize(820, 600)
    d.resize(900, 700)
    expect(onApply).not.toHaveBeenCalled()
    // 无计时器被启动：推进时间也无补发
    vi.advanceTimersByTime(500)
    expect(onApply).not.toHaveBeenCalled()
  })

  it('不可见期间多次 resize：恢复可见后 flush 仅触发一次且只兑现最新尺寸', () => {
    const onApply = vi.fn()
    const isVisible = vi.fn(() => true)
    const d = new TerminalResizeDebouncer({ onApply, isVisible })
    // 可见期建立基线（首次喂入立即应用）
    d.resize(800, 600)
    expect(onApply).toHaveBeenCalledTimes(1)

    // 进入不可见期：连续多次 resize（820→900→950），全部挂起、只记最新
    isVisible.mockReturnValue(false)
    d.resize(820, 600)
    d.resize(900, 600)
    d.resize(950, 600)
    expect(onApply).toHaveBeenCalledTimes(1)

    // 恢复可见 → flush 一次性兑现（中间尺寸 820/900 不各自触发）
    isVisible.mockReturnValue(true)
    d.flush()
    expect(onApply).toHaveBeenCalledTimes(2)

    // 最新尺寸已记录：等值喂入不再触发；无挂起时 flush 为 no-op
    d.resize(950, 600)
    expect(onApply).toHaveBeenCalledTimes(2)
    d.flush()
    expect(onApply).toHaveBeenCalledTimes(2)
    vi.advanceTimersByTime(500)
    expect(onApply).toHaveBeenCalledTimes(2)
  })

  it('不可见时 flush 不兑现且不清除挂起：恢复可见后再 flush 兑现一次', () => {
    const onApply = vi.fn()
    const isVisible = vi.fn(() => true)
    const d = new TerminalResizeDebouncer({ onApply, isVisible })
    d.resize(800, 600)
    onApply.mockClear()

    isVisible.mockReturnValue(false)
    d.resize(880, 600)
    // 仍不可见：flush 不触发、挂起内容保留
    d.flush()
    expect(onApply).not.toHaveBeenCalled()

    isVisible.mockReturnValue(true)
    d.flush()
    expect(onApply).toHaveBeenCalledTimes(1)
    // 兑现后无残留：再 flush 为 no-op
    d.flush()
    expect(onApply).toHaveBeenCalledTimes(1)
  })

  it('不可见挂起优先于小缓冲立即应用（对齐 VS Code 不可见分支在前）', () => {
    const onApply = vi.fn()
    const isVisible = vi.fn(() => false)
    const d = new TerminalResizeDebouncer({
      onApply,
      isVisible,
      getBufferLength: () => 100, // 小缓冲（<200 行）
    })

    d.resize(800, 600)
    expect(onApply).not.toHaveBeenCalled() // 不可见时连小缓冲也不立即应用

    isVisible.mockReturnValue(true)
    d.flush()
    expect(onApply).toHaveBeenCalledTimes(1)
  })

  it('可见期遗留的防抖计时器在不可见 resize 时被取消（无后台触发、无重复兑现）', () => {
    const onApply = vi.fn()
    const isVisible = vi.fn(() => true)
    const d = new TerminalResizeDebouncer({ onApply, isVisible })
    d.resize(800, 600)
    onApply.mockClear()

    // 可见期挂起一个宽度防抖（900）
    d.resize(900, 600)

    // 进入不可见并再次 resize：遗留计时器必须被取消
    isVisible.mockReturnValue(false)
    d.resize(950, 600)

    // 原 100ms 到期点不再触发（不可见期间的实验性后台应用被消除）
    vi.advanceTimersByTime(100)
    expect(onApply).not.toHaveBeenCalled()

    // 恢复可见 → flush 一次性兑现最新尺寸（950）
    isVisible.mockReturnValue(true)
    d.flush()
    expect(onApply).toHaveBeenCalledTimes(1)
    vi.advanceTimersByTime(500)
    expect(onApply).toHaveBeenCalledTimes(1)
  })
})
