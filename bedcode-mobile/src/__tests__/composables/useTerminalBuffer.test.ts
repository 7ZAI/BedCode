/**
 * useTerminalBuffer 单元测试（票 05：终端流新协议）
 *
 * 覆盖：subscribeSession（fresh subscribe：已订阅也重播）、unsubscribeSession
 * （退出页面 = 注销 handler + 关闭链路）、prepareSession 轮询、
 * handleDisconnect/handleSessionStopped/markSessionRunning/handleSessionRemoved、
 * registerRealtimeHandler 写队列（onClear / onRawOutput）。
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

const eventHandlers: Record<string, ((payload: unknown) => void) | null> = {}
const listenMock = vi.fn()
const emitMock = vi.fn().mockResolvedValue(undefined)

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => (listenMock as (...a: unknown[]) => unknown)(...args),
  emit: (...args: unknown[]) => (emitMock as (...a: unknown[]) => unknown)(...args),
}))

const cmd = vi.hoisted(() => ({
  terminalSubscribe: vi.fn(async () => {}),
  terminalUnsubscribe: vi.fn(async () => {}),
  terminalUnsubscribeAll: vi.fn(async () => {}),
  terminalRemove: vi.fn(async () => {}),
  terminalSendInput: vi.fn(async () => {}),
  terminalAckRendered: vi.fn(async () => {}),
  terminalPageSubscribe: vi.fn(async () => {}),
  terminalPageUnsubscribe: vi.fn(async () => {}),
  // Rust 链路状态对账（订阅幂等无事件路径的收敛兜底）：默认未订阅
  terminalGetState: vi.fn(async (sessionId: string) => ({
    sessionId,
    phase: 'idle',
    cursor: 0,
    acked: 0,
    stopped: false,
    subscribed: false,
  })),
}))
vi.mock('@/composables/useMobileCommands', () => cmd)

// 用户可见提示（历史截断通知）与 i18n 与本用例断言的链路行为无关，按项目惯例替身化
vi.mock('@/composables/useToast', () => ({
  useToast: () => ({ success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() }),
}))
vi.mock('@/locales', () => ({ default: { global: { t: (key: string) => key } } }))

import { useTerminalBufferStore } from '@/stores/terminalBuffer'
import { useTerminalBuffer } from '@/composables/useTerminalBuffer'
import type { Terminal } from '@xterm/xterm'

function emitState(sessionId: string, phase: string, detail?: string, retryInMs?: number) {
  eventHandlers['terminal-state']!({
    payload: { session_id: sessionId, phase, detail, retry_in_ms: retryInMs },
  })
}

async function flushAsync(n = 3) {
  for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0))
}

describe('useTerminalBuffer（票 05：新协议）', () => {
  let store: ReturnType<typeof useTerminalBufferStore>
  let terminalBuffer: ReturnType<typeof useTerminalBuffer>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useTerminalBufferStore()
    terminalBuffer = useTerminalBuffer()
    vi.clearAllMocks()
    eventHandlers['terminal-state'] = null
    ;(vi.mocked(listenMock).mockImplementation as any)(async (name: string, cb: (p: unknown) => void) => {
      eventHandlers[name] = cb
      return () => {}
    })
  })

  it('首次订阅：触发 terminalSubscribe；收 subscribed（phase=live）确认已订阅', async () => {
    await terminalBuffer.subscribeSession('s1')
    await flushAsync()

    expect(cmd.terminalSubscribe).toHaveBeenCalledWith('s1')
    expect(store.getBuffer('s1')!.subscribed).toBe(false)

    // 新协议无独立 history 阶段：收 subscribed 即 live
    emitState('s1', 'live', 'subscribed')
    expect(store.getBuffer('s1')!.subscribed).toBe(true)
    expect(store.getBuffer('s1')!.phase).toBe('live')
  })

  it('已订阅会话再次订阅：仍触发 terminalSubscribe（fresh subscribe 重播）', async () => {
    await terminalBuffer.subscribeSession('s1')
    emitState('s1', 'live')
    cmd.terminalSubscribe.mockClear()
    const result = await terminalBuffer.subscribeSession('s1')
    expect(result).toBeNull()
    expect(cmd.terminalSubscribe).toHaveBeenCalledWith('s1')
  })

  it('unsubscribeSession：注销 handler + 关闭链路（页面退出语义：不得后台常拉）', async () => {
    await terminalBuffer.subscribeSession('s1')
    emitState('s1', 'live')
    store.registerRealtimeHandler('s1', { onOutput: vi.fn() })
    await flushAsync()

    await terminalBuffer.unsubscribeSession('s1')
    await flushAsync()

    // 页面退出：注销 handler + 关闭链路；重进时重订阅回放补齐
    expect(store.realtimeHandlers.has('s1')).toBe(false)
    expect(cmd.terminalUnsubscribe).toHaveBeenCalledWith('s1')
  })

  it('handleDisconnect：标记所有 buffer 未订阅 + 全量取消 Rust 链路', async () => {
    await terminalBuffer.subscribeSession('s1')
    emitState('s1', 'live')
    terminalBuffer.handleDisconnect()
    await flushAsync()
    expect(cmd.terminalUnsubscribeAll).toHaveBeenCalled()
    expect(store.getBuffer('s1')!.subscribed).toBe(false)
  })

  it('handleSessionStopped：标记停止 + 取消订阅（handler 保留供重启渲染）', async () => {
    await terminalBuffer.subscribeSession('s1')
    emitState('s1', 'live')
    store.registerRealtimeHandler('s1', { onOutput: vi.fn() })

    await terminalBuffer.handleSessionStopped('s1')
    await flushAsync()

    expect(cmd.terminalUnsubscribe).toHaveBeenCalledWith('s1')
    expect(store.getBuffer('s1')!.sessionStopped).toBe(true)
    expect(store.getBuffer('s1')!.subscribed).toBe(false)
    expect(store.getBuffer('s1')!.lastRenderedOffset).toBeNull()
    // handler 生命周期归视图：会话停止不注销
    expect(store.realtimeHandlers.has('s1')).toBe(true)
  })

  it('markSessionRunning：仅复位 sessionStopped，不订阅（订阅由页面驱动）', async () => {
    await terminalBuffer.subscribeSession('s1')
    store.markSessionStopped('s1')
    const buf = store.getBuffer('s1')!

    terminalBuffer.markSessionRunning('s1')
    await flushAsync()

    expect(buf.sessionStopped).toBe(false)
    // 页面驱动的订阅路径（TerminalView watch → subscribeWithRetry）负责重新订阅
    cmd.terminalSubscribe.mockClear()
    expect(cmd.terminalSubscribe).not.toHaveBeenCalled()
  })

  it('handleSessionRemoved：清理 buffer 与 handler + terminalRemove', async () => {
    await terminalBuffer.subscribeSession('s1')
    store.registerRealtimeHandler('s1', { onOutput: vi.fn() })

    await terminalBuffer.handleSessionRemoved('s1')
    await flushAsync()

    expect(cmd.terminalRemove).toHaveBeenCalledWith('s1')
    expect(store.buffers.has('s1')).toBe(false)
    expect(store.realtimeHandlers.has('s1')).toBe(false)
  })

  it('registerRealtimeHandler：onClear 清空 xterm 并释放写队列（writeCoalescer dispose）', async () => {
    const terminal = {
      clear: vi.fn(),
      write: vi.fn(),
      dispose: vi.fn(),
      onWriteParsed: vi.fn(() => ({ dispose: vi.fn() })),
    } as unknown as Terminal

    terminalBuffer.registerRealtimeHandler('s1', terminal)
    await flushAsync()
    const handler = store.realtimeHandlers.get('s1')!
    handler.onClear?.()

    expect(terminal.clear).toHaveBeenCalledTimes(1)
  })

  it('prepareSession：订阅确认到达 → 返回就绪（终端页挂载时重新订阅触发回放）', async () => {
    const readyPromise = terminalBuffer.prepareSession('s1')
    // 轮询期间 Rust 链路状态到 live
    setTimeout(() => {
      emitState('s1', 'live')
    }, 50)

    const ready = await readyPromise
    expect(ready).toBe(true)
    expect(store.getBuffer('s1')!.subscribed).toBe(true)
  })

  it('prepareSession：超时未确认 → 返回 false（终端页自行重试）', async () => {
    vi.useFakeTimers()
    try {
      const pending = terminalBuffer.prepareSession('s1')
      await vi.advanceTimersByTimeAsync(9000)
      const ready = await pending
      expect(ready).toBe(false)
    } finally {
      vi.useRealTimers()
    }
  })

  it('registerRealtimeHandler：输出字节喂 onRawOutput（TUI 嗅探不丢任何内容）', async () => {
    await terminalBuffer.subscribeSession('s1')
    emitState('s1', 'live')

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

    terminalBuffer.registerRealtimeHandler('s1', terminal, (d) => rawFed.push(d))
    // 经段2 通道投递裸字节（含转义序列）：写队列 + 原始字节钩子都收到
    const channelCall = cmd.terminalPageSubscribe.mock.calls.filter((c) => c[0] === 's1').pop()
    const channel = channelCall?.[1] as { onmessage: ((m: ArrayBuffer) => void) | null } | undefined
    const bytes = new TextEncoder().encode('prompt\x1b[?1049h\x1b[?1006h')
    const buf = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer
    channel!.onmessage?.(buf)
    await flushAsync()

    expect(written.length).toBeGreaterThan(0)
    const fedText = rawFed.map((d) => new TextDecoder().decode(d)).join('')
    expect(fedText).toContain('\x1b[?1006h')
  })

  it('sendInput：转发到 store（terminalSendInput 命令）', async () => {
    await terminalBuffer.subscribeSession('s1')
    emitState('s1', 'live')
    const ok = terminalBuffer.sendInput('s1', 'echo hi', 'enter')
    expect(ok).toBe(true)
    await flushAsync()
    expect(cmd.terminalSendInput).toHaveBeenCalledWith('s1', 'echo hi', 'enter')
  })

  // ==================== 链路重连提示（2026-10-04） ====================
  //
  // 契约：断线退避时 `phase` 恒为 connecting，与「首次订阅中」无法区分，两者
  // 用户含义相反。必须由 `reconnecting` 单独建模，否则终端静默冻结
  // （ui-ux-pro-max ux 域 Severity High 的「No feedback」反模式）。

  /** 惰性注册：store 的 terminal-state 监听由 subscribeSession 触发
   * ensureEventListeners（异步），不先挂上监听 emitState 会直接报
   * `eventHandlers.terminal-state is not a function`（另一个测试文件同款） */
  async function armListeners(sessionId = 's1') {
    await terminalBuffer.subscribeSession(sessionId)
    await flushAsync()
  }

  it('重连提示：reconnecting 事件置位标记（正例）', async () => {
    await armListeners()
    store.ensureBuffer('s1')
    emitState('s1', 'live')
    expect(store.getBuffer('s1')?.reconnecting).toBe(false)

    // 断线 → Rust 发 reconnecting（此时 phase 已是 connecting）
    emitState('s1', 'connecting', 'reconnecting')
    expect(store.getBuffer('s1')?.reconnecting).toBe(true)
    expect(store.getBuffer('s1')?.reconnectInMs).toBeNull()
  })

  it('重连提示：reconnect_scheduled 携带倒计时（正例）', async () => {
    await armListeners()
    store.ensureBuffer('s1')
    emitState('s1', 'connecting', 'reconnecting')
    emitState('s1', 'connecting', 'reconnect_scheduled', 4000)

    expect(store.getBuffer('s1')?.reconnecting).toBe(true)
    expect(store.getBuffer('s1')?.reconnectInMs).toBe(4000)
  })

  it('重连提示：恢复 live 后标记与倒计时一并撤销（副作用）', async () => {
    await armListeners()
    store.ensureBuffer('s1')
    emitState('s1', 'connecting', 'reconnect_scheduled', 4000)
    emitState('s1', 'live')

    // 不撤销会残留一个继续跑的倒计时，连接早已恢复却还在报「Xs 后重连」
    expect(store.getBuffer('s1')?.reconnecting).toBe(false)
    expect(store.getBuffer('s1')?.reconnectInMs).toBeNull()
    expect(store.getBuffer('s1')?.reconnectReceivedAt).toBeNull()
  })

  it('重连提示：reconnect_scheduled 记录到达时刻作为倒计时基准（M-02）', async () => {
    await armListeners()
    store.ensureBuffer('s1')
    // Rust 每轮顺序（terminal_link.rs Err(Io) 分支）：reconnecting（仅报状态）
    // → reconnect_scheduled（带等待时长）。前者把倒计时清掉，后者才设基准。
    emitState('s1', 'connecting', 'reconnecting')
    expect(store.getBuffer('s1')?.reconnectInMs).toBeNull()
    expect(store.getBuffer('s1')?.reconnectReceivedAt).toBeNull()

    const before = Date.now()
    emitState('s1', 'connecting', 'reconnect_scheduled', 4000)
    const receivedAt = store.getBuffer('s1')?.reconnectReceivedAt ?? 0
    expect(store.getBuffer('s1')?.reconnectInMs).toBe(4000)
    expect(receivedAt).toBeGreaterThanOrEqual(before)
    expect(receivedAt).toBeLessThanOrEqual(Date.now())
  })

  it('重连提示：stopped 后标记与倒计时一并清除（M-03，Rust 不再发事件）', async () => {
    await armListeners()
    store.ensureBuffer('s1')
    emitState('s1', 'connecting', 'reconnect_scheduled', 4000)
    expect(store.getBuffer('s1')?.reconnecting).toBe(true)

    // 退避中途命中 stopped（strikes 耗尽/会话停止）：若不清除，横幅永久卡
    // 「正在重连/30s 后重连」——Rust 侧链路放弃后不再发事件，无人收尾
    emitState('s1', 'idle', 'stopped')
    expect(store.getBuffer('s1')?.reconnecting).toBe(false)
    expect(store.getBuffer('s1')?.reconnectInMs).toBeNull()
    expect(store.getBuffer('s1')?.reconnectReceivedAt).toBeNull()
  })

  it('重连提示：session_missing 后标记与倒计时一并清除（M-03）', async () => {
    await armListeners()
    store.ensureBuffer('s1')
    emitState('s1', 'connecting', 'reconnect_scheduled', 4000)

    emitState('s1', 'idle', 'session_missing')
    expect(store.getBuffer('s1')?.reconnecting).toBe(false)
    expect(store.getBuffer('s1')?.reconnectInMs).toBeNull()
    expect(store.getBuffer('s1')?.reconnectReceivedAt).toBeNull()
  })

  it('重连提示：unsubscribed 后标记与倒计时一并清除（M-03）', async () => {
    await armListeners()
    store.ensureBuffer('s1')
    emitState('s1', 'connecting', 'reconnect_scheduled', 4000)

    emitState('s1', 'idle', 'unsubscribed')
    expect(store.getBuffer('s1')?.reconnecting).toBe(false)
    expect(store.getBuffer('s1')?.reconnectInMs).toBeNull()
    expect(store.getBuffer('s1')?.reconnectReceivedAt).toBeNull()
  })

  it('重连提示：首次订阅中的 connecting 不得误报为重连（反例）', async () => {
    await armListeners()
    store.ensureBuffer('s1')
    // 无 detail 的普通 connecting = 正在建立连接，不是断线
    emitState('s1', 'connecting', 'auth')
    emitState('s1', 'connecting', 'subscribed_sent')

    expect(store.getBuffer('s1')?.reconnecting).toBe(false)
  })

  it('重连提示：reconnect_scheduled 缺 retry_in_ms 时不编造数字（边界）', async () => {
    await armListeners()
    store.ensureBuffer('s1')
    emitState('s1', 'connecting', 'reconnect_scheduled')

    expect(store.getBuffer('s1')?.reconnecting).toBe(true)
    // 显示「正在重连…」而非凭空显示「0 秒后重连」
    expect(store.getBuffer('s1')?.reconnectInMs).toBeNull()
  })

  it('重连提示：新建 buffer 不得继承重连态（反例）', async () => {
    await armListeners('s2')
    store.ensureBuffer('s2')
    emitState('s2', 'connecting', 'reconnecting')
    expect(store.getBuffer('s2')?.reconnecting).toBe(true)

    store.clearAllBuffers()
    const fresh = store.ensureBuffer('s2')
    expect(fresh.reconnecting).toBe(false)
    expect(fresh.reconnectInMs).toBeNull()
  })
})