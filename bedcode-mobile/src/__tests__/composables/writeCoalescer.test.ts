/**
 * writeCoalescer 单元测试
 *
 * 验证移动端写入管线行为：
 * - 同帧多次 write 合并为一次 term.write（DEC 2026 同步输出已由 xterm.js 6.0
 *   内置，应用侧不再包裹）
 * - 单次 write 超过 64KB 拆块，让 xterm parser 让出主线程
 * - 累积超过 256KB 阈值立即 flush（移动端特殊处理）
 * - rAF 暂停（最小化/后台）时 100ms 兜底定时器清空队列
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { createWriteCoalescer } from '@/composables/writeCoalescer'
import type { Terminal } from '@xterm/xterm'

// 单次 write 上限（与实现保持一致）
const MAX_WRITE_CHUNK = 64 * 1024

/** 断言写入的数据 = 原始 payload */
function expectRaw(writeMock: ReturnType<typeof vi.fn>, payload: number[]) {
  const written = writeMock.mock.calls[writeMock.mock.calls.length - 1][0] as Uint8Array
  expect(Array.from(written)).toEqual(payload)
}

function makeMockTerminal(): Terminal {
  const writeMock = vi.fn()
  return {
    write: writeMock,
    element: document.createElement('div'),
  } as unknown as Terminal
}

describe('createWriteCoalescer', () => {
  it('rAF 合并默认关闭：每个事件直接写入，不经合并管线', () => {
    const term = makeMockTerminal()
    const coalescer = createWriteCoalescer(term)

    const d1 = new Uint8Array([1, 2, 3])
    const d2 = new Uint8Array([4, 5])
    coalescer(d1)
    coalescer(d2)

    // 两次独立 write，且未注册任何 rAF
    expect(term.write).toHaveBeenCalledTimes(2)
    expect(term.write.mock.calls[0][0]).toEqual(d1)
    expect(term.write.mock.calls[1][0]).toEqual(d2)
    expect(rafCallbacks).toHaveLength(0)
  })

  it('rAF 合并关闭时 dispose 幂等无害', () => {
    const term = makeMockTerminal()
    const coalescer = createWriteCoalescer(term)
    coalescer(new Uint8Array([1]))
    coalescer.dispose()
    expect(term.write).toHaveBeenCalledTimes(1)
  })

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
    const coalescer = createWriteCoalescer(term, { enableRafCoalesce: true })

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
    expectRaw(term.write, [1, 2, 3, 4, 5, 6, 7, 8, 9])
  })

  it('flush 后下一帧再次入队可正常 flush', () => {
    const term = makeMockTerminal()
    const coalescer = createWriteCoalescer(term, { enableRafCoalesce: true })

    coalescer(new Uint8Array([1]))
    rafCallbacks[0](0)
    expect(term.write).toHaveBeenCalledTimes(1)

    coalescer(new Uint8Array([2, 3]))
    expect(rafCallbacks).toHaveLength(2)
    rafCallbacks[1](0)
    expect(term.write).toHaveBeenCalledTimes(2)
    expectRaw(term.write, [2, 3])
  })

  it('单次 write 也走 rAF，不直接调用', () => {
    const term = makeMockTerminal()
    const coalescer = createWriteCoalescer(term, { enableRafCoalesce: true })

    coalescer(new Uint8Array([42]))
    expect(term.write).not.toHaveBeenCalled()
    expect(rafCallbacks).toHaveLength(1)

    rafCallbacks[0](0)
    expect(term.write).toHaveBeenCalledTimes(1)
  })

  it('超过 64KB 拆块写入', () => {
    const term = makeMockTerminal()
    const coalescer = createWriteCoalescer(term, { enableRafCoalesce: true })

    // 96KB 载荷 → 2 块（零拷贝 subarray 切片）
    const payload = new Uint8Array(MAX_WRITE_CHUNK + 32 * 1024).fill(7)
    coalescer(payload)
    rafCallbacks[0](0)

    expect(term.write).toHaveBeenCalledTimes(2)
    const calls = term.write.mock.calls.map(c => Array.from(c[0] as Uint8Array))
    expect(calls[0]).toEqual(Array.from(payload.subarray(0, MAX_WRITE_CHUNK)))
    expect(calls[1]).toEqual(Array.from(payload.subarray(MAX_WRITE_CHUNK)))
  })

  it('累积超过 256KB 阈值时立即 flush（取消挂起 rAF，仍拆块）', () => {
    const term = makeMockTerminal()
    const coalescer = createWriteCoalescer(term, { enableRafCoalesce: true })

    coalescer(new Uint8Array(200 * 1024))
    expect(term.write).not.toHaveBeenCalled()
    expect(rafCallbacks).toHaveLength(1)

    coalescer(new Uint8Array(100 * 1024))
    // 300KB → 5 块，立即执行
    expect(term.write).toHaveBeenCalledTimes(5)
    // 立即 flush 取消了挂起的 rAF
    expect(rafCallbacks).toHaveLength(1)

    // 内容完整性：分块拼接 = 原始 300KB
    const written = term.write.mock.calls.map(c => c[0] as Uint8Array)
    const joined = new Uint8Array(300 * 1024)
    let offset = 0
    for (const chunk of written) {
      joined.set(chunk, offset)
      offset += chunk.byteLength
    }
    expect(offset).toBe(300 * 1024)
    // 每块不超过上限
    for (const chunk of written) {
      expect(chunk.byteLength).toBeLessThanOrEqual(MAX_WRITE_CHUNK)
    }
  })

  it('rAF 暂停时 100ms 兜底定时器清空队列', () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    try {
      const term = makeMockTerminal()
      const coalescer = createWriteCoalescer(term, { enableRafCoalesce: true })

      coalescer(new Uint8Array([1, 2, 3]))
      expect(term.write).not.toHaveBeenCalled()

      vi.advanceTimersByTime(99)
      expect(term.write).not.toHaveBeenCalled()

      vi.advanceTimersByTime(1)
      expect(term.write).toHaveBeenCalledTimes(1)
      expectRaw(term.write, [1, 2, 3])
    } finally {
      vi.useRealTimers()
    }
  })

  it('terminal 已 dispose 时 flush 静默丢弃', () => {
    const term = makeMockTerminal()
    const coalescer = createWriteCoalescer(term, { enableRafCoalesce: true })

    coalescer(new Uint8Array([1, 2]))
    ;(term as unknown as { element: HTMLElement | undefined }).element = undefined

    rafCallbacks[0](0)
    expect(term.write).not.toHaveBeenCalled()
  })

  it('dispose 取消挂起的 rAF 与兜底定时器，清空缓冲', () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    try {
      const term = makeMockTerminal()
      const coalescer = createWriteCoalescer(term, { enableRafCoalesce: true })

      coalescer(new Uint8Array([1, 2, 3]))
      coalescer.dispose()

      vi.advanceTimersByTime(200)
      expect(term.write).not.toHaveBeenCalled()
    } finally {
      vi.useRealTimers()
    }
  })

  it('dispose 之后再次 write 会重新调度 rAF', () => {
    const term = makeMockTerminal()
    const coalescer = createWriteCoalescer(term, { enableRafCoalesce: true })

    coalescer(new Uint8Array([1]))
    coalescer.dispose()
    expect(rafCallbacks).toHaveLength(1)

    coalescer(new Uint8Array([2]))
    expect(rafCallbacks).toHaveLength(2)
    rafCallbacks[1](0)
    expect(term.write).toHaveBeenCalledTimes(1)
  })
})
