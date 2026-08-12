/**
 * useTuiCompat 单测（ADR-0013：移动端 TUI 滚动兼容）
 *
 * 覆盖：
 * 1. createMouseSgrSniffer — 1006h 启用 / 1006l 关闭 / CSI 跨 chunk 切分 / 无关序列忽略 / reset
 * 2. createSgrWheelSequence — 方向映射（>0 下滚 65，<0 上滚 64）/ 坐标 / 单次上限
 * 3. useTuiCompat — 双条件门控 / 节流合并 / inflight 丢弃 / 非 TUI 模式不发送
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

// Mock WS 发送命令：断言调用而不真正 invoke
const mockWsSendInput = vi.fn(() => Promise.resolve())
vi.mock('@/composables/useMobileCommands', () => ({
  wsSendInput: (...args: any[]) => mockWsSendInput(...args),
}))

// Mock Tauri invoke（useMobileCommands 内部依赖）
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

import { ref } from 'vue'
import { createMouseSgrSniffer, createSgrWheelSequence, useTuiCompat } from '@/composables/useTuiCompat'

const enc = (s: string) => new TextEncoder().encode(s)

describe('createMouseSgrSniffer', () => {
  it('1006h 启用、1006l 关闭', () => {
    const sniffer = createMouseSgrSniffer()
    expect(sniffer.enabled).toBe(false)
    sniffer.feed(enc('normal output\x1b[?1006h'))
    expect(sniffer.enabled).toBe(true)
    sniffer.feed(enc('more output\x1b[?1006l'))
    expect(sniffer.enabled).toBe(false)
    // 再次启用
    sniffer.feed(enc('\x1b[?1006h'))
    expect(sniffer.enabled).toBe(true)
  })

  it('CSI 序列跨 chunk 切分时仍能识别', () => {
    const sniffer = createMouseSgrSniffer()
    // \x1b[?1006h 被切成两半
    sniffer.feed(enc('text\x1b[?100'))
    expect(sniffer.enabled).toBe(false)
    sniffer.feed(enc('6h'))
    expect(sniffer.enabled).toBe(true)
  })

  it('跨多 chunk 切分（逐字节）仍能识别', () => {
    const sniffer = createMouseSgrSniffer()
    for (const byte of '\x1b[?1006h') {
      sniffer.feed(enc(byte))
    }
    expect(sniffer.enabled).toBe(true)
  })

  it('无关 DECSET 序列（1049h 备用屏幕）不影响鼠标状态', () => {
    const sniffer = createMouseSgrSniffer()
    sniffer.feed(enc('\x1b[?1049h\x1b[?25l'))
    expect(sniffer.enabled).toBe(false)
  })

  it('非 SGR 鼠标模式（1000/1002）不误启用', () => {
    const sniffer = createMouseSgrSniffer()
    sniffer.feed(enc('\x1b[?1002h\x1b[?1000h'))
    expect(sniffer.enabled).toBe(false)
  })

  it('reset 复位状态', () => {
    const sniffer = createMouseSgrSniffer()
    sniffer.feed(enc('\x1b[?1006h'))
    expect(sniffer.enabled).toBe(true)
    sniffer.reset()
    expect(sniffer.enabled).toBe(false)
    // reset 后尾部缓冲清空，不残留旧序列影响
    sniffer.feed(enc('6h'))
    expect(sniffer.enabled).toBe(false)
  })
})

describe('createSgrWheelSequence', () => {
  it('正 delta（向下查看）生成 button 65 下滚序列', () => {
    expect(createSgrWheelSequence(3, 12, 8)).toBe('\x1b[<65;12;8M\x1b[<65;12;8M\x1b[<65;12;8M')
  })

  it('负 delta（向上查看）生成 button 64 上滚序列', () => {
    expect(createSgrWheelSequence(-2, 5, 20)).toBe('\x1b[<64;5;20M\x1b[<64;5;20M')
  })

  it('delta 为 0 返回空串', () => {
    expect(createSgrWheelSequence(0, 1, 1)).toBe('')
  })

  it('单次发送上限 MAX_WHEEL_EVENTS_PER_SEND（10 个）', () => {
    const seq = createSgrWheelSequence(25, 1, 1)
    expect(seq.match(/\x1b\[<65;1;1M/g)?.length).toBe(10)
  })
})

describe('useTuiCompat', () => {
  beforeEach(() => {
    mockWsSendInput.mockClear()
    vi.useFakeTimers()
  })

  function mockTerminal(alt: () => boolean) {
    const listeners: (() => void)[] = []
    return {
      buffer: { active: { type: 'normal' } },
      onWriteParsed: vi.fn((cb: () => void) => {
        listeners.push(cb)
        return { dispose: vi.fn() }
      }),
      // 测试辅助：触发 onWriteParsed 前同步 buffer type
      _emitParsed() {
        ;(this.buffer.active as { type: string }).type = alt() ? 'alternate' : 'normal'
        listeners.forEach(cb => cb())
      },
    }
  }

  it('双条件门控：备用屏幕 + 1006h 同时满足才进入 TUI 模式', () => {
    let alt = false
    const term = mockTerminal(() => alt)
    const compat = useTuiCompat('s1')

    compat.attach(term as any)
    term._emitParsed()
    expect(compat.isTuiMode.value).toBe(false)

    // 仅 1006h：不在备用屏幕 → 非 TUI
    compat.feedOutput(enc('\x1b[?1006h'))
    expect(compat.isTuiMode.value).toBe(false)

    // 进入备用屏幕 → TUI 模式
    alt = true
    term._emitParsed()
    expect(compat.isTuiMode.value).toBe(true)

    // 退出备用屏幕 → 立即退出 TUI 模式
    alt = false
    term._emitParsed()
    expect(compat.isTuiMode.value).toBe(false)
    compat.dispose()
  })

  it('非 TUI 模式下 sendWheel 不发送', () => {
    const term = mockTerminal(() => false)
    const compat = useTuiCompat('s1')
    compat.attach(term as any)
    term._emitParsed()
    compat.sendWheel(3, 1, 1)
    vi.advanceTimersByTime(100)
    expect(mockWsSendInput).not.toHaveBeenCalled()
    compat.dispose()
  })

  it('TUI 模式下节流合并：窗口内多次 sendWheel 只发一次', async () => {
    let alt = true
    const term = mockTerminal(() => alt)
    const compat = useTuiCompat('s1')
    compat.attach(term as any)
    term._emitParsed()
    compat.feedOutput(enc('\x1b[?1006h'))
    expect(compat.isTuiMode.value).toBe(true)

    compat.sendWheel(2, 10, 5)
    compat.sendWheel(1, 11, 6)
    expect(mockWsSendInput).not.toHaveBeenCalled()
    await vi.advanceTimersByTimeAsync(41)
    expect(mockWsSendInput).toHaveBeenCalledTimes(1)
    // 合并为 3 个下滚事件，坐标取最新
    expect(mockWsSendInput.mock.calls[0][0]).toBe('s1')
    expect(mockWsSendInput.mock.calls[0][1]).toBe('\x1b[<65;11;6M\x1b[<65;11;6M\x1b[<65;11;6M')
    compat.dispose()
  })

  it('inflight 丢弃：上次发送未完成时到期窗口不发送（最新目标优先）', async () => {
    let alt = true
    const term = mockTerminal(() => alt)
    const compat = useTuiCompat('s1')
    compat.attach(term as any)
    term._emitParsed()
    compat.feedOutput(enc('\x1b[?1006h'))

    // 第一次发送挂起（可控 deferred）
    let resolveFirst: () => void = () => {}
    mockWsSendInput.mockReturnValueOnce(new Promise<void>(res => { resolveFirst = res }))
    compat.sendWheel(2, 1, 1)
    await vi.advanceTimersByTimeAsync(41)
    expect(mockWsSendInput).toHaveBeenCalledTimes(1)

    // 在途期间再次滚动：窗口到期应丢弃
    compat.sendWheel(3, 1, 1)
    await vi.advanceTimersByTimeAsync(41)
    expect(mockWsSendInput).toHaveBeenCalledTimes(1)

    // 完成在途发送（resolve 后 finally 复位 inflight）
    resolveFirst()
    await Promise.resolve()
    await Promise.resolve()

    // 新一轮滚动正常发送
    compat.sendWheel(1, 1, 1)
    await vi.advanceTimersByTimeAsync(41)
    expect(mockWsSendInput).toHaveBeenCalledTimes(2)
    compat.dispose()
  })

  it('dispose 清理节流定时器与状态', async () => {
    let alt = true
    const term = mockTerminal(() => alt)
    const compat = useTuiCompat('s1')
    compat.attach(term as any)
    term._emitParsed()
    compat.feedOutput(enc('\x1b[?1006h'))
    expect(compat.isTuiMode.value).toBe(true)

    compat.sendWheel(2, 1, 1)
    compat.dispose()
    await vi.advanceTimersByTimeAsync(100)
    expect(mockWsSendInput).not.toHaveBeenCalled()
    expect(compat.isTuiMode.value).toBe(false)
  })
})
