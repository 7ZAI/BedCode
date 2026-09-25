/**
 * 终端写入管线（插件侧迁移版）行为契约测试
 *
 * 被测对象：`useTerminalWritePipeline`（自宿主 `useTerminalWritePipeline.ts`
 * 迁入，数据源解耦为 `attachSource()` 注入接口）。
 *
 * 行为契约来源：
 * - 宿主 `src/__tests__/integration/terminal-flow.test.ts`（输出流渲染/缺口/
 *   截断/重同步契约——本测试聚焦写入管线的纯逻辑层，通道层留宿主集成测试）；
 * - 宿主实现逐字逻辑（分片/水线/补刷/队列合并/截断提示）。
 *
 * 硬性门禁对照（unit-test-discipline）：
 * - 每条契约有来源（宿主代码分支 / 集成测试用例名）；
 * - 正例 + 反例 + 边界覆盖；
 * - 断言用返回值/状态/副作用（xterm 写入调用、补刷定时器、提示回调），
 *   不用恒真断言。
 */

import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'
import { ref, shallowRef } from 'vue'
import { useTerminalWritePipeline } from '../composables/terminal/useTerminalWritePipeline'
import type { TerminalKernelContext } from '../composables/terminal/terminalKernel'

/** rAF 桩：同步执行回调（fake timers 不驱动浏览器帧调度） */
function stubRaf() {
  let rafCb: FrameRequestCallback | null = null
  vi.stubGlobal(
    'requestAnimationFrame',
    vi.fn((cb: FrameRequestCallback) => {
      rafCb = cb
      return 1
    }),
  )
  vi.stubGlobal('cancelAnimationFrame', vi.fn())
  return () => {
    if (rafCb) {
      const cb = rafCb
      rafCb = null
      cb(performance.now())
    }
  }
}

/** 构造最小可用 kernel ctx（terminal 挂 mock xterm） */
function makeCtx(overrides?: Partial<TerminalKernelContext>) {
  const ctx: TerminalKernelContext = {
    terminalRef: shallowRef(null as any),
    fitAddonRef: shallowRef(null as any),
    webglAddonRef: shallowRef(null as any),
    terminalHostRef: ref(null),
    isLinux: ref(false),
    isUserScrolling: ref(false),
    bgImageUrl: ref(''),
    getSession: () => null,
    callbacks: {
      getTheme: () => ({}),
      syncTerminalSize: () => {},
      applyResize: () => {},
      applyDprFit: () => {},
      fitAndRefresh: () => {},
      rebuildRenderer: () => {},
      scrollToBottom: vi.fn(),
    },
    ...overrides,
  }
  return ctx
}

/** mock xterm Terminal：只暴露 write/clear/refresh/rows/cols/element */
function makeTerminal() {
  const writes: Uint8Array[] = []
  return {
    writes,
    write: vi.fn((d: Uint8Array) => writes.push(d)),
    clear: vi.fn(),
    refresh: vi.fn(),
    rows: 24,
    cols: 80,
    element: { isConnected: true },
  }
}

describe('useTerminalWritePipeline（插件迁移版）', () => {
  let fireRaf: () => void
  beforeEach(() => {
    vi.useFakeTimers()
    fireRaf = stubRaf()
  })
  afterEach(() => {
    vi.useRealTimers()
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
  })

  it('入队输出合并为单次 write（正例）：多帧同 rAF 合并一块字节，一次 terminal.write', async () => {
    const terminal = makeTerminal()
    const ctx = makeCtx({ terminalRef: shallowRef(terminal as any) })
    const notify = vi.fn()
    const log = vi.fn()
    const pipeline = useTerminalWritePipeline(ctx, { notifyTruncated: notify, logTruncated: log })
    const source = pipeline.attachSource()

    source.onData({ data: new Uint8Array([0x68, 0x69]) }) // "hi"
    source.onData({ data: new Uint8Array([0x0a]) })
    expect(terminal.write).not.toHaveBeenCalled() // rAF 前不写

    fireRaf() // 触发 rAF 帧
    await vi.advanceTimersByTimeAsync(0)
    expect(terminal.write).toHaveBeenCalledTimes(1)
    const combined = terminal.write.mock.calls[0][0] as Uint8Array
    expect([...combined]).toEqual([0x68, 0x69, 0x0a])
    expect(notify).not.toHaveBeenCalled()
  })

  it('空帧丢弃（边界）：data.length=0 不入队不写', async () => {
    const terminal = makeTerminal()
    const ctx = makeCtx({ terminalRef: shallowRef(terminal as any) })
    const pipeline = useTerminalWritePipeline(ctx, { notifyTruncated: vi.fn(), logTruncated: vi.fn() })
    const source = pipeline.attachSource()

    source.onData({ data: new Uint8Array(0) })
    fireRaf()
    await vi.advanceTimersByTimeAsync(0)
    expect(terminal.write).not.toHaveBeenCalled()
  })

  it('终端未就绪丢弃不 panic（反例/防御）：terminalRef 为 null 时 flush 安全清队', async () => {
    const ctx = makeCtx({ terminalRef: shallowRef(null as any) })
    const pipeline = useTerminalWritePipeline(ctx, { notifyTruncated: vi.fn(), logTruncated: vi.fn() })
    const source = pipeline.attachSource()

    source.onData({ data: new Uint8Array([1, 2, 3]) })
    fireRaf()
    await vi.advanceTimersByTimeAsync(0) // flush：terminal 空 → 清队返回
    expect(() => pipeline.dispose()).not.toThrow()
  })

  it('大块分片 + 水线让出（正例）：>64KiB 拆块，累积 256KiB 让出主线程一次', async () => {
    const terminal = makeTerminal()
    const ctx = makeCtx({ terminalRef: shallowRef(terminal as any) })
    const pipeline = useTerminalWritePipeline(ctx, { notifyTruncated: vi.fn(), logTruncated: vi.fn() })
    const source = pipeline.attachSource()

    // 300 KiB（> MAX_WRITE_CHUNK=64KiB，> WRITE_YIELD_THRESHOLD=256KiB）
    const big = new Uint8Array(300 * 1024).fill(0x61)
    source.onData({ data: big })
    fireRaf()

    // flush 是 async（分片内 await setTimeout(0)）：需推过宏任务
    await vi.advanceTimersByTimeAsync(0)

    // 分片：300KiB / 64KiB = 5 次 write（整除向上）
    const chunkWrites = terminal.write.mock.calls.filter(
      (c) => (c[0] as Uint8Array).length === 64 * 1024,
    )
    expect(chunkWrites.length).toBeGreaterThanOrEqual(4)
    // 水线让出：至少一次 setTimeout(0) 宏任务（fake timers 下用 advanceTimersByTimeAsync 推过）
    await vi.advanceTimersByTimeAsync(0)
    // 总字节 = 300KiB（最后一块补齐）
    const total = terminal.writes.reduce((s, w) => s + w.length, 0)
    expect(total).toBe(300 * 1024)
  })

  it('onReset 清屏 + 置补刷（正例）：terminal.clear 调用一次', async () => {
    const terminal = makeTerminal()
    const ctx = makeCtx({ terminalRef: shallowRef(terminal as any) })
    const pipeline = useTerminalWritePipeline(ctx, { notifyTruncated: vi.fn(), logTruncated: vi.fn() })
    const source = pipeline.attachSource()

    source.onReset()
    expect(terminal.clear).toHaveBeenCalledTimes(1)
  })

  it('onTruncated 首次提示 + 后续仅日志（边界）：notify 一次，log 每次', async () => {
    const terminal = makeTerminal()
    const ctx = makeCtx({ terminalRef: shallowRef(terminal as any) })
    const notify = vi.fn()
    const log = vi.fn()
    const pipeline = useTerminalWritePipeline(ctx, { notifyTruncated: notify, logTruncated: log })
    const source = pipeline.attachSource()

    source.onTruncated(1024)
    source.onTruncated(2048)

    expect(notify).toHaveBeenCalledTimes(1) // 仅首次打扰用户
    expect(log).toHaveBeenCalledTimes(2) // 每次都有后台日志
  })

  it('resetTruncatedNotified 后再次截断可重新提示（正例）', async () => {
    const terminal = makeTerminal()
    const ctx = makeCtx({ terminalRef: shallowRef(terminal as any) })
    const notify = vi.fn()
    const pipeline = useTerminalWritePipeline(ctx, { notifyTruncated: notify, logTruncated: vi.fn() })
    const source = pipeline.attachSource()

    source.onTruncated(1)
    pipeline.resetTruncatedNotified()
    source.onTruncated(2)
    expect(notify).toHaveBeenCalledTimes(2)
  })

  it('回放静止补刷（正例）：onData 后 REPLAY_IDLE_MS 无新数据 → terminal.refresh 一次', async () => {
    const terminal = makeTerminal()
    const ctx = makeCtx({ terminalRef: shallowRef(terminal as any) })
    const pipeline = useTerminalWritePipeline(ctx, { notifyTruncated: vi.fn(), logTruncated: vi.fn() })
    const source = pipeline.attachSource()

    // 回放静止补刷由 armReplayRefresh 单独驱动（宿主 onReset 后置补刷路径）
    source.onReset()
    await vi.advanceTimersByTimeAsync(250)
    expect(terminal.refresh).toHaveBeenCalledTimes(1)
    expect(terminal.refresh).toHaveBeenCalledWith(0, 23) // rows-1
  })

  it('dispose 清理挂起调度（正例）：dispose 后入队不再写、无泄漏', async () => {
    const terminal = makeTerminal()
    const ctx = makeCtx({ terminalRef: shallowRef(terminal as any) })
    const pipeline = useTerminalWritePipeline(ctx, { notifyTruncated: vi.fn(), logTruncated: vi.fn() })
    const source = pipeline.attachSource()

    source.onData({ data: new Uint8Array([1]) })
    pipeline.dispose()
    fireRaf() // dispose 已取消 rAF → 不应有写入
    await vi.advanceTimersByTimeAsync(10)
    expect(terminal.write).not.toHaveBeenCalled() // dispose 后 rAF 已取消
  })
})
