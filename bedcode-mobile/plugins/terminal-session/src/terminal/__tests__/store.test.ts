/**
 * terminalBuffer store 单元测试（票 05：终端流新协议 + 本地字节计数）
 *
 * 覆盖：fresh subscribe 触发（已订阅也重播）、terminal-state 事件同步
 * phase/subscribed、裸字节输出交付（本地计数 + hasRenderedContent）、
 * resync 清屏重锚（有内容才清/提示）、停止帧、生命周期（停止/恢复/删除/
 * 页面进出 = 关闭链路）、输入门控、渲染背压 ack。
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

// 票 15：域宿主服务 mock —— 命令面按旧断言面分发到 cmd spy；
// 事件经内存 handler 表注入；页面字节流经 SDK getMobileApi().openTerminalStream 捕获
const mocks = vi.hoisted(() => {
  const cmd = {
    terminalSubscribe: vi.fn(async (_sid: string) => {}),
    terminalUnsubscribe: vi.fn(async (_sid: string) => {}),
    terminalUnsubscribeAll: vi.fn(async () => {}),
    terminalRemove: vi.fn(async (_sid: string) => {}),
    terminalSendInput: vi.fn(async (_sid: string, _data: string, _key?: string | null) => {}),
    terminalAckRendered: vi.fn(async (_sid: string, _offset: number) => {}),
    terminalGetState: vi.fn(async (sessionId: string) => ({
      sessionId,
      phase: 'idle',
      cursor: 0,
      acked: 0,
      stopped: false,
      subscribed: false,
    })),
  }
  const dispatch: Record<string, (args: any) => unknown> = {
    'terminal-session.subscribe': (a) => cmd.terminalSubscribe(a?.sessionId),
    'terminal-session.unsubscribe': (a) => cmd.terminalUnsubscribe(a?.sessionId),
    'terminal-session.unsubscribe-all': () => cmd.terminalUnsubscribeAll(),
    'terminal-session.remove': (a) => cmd.terminalRemove(a?.sessionId),
    'terminal-session.send-input': (a) => cmd.terminalSendInput(a?.sessionId, a?.data, a?.specialKey),
    'terminal-session.ack-rendered': (a) => cmd.terminalAckRendered(a?.sessionId, a?.offset),
    'terminal-session.get-state': (a) => cmd.terminalGetState(a?.sessionId),
  }
  return {
    cmd,
    invoke: vi.fn(async (command: string, args?: any) => dispatch[command]?.(args)),
    eventHandlers: {} as Record<string, ((payload: unknown) => void)[]>,
    /** openTerminalStream 捕获（页面通道登记/注销） */
    streamCalls: [] as {
      sessionId: string
      onBytes: (b: Uint8Array) => void
      disposed: boolean
      dispose: () => void
    }[],
  }
})

vi.mock('../host', () => ({
  invokeTerminal: (command: string, args?: any) => mocks.invoke(command, args),
  t: (key: string) => key,
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
  logger: { log: () => {}, debug: () => {}, info: () => {}, warn: () => {}, error: () => {} },
  storage: { get: async () => undefined, set: async () => {}, delete: async () => {} },
  events: {
    on: (event: string, handler: (payload: unknown) => void) => {
      ;(mocks.eventHandlers[event] ??= []).push(handler)
      return { dispose: () => {} }
    },
  },
}))

vi.mock('@binblink/bedcode-plugin-sdk-mobile', () => ({
  getMobileApi: () => ({
    mockSessionId: null,
    openTerminalStream: async (sessionId: string, onBytes: (b: Uint8Array) => void) => {
      const entry: {
        sessionId: string
        onBytes: (b: Uint8Array) => void
        disposed: boolean
        dispose: () => void
      } = {
        sessionId,
        onBytes,
        disposed: false,
        dispose: () => {
          entry.disposed = true
        },
      }
      mocks.streamCalls.push(entry)
      return { dispose: entry.dispose }
    },
  }),
}))

const { cmd, eventHandlers } = mocks

import { useTerminalBufferStore } from '../store'

// 票 12：事件源 = com.bedcode.terminal-session 插件 emit（事件名带插件命名空间）。
// 与 store 内常量保持字面同步（store 不导出这两个常量，此处按事件名字面锁）
const TERMINAL_STATE_EVENT = 'plugin:com.bedcode.terminal-session:terminal-state'
const TERMINAL_RESYNC_EVENT = 'plugin:com.bedcode.terminal-session:terminal-resync'

/** 取会话最近一次经 openTerminalStream 登记的页面流 */
function pageStream(sessionId: string) {
  const stream = mocks.streamCalls.filter((c) => c.sessionId === sessionId).pop()
  if (!stream) throw new Error(`no page stream for ${sessionId}`)
  return stream
}

/** 经段2 页面流投递一段裸字节（新协议：无帧头、无 per-frame offset） */
function emitRaw(sessionId: string, data: string) {
  pageStream(sessionId).onBytes(new TextEncoder().encode(data))
}

/** 模拟 Rust 推送链路状态事件 */
function emitState(sessionId: string, phase: string, detail?: string) {
  for (const fn of eventHandlers[TERMINAL_STATE_EVENT] ?? []) {
    fn({ session_id: sessionId, phase, detail })
  }
}

/** 模拟 Rust 推送重锚事件（ring_resync / 重订阅回包） */
function emitResync(sessionId: string, offset: number) {
  for (const fn of eventHandlers[TERMINAL_RESYNC_EVENT] ?? []) {
    fn({ session_id: sessionId, offset })
  }
}

async function flushAsync(n = 3) {
  for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0))
}

describe('terminalBuffer store（票 05：新协议 + 本地字节计数）', () => {
  let store: ReturnType<typeof useTerminalBufferStore>

  beforeEach(async () => {
    setActivePinia(createPinia())
    store = useTerminalBufferStore()
    vi.clearAllMocks()
    for (const key of Object.keys(eventHandlers)) delete eventHandlers[key]
    mocks.streamCalls.length = 0
    // 预注册事件监听：subscribeSession（任意路径触发）即 ensureEventListeners，
    // 挂载/测试中 emitState/emitRaw 依赖监听已就位（惰性注册是异步的）
    await store.subscribeSession('s1')
    cmd.terminalSubscribe.mockClear()
    await flushAsync()
  })

  describe('订阅触发（fresh subscribe 语义：已订阅也重播）', () => {
    it('subscribeSession 触发 terminalSubscribe；terminal-state 事件同步 subscribed', async () => {
      await store.subscribeSession('s1')
      expect(cmd.terminalSubscribe).toHaveBeenCalledWith('s1')

      // 订阅回包：收 subscribed → phase=live → subscribed=true
      emitState('s1', 'live', 'subscribed')
      const buffer = store.getBuffer('s1')!
      expect(buffer.phase).toBe('live')
      expect(buffer.subscribed).toBe(true)
      expect(buffer.subscribing).toBe(false)
    })

    it('已订阅会话再次 subscribeSession 仍触发（fresh subscribe 重播，不早退）', async () => {
      await store.subscribeSession('s1')
      expect(cmd.terminalSubscribe).toHaveBeenCalledTimes(1)
      emitState('s1', 'live', 'subscribed')

      // 页面挂载/刷新路径：信念已真也必须重新 invoke——Rust 侧据此发
      // subscribe 帧触发环窗口回放（历史由重播提供，无独立拼接）
      await store.subscribeSession('s1')
      expect(cmd.terminalSubscribe).toHaveBeenCalledTimes(2)
    })

    it('会话已停止：subscribeSession 早退（null），不触发建连', async () => {
      store.markSessionStopped('s1')
      const result = await store.subscribeSession('s1')
      expect(result).toBeNull()
      expect(cmd.terminalSubscribe).not.toHaveBeenCalled()
    })
  })

  describe('订阅信念收敛（Rust 幂等订阅不发事件 → 必须主动对账）', () => {
    it('链路已 live 而前端信念为假：对账后置真并放行输入', async () => {
      cmd.terminalGetState.mockResolvedValueOnce({
        sessionId: 's1',
        phase: 'live',
        cursor: 128,
        acked: 128,
        stopped: false,
        subscribed: true,
      })
      await store.reconcileState('s1')
      const buffer = store.getBuffer('s1')!
      expect(buffer.phase).toBe('live')
      expect(buffer.subscribed).toBe(true)
      expect(store.sendInput('s1', 'ls')).toBe(true)
      expect(cmd.terminalSendInput).toHaveBeenCalledWith('s1', 'ls', null)
    })

    it('对账只前进不回退：晚到的陈旧 idle 响应不覆盖已 live 的 phase', async () => {
      emitState('s1', 'live', 'subscribed')
      cmd.terminalGetState.mockResolvedValueOnce({
        sessionId: 's1',
        phase: 'idle',
        cursor: 0,
        acked: 0,
        stopped: false,
        subscribed: false,
      })
      await store.reconcileState('s1')
      expect(store.getBuffer('s1')!.phase).toBe('live')
      expect(store.getBuffer('s1')!.subscribed).toBe(true)
    })

    it('对账检出 stopped：标记会话停止并清订阅信念', async () => {
      cmd.terminalGetState.mockResolvedValueOnce({
        sessionId: 's1',
        phase: 'idle',
        cursor: 0,
        acked: 0,
        stopped: true,
        subscribed: false,
      })
      await store.reconcileState('s1')
      const buffer = store.getBuffer('s1')!
      expect(buffer.sessionStopped).toBe(true)
      expect(buffer.subscribed).toBe(false)
    })
  })

  describe('裸字节输出交付（本地计数 + 渲染）', () => {
    function mountHandler(onOutput: (d: Uint8Array) => void = () => {}) {
      store.registerRealtimeHandler('s1', { onOutput })
    }

    it('裸字节按到达序渲染：本地计数推进 + hasRenderedContent', () => {
      mountHandler()
      emitState('s1', 'live', 'subscribed')
      emitRaw('s1', 'hello')
      emitRaw('s1', ' world')
      const buffer = store.getBuffer('s1')!
      expect(buffer.lastRenderedOffset).toBe(11) // 5 + 6，本地字节计数
      expect(buffer.hasRenderedContent).toBe(true)
    })

    it('页面未挂载（无 handler）：字节丢弃计数（重进由重订阅回放补齐）', () => {
      mountHandler()
      store.unregisterRealtimeHandler('s1') // 页面离开：handler 清除 + 链路关闭
      emitRaw('s1', 'dropped')
      const buffer = store.getBuffer('s1')!
      expect(buffer.lastRenderedOffset).toBeNull()
      expect(cmd.terminalUnsubscribe).toHaveBeenCalledWith('s1')
    })

    // 票 15 行为变更：`terminal_output_activity` 活动通知链已随 host-terminal /
    // terminal-hooks 退役（零消费者），此处不再有对应契约
  })

  describe('resync 清屏重锚（唯一重锚信号）', () => {
    it('有在屏内容：清屏 + 本地计数归零 + 一次性提示', () => {
      const onClear = vi.fn()
      const onTruncated = vi.fn()
      store.registerRealtimeHandler('s1', { onOutput: () => {}, onClear, onTruncated })
      emitState('s1', 'live', 'subscribed')
      emitRaw('s1', 'old content')
      expect(store.getBuffer('s1')!.hasRenderedContent).toBe(true)

      emitResync('s1', 4096)
      const buffer = store.getBuffer('s1')!
      expect(buffer.lastRenderedOffset).toBeNull() // 基准重置
      expect(onClear).toHaveBeenCalledTimes(1) // 重锚必清屏
      expect(onTruncated).toHaveBeenCalledWith(4096)
      expect(buffer.hasRenderedContent).toBe(false)

      // 重锚后新流从 0 续计
      emitRaw('s1', 'new')
      expect(buffer.lastRenderedOffset).toBe(3)
    })

    it('无在屏内容（首次挂载空屏）：不清屏、不提示（清屏无意义）', () => {
      const onClear = vi.fn()
      const onTruncated = vi.fn()
      store.registerRealtimeHandler('s1', { onOutput: () => {}, onClear, onTruncated })
      emitResync('s1', 1024)
      expect(onClear).not.toHaveBeenCalled()
      expect(onTruncated).not.toHaveBeenCalled()
      expect(store.getBuffer('s1')!.subscribed).toBe(true) // 重锚后即视作已订阅（重播随流）
    })

    it('一次性提示：同会话二次 resync 不重复 toast', () => {
      const onClear = vi.fn()
      const onTruncated = vi.fn()
      store.registerRealtimeHandler('s1', { onOutput: () => {}, onClear, onTruncated })
      emitState('s1', 'live', 'subscribed')
      emitRaw('s1', 'x')
      emitResync('s1', 100)
      emitRaw('s1', 'y')
      emitResync('s1', 200)
      expect(onTruncated).toHaveBeenCalledTimes(1)
    })
  })

  describe('停止帧与恢复', () => {
    it('Rust 推 stopped 状态：phase=idle / subscribed=false / 计数归零', () => {
      store.registerRealtimeHandler('s1', { onOutput: () => {} })
      emitState('s1', 'live', 'subscribed')
      emitRaw('s1', 'tail bytes') // 尾帧先到并渲染（帧序保证）
      emitState('s1', 'idle', 'stopped')
      const buffer = store.getBuffer('s1')!
      expect(buffer.phase).toBe('idle')
      expect(buffer.subscribed).toBe(false)
      expect(buffer.sessionStopped).toBe(true)
      expect(buffer.lastRenderedOffset).toBeNull()
    })

    it('停止后到达的字节被丢弃，不渲染', () => {
      const onOutput = vi.fn()
      store.registerRealtimeHandler('s1', { onOutput })
      emitState('s1', 'idle', 'stopped')
      emitRaw('s1', 'after stop')
      expect(onOutput).not.toHaveBeenCalled()
    })

    it('markSessionRunning：仅复位停止信念，不订阅（后台不常拉；页面驱动订阅）', async () => {
      store.markSessionStopped('s1')
      store.markSessionRunning('s1')
      const buffer = store.getBuffer('s1')!
      expect(buffer.sessionStopped).toBe(false)
      expect(cmd.terminalSubscribe).not.toHaveBeenCalled()
    })
  })

  describe('生命周期（页面进出 = 关闭链路）', () => {
    it('markPageLeft：注销页面流（dispose）+ terminalUnsubscribe（关闭连接，不得后台常拉）', async () => {
      store.registerRealtimeHandler('s1', { onOutput: () => {} })
      await flushAsync()
      const stream = pageStream('s1')
      store.unregisterRealtimeHandler('s1')
      expect(stream.disposed).toBe(true)
      expect(cmd.terminalUnsubscribe).toHaveBeenCalledWith('s1')
    })

    it('markPageEntered：登记页面流；重复进入作废上一代流', async () => {
      store.markPageEntered('s1')
      await flushAsync()
      expect(mocks.streamCalls.length).toBe(1)
      const first = pageStream('s1')
      store.markPageEntered('s1')
      await flushAsync()
      expect(mocks.streamCalls.length).toBe(2)
      // 上一代已 dispose（代际作废）：在途帧不再写进已丢弃的 handler
      expect(first.disposed).toBe(true)
      first.onBytes(new Uint8Array(4))
      expect(store.getBuffer('s1')!.lastRenderedOffset).toBeNull()
    })

    it('markSessionStopped：terminalUnsubscribe + 游标重置', () => {
      store.markSessionStopped('s1')
      expect(cmd.terminalUnsubscribe).toHaveBeenCalledWith('s1')
      const buffer = store.getBuffer('s1')!
      expect(buffer.sessionStopped).toBe(true)
      expect(buffer.lastRenderedOffset).toBeNull()
    })

    it('markAllUnsubscribed：全量 terminalUnsubscribeAll（设备断开）', () => {
      store.markAllUnsubscribed()
      expect(cmd.terminalUnsubscribeAll).toHaveBeenCalled()
    })

    it('clearBuffer：terminalRemove（会话删除）', () => {
      store.clearBuffer('s1')
      expect(cmd.terminalRemove).toHaveBeenCalledWith('s1')
      expect(store.getBuffer('s1')).toBeUndefined()
    })
  })

  describe('输入与背压（前端 → Rust）', () => {
    it('已订阅会话 sendInput 经 terminalSendInput 发送（文本 + 特殊键双形态透传）', () => {
      store.registerRealtimeHandler('s1', { onOutput: () => {} })
      emitState('s1', 'live', 'subscribed')
      expect(store.sendInput('s1', 'ls -la')).toBe(true)
      expect(cmd.terminalSendInput).toHaveBeenCalledWith('s1', 'ls -la', null)
      expect(store.sendInput('s1', '', 'ctrl_c')).toBe(true)
      expect(cmd.terminalSendInput).toHaveBeenCalledWith('s1', '', 'ctrl_c')
    })

    it('未订阅会话 sendInput 拒绝', () => {
      expect(store.sendInput('s1', 'ls')).toBe(false)
      expect(cmd.terminalSendInput).not.toHaveBeenCalled()
    })

    it('ackRendered：按本地渲染字节计数推进 Rust 水位', () => {
      store.registerRealtimeHandler('s1', { onOutput: () => {} })
      emitState('s1', 'live', 'subscribed')
      emitRaw('s1', 'abc')
      store.ackRendered('s1')
      expect(cmd.terminalAckRendered).toHaveBeenCalledWith('s1', 3)
    })
  })

  describe('重锚后中断不产生缺口误报（缺口号不再误报）', () => {
    it('连续帧区间乱序到达也按序渲染（无 offset 判定，无 gap/resplice 路径）', () => {
      const onOutput = vi.fn()
      store.registerRealtimeHandler('s1', { onOutput })
      emitState('s1', 'live', 'subscribed')
      // 新协议无 per-frame offset：任何字节块到达即渲染，不存在「帧首越过游标」
      // 的缺口判定；字节连续性由 WS 帧序保证
      emitRaw('s1', 'part1')
      emitRaw('s1', 'part2')
      expect(onOutput).toHaveBeenCalledTimes(2)
      expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(10)
    })
  })
})