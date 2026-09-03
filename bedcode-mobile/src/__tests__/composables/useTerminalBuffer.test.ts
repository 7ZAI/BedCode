/**
 * useTerminalBuffer 单元测试（10 号票重写）
 *
 * 覆盖：subscribeSession（socket 驱动 + subscribe_ok 确认）、
 * unsubscribeSession（关闭订阅不通知后端——命令已删）、prepareSession 轮询、
 * handleDisconnect/handleSessionStopped/markSessionRunning/handleSessionRemoved、
 * registerRealtimeHandler 写队列。
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
  emit: vi.fn().mockResolvedValue(undefined),
}))

// mock 终端 socket
let capturedHandlers: Record<string, any> | null = null
let fakeSocket: {
  start: ReturnType<typeof vi.fn>
  subscribe: ReturnType<typeof vi.fn>
  sendInput: ReturnType<typeof vi.fn>
  stop: ReturnType<typeof vi.fn>
  reconnect: ReturnType<typeof vi.fn>
  isOpen: ReturnType<typeof vi.fn>
}
const createTerminalSocketMock = vi.fn()
vi.mock('@/composables/useTerminalSocket', () => ({
  createTerminalSocket: (...args: unknown[]) => createTerminalSocketMock(...args),
}))

vi.mock('@/composables/useMobileCommands', () => ({
  getTerminalWsInfo: vi.fn().mockResolvedValue({ url: 'ws://host:8765/ws/terminal/session/s1', token: 'jwt' }),
}))

import { useTerminalBufferStore } from '@/stores/terminalBuffer'
import { useTerminalBuffer } from '@/composables/useTerminalBuffer'
import type { Terminal } from '@xterm/xterm'

function setupSocket() {
  fakeSocket = {
    start: vi.fn(),
    subscribe: vi.fn(),
    sendInput: vi.fn(),
    stop: vi.fn(),
    reconnect: vi.fn(),
    isOpen: vi.fn(() => false),
    ackRendered: vi.fn(),
  }
  capturedHandlers = null
  createTerminalSocketMock.mockImplementation((handlers: unknown) => {
    capturedHandlers = handlers as Record<string, any>
    return fakeSocket
  })
  return fakeSocket
}

describe('useTerminalBuffer.subscribeSession', () => {
  let store: ReturnType<typeof useTerminalBufferStore>
  let terminalBuffer: ReturnType<typeof useTerminalBuffer>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useTerminalBufferStore()
    terminalBuffer = useTerminalBuffer()
    vi.clearAllMocks()
    setupSocket()
  })

  it('首次订阅：建 socket + 发订阅帧；subscribe_ok 后标记已订阅', async () => {
    await terminalBuffer.subscribeSession('s1')

    expect(fakeSocket.start).toHaveBeenCalledWith('s1')
    expect(fakeSocket.subscribe).toHaveBeenCalled()
    // subscribe_ok 异步到达后置 subscribed
    capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
    expect(store.getBuffer('s1')!.subscribed).toBe(true)
    expect(store.getBuffer('s1')!.phase).toBe('history')
  })

  it('已订阅会话跳过：直接返回快照元数据，不重复建连', async () => {
    await terminalBuffer.subscribeSession('s1')
    capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
    fakeSocket.start.mockClear()

    const result = await terminalBuffer.subscribeSession('s1')
    expect(result).toEqual({ snapshotSeq: 10, minSeq: 0, historyCount: 0 })
    expect(fakeSocket.start).not.toHaveBeenCalled()
  })

  it('unsubscribeSession：注销 handler + 标记未订阅 + 停 socket', async () => {
    await terminalBuffer.subscribeSession('s1')
    capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
    store.registerRealtimeHandler('s1', { onOutput: vi.fn() })

    await terminalBuffer.unsubscribeSession('s1')

    expect(fakeSocket.stop).toHaveBeenCalled()
    const buf = store.getBuffer('s1')!
    expect(buf.subscribed).toBe(false)
    expect(store.realtimeHandlers.has('s1')).toBe(false)
  })

  it('handleDisconnect：标记所有 buffer 未订阅', async () => {
    await terminalBuffer.subscribeSession('s1')
    await terminalBuffer.subscribeSession('s2')

    terminalBuffer.handleDisconnect()

    expect(store.getBuffer('s1')!.subscribed).toBe(false)
    expect(store.getBuffer('s2')!.subscribed).toBe(false)
  })

  it('handleSessionStopped：标记停止 + 停 socket（handler 保留供重启渲染）', async () => {
    await terminalBuffer.subscribeSession('s1')
    capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
    store.registerRealtimeHandler('s1', { onOutput: vi.fn() })

    await terminalBuffer.handleSessionStopped('s1')

    expect(store.getBuffer('s1')!.sessionStopped).toBe(true)
    expect(store.getBuffer('s1')!.subscribed).toBe(false)
    expect(store.getBuffer('s1')!.lastRenderedSeq).toBeNull()
    // handler 生命周期归视图：会话停止不注销
    expect(store.realtimeHandlers.has('s1')).toBe(true)
  })

  it('markSessionRunning：复位 sessionStopped（同 id 重启后可重新订阅）', async () => {
    await terminalBuffer.subscribeSession('s1')
    capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
    store.markSessionStopped('s1')
    const buf = store.getBuffer('s1')!

    terminalBuffer.markSessionRunning('s1')

    expect(buf.sessionStopped).toBe(false)
    expect(buf.subscribed).toBe(false)
  })

  it('handleSessionRemoved：清理 buffer 与 handler', async () => {
    await terminalBuffer.subscribeSession('s1')
    capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
    store.registerRealtimeHandler('s1', { onOutput: vi.fn() })

    await terminalBuffer.handleSessionRemoved('s1')

    expect(store.buffers.has('s1')).toBe(false)
    expect(store.realtimeHandlers.has('s1')).toBe(false)
  })

  it('registerRealtimeHandler：onClear 清空 xterm 并释放写队列（writeCoalescer dispose）', async () => {
    const terminal = {
      clear: vi.fn(),
      write: vi.fn(),
      dispose: vi.fn(),
      // 渲染背压接线（registerRealtimeHandler 挂 onWriteParsed）
      onWriteParsed: vi.fn(() => ({ dispose: vi.fn() })),
    } as unknown as Terminal

    terminalBuffer.registerRealtimeHandler('s1', terminal)
    const handler = store.realtimeHandlers.get('s1')!
    handler.onClear?.()

    expect(terminal.clear).toHaveBeenCalledTimes(1)
  })

  it('prepareSession：订阅确认到达 → 标记就绪（终端页 consumePrepared 消费）', async () => {
    const readyPromise = terminalBuffer.prepareSession('s1')
    // 轮询期间 subscribe_ok 到达
    setTimeout(() => {
      capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
    }, 50)

    const ready = await readyPromise
    expect(ready).toBe(true)
    expect(store.consumePrepared()).toBe('s1')
  })

  it('prepareSession：超时未确认 → 返回 false、不标记就绪（终端页自行重试）', async () => {
    vi.useFakeTimers()
    try {
      const pending = terminalBuffer.prepareSession('s1')
      await vi.advanceTimersByTimeAsync(9000)
      const ready = await pending
      expect(ready).toBe(false)
      expect(store.consumePrepared()).toBeNull()
    } finally {
      vi.useRealTimers()
    }
  })

  it('registerRealtimeHandler：历史回放路径同样喂 onRawOutput（TUI 嗅探不丢历史中的 DECSET 1006h）', async () => {
    // 场景：进入终端页前 opencode 已启用 SGR 鼠标上报，1006h 只存在于
    // 历史缓存中——回放若绕过原始字节钩子，嗅探器丢失该状态 → isTuiMode
    // 误判关闭 → 备用屏幕上触摸滚动完全失效
    await terminalBuffer.subscribeSession('s1')
    capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
    capturedHandlers!.onHistoryEnd(10)
    // 视图未挂载（无 handler）：帧仅入历史缓存
    const tuiBytes = new TextEncoder().encode('prompt\x1b[?1049h\x1b[?1006h')
    capturedHandlers!.onFrame({ data: tuiBytes, seq: 11, eventCount: 1, lastSeq: 11, isWaiting: false })
    expect(store.getBuffer('s1')!.historyCache.length).toBe(1)

    const rawFed: Uint8Array[] = []
    const written: Uint8Array[] = []
    const terminal = {
      element: document.createElement('div'),
      write: vi.fn((data: Uint8Array, cb?: () => void) => {
        written.push(data)
        cb?.()
      }),
      clear: vi.fn(),
      dispose: vi.fn(),
      onWriteParsed: vi.fn(() => ({ dispose: vi.fn() })),
    } as unknown as Terminal

    const { replayDone } = terminalBuffer.registerRealtimeHandler('s1', terminal, (d) => rawFed.push(d))
    await replayDone

    // 原始字节钩子与 xterm 写入收到相同字节：嗅探器可从回放恢复 1006h 状态
    expect(written.length).toBeGreaterThan(0)
    const fedText = rawFed.map((d) => new TextDecoder().decode(d)).join('')
    expect(fedText).toContain('\x1b[?1006h')
  })

  it('sendInput：转发到 store（socket 输入帧）', async () => {
    await terminalBuffer.subscribeSession('s1')
    capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
    capturedHandlers!.onHistoryEnd(10)
    fakeSocket.isOpen.mockReturnValue(true)

    const ok = terminalBuffer.sendInput('s1', 'echo hi', 'enter')
    expect(ok).toBe(true)
    expect(fakeSocket.sendInput).toHaveBeenCalledWith('echo hi', 'enter')
  })
})
