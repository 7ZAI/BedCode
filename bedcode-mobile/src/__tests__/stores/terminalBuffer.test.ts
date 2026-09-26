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

// mock Tauri 事件：捕获 listen 回调（按事件名），emit 记录
const emitMock = vi.fn().mockResolvedValue(undefined)
const listenMock = vi.fn()
const eventHandlers: Record<string, ((payload: unknown) => void) | null> = {}
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => listenMock(...args),
  emit: (...args: unknown[]) => emitMock(...args),
}))

// mock Tauri Channel：实例由 store 在 markPageEntered 创建并交给被 mock 的
// terminalPageSubscribe（可经 mock.calls 取回），测试据此注入裸字节
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => {}),
  Channel: class ChannelMock<T> {
    onmessage: ((message: T) => void) | null = null
  },
}))

// mock Rust 命令面（terminal_* 全部可观测）
const cmd = vi.hoisted(() => ({
  terminalSubscribe: vi.fn(async () => {}),
  terminalUnsubscribe: vi.fn(async () => {}),
  terminalUnsubscribeAll: vi.fn(async () => {}),
  terminalRemove: vi.fn(async () => {}),
  terminalSendInput: vi.fn(async () => {}),
  terminalAckRendered: vi.fn(async () => {}),
  terminalPageSubscribe: vi.fn(async () => {}),
  terminalPageUnsubscribe: vi.fn(async () => {}),
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

import { useTerminalBufferStore } from '@/stores/terminalBuffer'

/** 取会话最近一次经 terminal_page_subscribe 交给 Rust 的页面通道 */
function pageChannel(sessionId: string) {
  const call = cmd.terminalPageSubscribe.mock.calls
    .filter((c) => c[0] === sessionId)
    .pop()
  const channel = call?.[1] as { onmessage: ((m: ArrayBuffer) => void) | null } | undefined
  if (!channel?.onmessage) throw new Error(`no page channel for ${sessionId}`)
  return channel
}

/** 经段2 Channel 投递一段裸字节（新协议：无帧头、无 per-frame offset） */
function emitRaw(sessionId: string, data: string) {
  const bytes = new TextEncoder().encode(data)
  const buf = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer
  pageChannel(sessionId).onmessage!(buf)
}

/** 模拟 Rust 推送链路状态事件 */
function emitState(sessionId: string, phase: string, detail?: string) {
  eventHandlers['terminal-state']!({
    payload: { session_id: sessionId, phase, detail },
  })
}

/** 模拟 Rust 推送重锚事件（ring_resync / 重订阅回包） */
function emitResync(sessionId: string, offset: number) {
  eventHandlers['terminal-resync']!({
    payload: { session_id: sessionId, offset },
  })
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
    eventHandlers['terminal-state'] = null
    eventHandlers['terminal-resync'] = null
    listenMock.mockImplementation(async (name: string, cb: (p: unknown) => void) => {
      eventHandlers[name] = cb
      return () => {}
    })
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

    it('输出活动通知：实时字节触发 terminal_output_activity（节流）', () => {
      mountHandler()
      emitState('s1', 'live', 'subscribed')
      emitRaw('s1', 'a')
      expect(emitMock).toHaveBeenCalledWith('terminal_output_activity', { session_id: 's1' })
      emitMock.mockClear()
      // 节流窗口内再次到达不重复通知
      emitRaw('s1', 'b')
      expect(emitMock).not.toHaveBeenCalled()
    })
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
    it('markPageLeft：page_unsubscribe + terminalUnsubscribe（关闭连接，不得后台常拉）', () => {
      store.registerRealtimeHandler('s1', { onOutput: () => {} })
      store.unregisterRealtimeHandler('s1')
      expect(cmd.terminalPageUnsubscribe).toHaveBeenCalledWith('s1')
      expect(cmd.terminalUnsubscribe).toHaveBeenCalledWith('s1')
    })

    it('markPageEntered：page_subscribe 登记通道；重复进入作废上一代通道', () => {
      store.markPageEntered('s1')
      expect(cmd.terminalPageSubscribe).toHaveBeenCalledTimes(1)
      const first = pageChannel('s1')
      store.markPageEntered('s1')
      expect(cmd.terminalPageSubscribe).toHaveBeenCalledTimes(2)
      // 上一代通道 onmessage 已被替换为空操作：在途帧不写进已丢弃的 handler
      expect(first.onmessage).not.toBeNull()
      first.onmessage?.(new ArrayBuffer(4))
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