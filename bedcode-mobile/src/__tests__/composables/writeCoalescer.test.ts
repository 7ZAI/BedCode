/**
 * writeCoalescer 单元测试
 *
 * 验证 rAF 合并写入行为：同帧多次 write 合并为一次 term.write，
 * 避免 TUI 高频输出时 WebGL 双缓冲重影。
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { createWriteCoalescer } from '@/composables/writeCoalescer'
import type { Terminal } from '@xterm/xterm'

function makeMockTerminal(): Terminal {
  const writeMock = vi.fn()
  return {
    write: writeMock,
    element: document.createElement('div'),
  } as unknown as Terminal
}

describe('createWriteCoalescer', () => {
  let rafCallbacks: FrameRequestCallback[]

  beforeEach(() => {
    rafCallbacks = []
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
      rafCallbacks.push(cb)
      return rafCallbacks.length
    })
    vi.stubGlobal('cancelAnimationFrame', () => {})
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('同帧多次 write 合并为一次 terminal.write', () => {
    const term = makeMockTerminal()
    const coalescer = createWriteCoalescer(term)

    const d1 = new Uint8Array([1, 2, 3])
    const d2 = new Uint8Array([4, 5])
    const d3 = new Uint8Array([6, 7, 8, 9])
    coalescer(d1)
    coalescer(d2)
    coalescer(d3)

    expect(term.write).not.toHaveBeenCalled()
    expect(rafCallbacks).toHaveLength(1)

    rafCallbacks[0](0)
    expect(term.write).toHaveBeenCalledTimes(1)
    const combined = (term.write as ReturnType<typeof vi.fn>).mock.calls[0][0] as Uint8Array
    expect(Array.from(combined)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9])
  })

  it('flush 后下一帧再次入队可正常 flush', () => {
    const term = makeMockTerminal()
    const coalescer = createWriteCoalescer(term)

    coalescer(new Uint8Array([1]))
    rafCallbacks[0](0)
    expect(term.write).toHaveBeenCalledTimes(1)

    coalescer(new Uint8Array([2, 3]))
    expect(rafCallbacks).toHaveLength(2)
    rafCallbacks[1](0)
    expect(term.write).toHaveBeenCalledTimes(2)
    const second = (term.write as ReturnType<typeof vi.fn>).mock.calls[1][0] as Uint8Array
    expect(Array.from(second)).toEqual([2, 3])
  })

  it('单次 write 也走 rAF，不直接调用', () => {
    const term = makeMockTerminal()
    const coalescer = createWriteCoalescer(term)

    coalescer(new Uint8Array([42]))
    expect(term.write).not.toHaveBeenCalled()
    expect(rafCallbacks).toHaveLength(1)

    rafCallbacks[0](0)
    expect(term.write).toHaveBeenCalledTimes(1)
  })

  it('累积超过阈值时立即 flush（不走 rAF）', () => {
    const term = makeMockTerminal()
    const coalescer = createWriteCoalescer(term)

    // 256KB 阈值，200KB + 100KB 累积到 300KB 时立即 flush
    coalescer(new Uint8Array(200 * 1024))
    expect(term.write).not.toHaveBeenCalled()
    expect(rafCallbacks).toHaveLength(1)

    coalescer(new Uint8Array(100 * 1024))
    expect(term.write).toHaveBeenCalledTimes(1)
    // 立即 flush 取消了挂起的 rAF
    expect(rafCallbacks).toHaveLength(1)
  })

  it('terminal 已 dispose 时 flush 静默丢弃', () => {
    const term = makeMockTerminal()
    const coalescer = createWriteCoalescer(term)

    coalescer(new Uint8Array([1, 2]))
    ;(term as unknown as { element: HTMLElement | undefined }).element = undefined

    rafCallbacks[0](0)
    expect(term.write).not.toHaveBeenCalled()
  })

  it('dispose 取消挂起的 rAF 并清空缓冲', () => {
    const term = makeMockTerminal()
    const coalescer = createWriteCoalescer(term)

    coalescer(new Uint8Array([1, 2, 3]))
    coalescer.dispose()
    expect(rafCallbacks).toHaveLength(1)

    rafCallbacks[0](0)
    expect(term.write).not.toHaveBeenCalled()
  })

  it('dispose 之后再次 write 会重新调度 rAF', () => {
    const term = makeMockTerminal()
    const coalescer = createWriteCoalescer(term)

    coalescer(new Uint8Array([1]))
    coalescer.dispose()
    expect(rafCallbacks).toHaveLength(1)

    coalescer(new Uint8Array([2]))
    expect(rafCallbacks).toHaveLength(2)
    rafCallbacks[1](0)
    expect(term.write).toHaveBeenCalledTimes(1)
  })
})
