/**
 * writeCoalescer 单元测试
 *
 * 验证移动端写入管线行为：
 * - 同帧多次 write 合并为一次 term.write（DEC 2026 同步输出已由 xterm.js 6.0
 *   内置，应用侧不再包裹）
 * - 单次 write 超过 64KB 拆块，让 xterm parser 让出主线程
 * - 大块写入每累积 WRITE_YIELD_THRESHOLD 让出一次宏任务（防 UI 冻结）
 * - 累积超过 512KB 阈值立即 flush（移动端特殊处理）
 * - rAF 暂停（最小化/后台）时 100ms 兜底定时器清空队列
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { createWriteCoalescer, type WriteCoalescer, type WriteCoalescerOptions } from '@/composables/writeCoalescer'
import type { Terminal } from '@xterm/xterm'

// 单次 write 上限（与实现保持一致）
const MAX_WRITE_CHUNK = 64 * 1024
// 主线程让出阈值（与实现保持一致）
const WRITE_YIELD_THRESHOLD = 128 * 1024

/** 等待 in-flight async flush 把数据全部写入（轮询，不依赖让出点数：
 * 让出点 = setTimeout(0)，块数/阈值变化时层数不可预判） */
async function waitForWrites(term: Terminal, count: number): Promise<void> {
  const deadline = Date.now() + 2000
  const writeMock = term.write as unknown as { mock: { calls: unknown[][] } }
  while (writeMock.mock.calls.length < count) {
    if (Date.now() > deadline) throw new Error(`timeout: expected ${count} writes, got ${writeMock.mock.calls.length}`)
    await new Promise((resolve) => setTimeout(resolve, 5))
  }
}

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

/**
 * 测试用 helper：登记创建的 coalescer，用例结束后由 afterEach 统一 dispose。
 *
 * 背景：scheduleFlush 会挂一个 100ms 真实兜底定时器（FALLBACK_FLUSH_MS），
 * 用例不 dispose 即泄漏——定时器在 vitest teardown（afterEach 的
 * unstubAllGlobals 已移除 cancelAnimationFrame stub）之后触发时，回调里
 * 引用未定义的 cancelAnimationFrame 抛 ReferenceError，被 vitest 记为
 * unhandled error 导致整个 job 失败（CI 时序敏感 flaky）。
 */
const createdCoalescers: WriteCoalescer[] = []

function makeCoalescer(
  term: Terminal,
  options?: WriteCoalescerOptions,
): WriteCoalescer {
  const c = createWriteCoalescer(term, options)
  createdCoalescers.push(c)
  return c
}

describe('createWriteCoalescer', () => {
  it('rAF 合并默认开启：事件挂起到 rAF，不立即写入', () => {
    const term = makeMockTerminal()
    const coalescer = makeCoalescer(term)

    const d1 = new Uint8Array([1, 2, 3])
    coalescer(d1)

    // 默认合并：未注册 rAF 前不写，注册了一个 rAF
    expect(term.write).not.toHaveBeenCalled()
    expect(rafCallbacks).toHaveLength(1)
  })

  it('rAF 合并关闭（调试回退）时：每个事件直接写入，不经合并管线', () => {
    const term = makeMockTerminal()
    const coalescer = makeCoalescer(term, { enableRafCoalesce: false })

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

  it('rAF 合并关闭（调试回退）时 dispose 幂等无害', () => {
    const term = makeMockTerminal()
    const coalescer = makeCoalescer(term, { enableRafCoalesce: false })
    coalescer(new Uint8Array([1]))
    coalescer.dispose()
    expect(term.write).toHaveBeenCalledTimes(1)
  })

  let rafCallbacks: FrameRequestCallback[]

  beforeEach(() => {
    createdCoalescers.length = 0
    rafCallbacks = []
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
      rafCallbacks.push(cb)
      return rafCallbacks.length
    })
    vi.stubGlobal('cancelAnimationFrame', () => {})
  })

  afterEach(() => {
    // 清理所有登记实例的 rAF/兜底定时器，避免 teardown 后真实定时器触发（见 helper 注释）
    for (const c of createdCoalescers) c.dispose()
    createdCoalescers.length = 0
    vi.unstubAllGlobals()
  })

  it('同帧多次 write 合并为一次 terminal.write', () => {
    const term = makeMockTerminal()
    const coalescer = makeCoalescer(term, { enableRafCoalesce: true })

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
    const coalescer = makeCoalescer(term, { enableRafCoalesce: true })

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
    const coalescer = makeCoalescer(term, { enableRafCoalesce: true })

    coalescer(new Uint8Array([42]))
    expect(term.write).not.toHaveBeenCalled()
    expect(rafCallbacks).toHaveLength(1)

    rafCallbacks[0](0)
    expect(term.write).toHaveBeenCalledTimes(1)
  })

  it('超过 64KB 拆块写入', () => {
    const term = makeMockTerminal()
    const coalescer = makeCoalescer(term, { enableRafCoalesce: true })

    // 96KB 载荷 → 2 块（零拷贝 subarray 切片）
    const payload = new Uint8Array(MAX_WRITE_CHUNK + 32 * 1024).fill(7)
    coalescer(payload)
    rafCallbacks[0](0)

    expect(term.write).toHaveBeenCalledTimes(2)
    const calls = term.write.mock.calls.map(c => Array.from(c[0] as Uint8Array))
    expect(calls[0]).toEqual(Array.from(payload.subarray(0, MAX_WRITE_CHUNK)))
    expect(calls[1]).toEqual(Array.from(payload.subarray(MAX_WRITE_CHUNK)))
  })

  it('累积超过 512KB 阈值时立即 flush（取消挂起 rAF，仍拆块）', async () => {
    const term = makeMockTerminal()
    const coalescer = makeCoalescer(term, { enableRafCoalesce: true })

    coalescer(new Uint8Array(400 * 1024))
    expect(term.write).not.toHaveBeenCalled()
    expect(rafCallbacks).toHaveLength(1)

    coalescer(new Uint8Array(200 * 1024))
    // 600KB 超阈值立即 flush：flush 开始同步写前 128KB（2 块）后让出，
    // 完整 10 块（600KB / 64KB = 9.375）需等让出点消化
    expect(term.write.mock.calls.length).toBeGreaterThan(0)
    expect(term.write.mock.calls.length).toBeLessThan(10)
    // 立即 flush 取消了挂起的 rAF
    expect(rafCallbacks).toHaveLength(1)

    await waitForWrites(term, 10)

    // 内容完整性：分块拼接 = 原始 600KB
    const written = term.write.mock.calls.map(c => c[0] as Uint8Array)
    const joined = new Uint8Array(600 * 1024)
    let offset = 0
    for (const chunk of written) {
      joined.set(chunk, offset)
      offset += chunk.byteLength
    }
    expect(offset).toBe(600 * 1024)
    // 每块不超过上限
    for (const chunk of written) {
      expect(chunk.byteLength).toBeLessThanOrEqual(MAX_WRITE_CHUNK)
    }
  })

  it('超过 WRITE_YIELD_THRESHOLD 时让出主线程（分块分批写，非一次性同步写）', async () => {
    const term = makeMockTerminal()
    const coalescer = makeCoalescer(term, { enableRafCoalesce: true })

    // 300KB = 5 块（64KB）；128KB 阈值 → 写 2 块（128KB）让出一次，再 2 块让出一次，末块收尾
    const payload = new Uint8Array(300 * 1024).fill(9)
    coalescer(payload)
    rafCallbacks[0](0)

    // 让出点前的同步窗口：已写满一个 WRITE_YIELD_THRESHOLD（128KB / 64KB = 2 块），
    // 未全部写完——证明非一次性同步写
    expect(term.write.mock.calls.length).toBe(WRITE_YIELD_THRESHOLD / MAX_WRITE_CHUNK)

    await waitForWrites(term, 5)
    expect(term.write).toHaveBeenCalledTimes(5)
    const calls = term.write.mock.calls.map(c => c[0] as Uint8Array)
    const joined = new Uint8Array(300 * 1024)
    let offset = 0
    for (const chunk of calls) {
      joined.set(chunk, offset)
      offset += chunk.byteLength
    }
    expect(offset).toBe(300 * 1024)
    for (const chunk of calls) {
      expect(chunk.byteLength).toBeLessThanOrEqual(MAX_WRITE_CHUNK)
    }
  })

  it('让出期间新入队数据由同一 flush 的 while 轮次消费（无滞留无双写）', async () => {
    const term = makeMockTerminal()
    const coalescer = makeCoalescer(term, { enableRafCoalesce: true })

    // 第一批 200KB（128KB 阈值 → 写 2 块后让出）；让出期间入队第二批
    coalescer(new Uint8Array(200 * 1024))
    rafCallbacks[0](0)
    // 同步窗口：已写满一个 WRITE_YIELD_THRESHOLD（2 块），flush 在让出点挂起
    expect(term.write.mock.calls.length).toBe(WRITE_YIELD_THRESHOLD / MAX_WRITE_CHUNK)

    // 让出期间新数据入队：调 write 但不再注册新 rAF（当前 flush 会消费）
    coalescer(new Uint8Array(200 * 1024))
    expect(rafCallbacks).toHaveLength(1)

    await waitForWrites(term, 8)
    // 200KB→4 块 + 200KB→4 块 = 8 块，且全部由同一 flush 消化（无新 rAF）
    expect(term.write).toHaveBeenCalledTimes(8)
  })

  it('rAF 暂停时 100ms 兜底定时器清空队列', () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    try {
      const term = makeMockTerminal()
      const coalescer = makeCoalescer(term, { enableRafCoalesce: true })

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
    const coalescer = makeCoalescer(term, { enableRafCoalesce: true })

    coalescer(new Uint8Array([1, 2]))
    ;(term as unknown as { element: HTMLElement | undefined }).element = undefined

    rafCallbacks[0](0)
    expect(term.write).not.toHaveBeenCalled()
  })

  it('dispose 取消挂起的 rAF 与兜底定时器，清空缓冲', () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    try {
      const term = makeMockTerminal()
      const coalescer = makeCoalescer(term, { enableRafCoalesce: true })

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
    const coalescer = makeCoalescer(term, { enableRafCoalesce: true })

    coalescer(new Uint8Array([1]))
    coalescer.dispose()
    expect(rafCallbacks).toHaveLength(1)

    coalescer(new Uint8Array([2]))
    expect(rafCallbacks).toHaveLength(2)
    rafCallbacks[1](0)
    expect(term.write).toHaveBeenCalledTimes(1)
  })
})
