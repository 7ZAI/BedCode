/**
 * terminalBuffer store 单元测试（10 号票重写）
 *
 * 覆盖：终端 WS 状态机（HISTORY 拼接 → history_end FLUSH → LIVE）、
 * 快照重播去重（跳过 ≤ lastRenderedSeq）、seq 缺口重订阅、
 * 环形淘汰截断（清屏 + 锚定）、重连恢复、历史缓存回放、输入帧发送。
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

// mock Tauri 事件（terminal_output_activity 通知）
const emitMock = vi.fn().mockResolvedValue(undefined)
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
  emit: (...args: unknown[]) => emitMock(...args),
}))

// mock 终端 socket：测试持有 handlers 引用，手动驱动状态机
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

import { useTerminalBufferStore } from '@/stores/terminalBuffer'

/** 构造 TB v2 帧对象（store 消费的解析结果） */
function frame(seq: number, eventCount = 1, data = `data-${seq}`) {
  return {
    data: new TextEncoder().encode(data),
    seq,
    eventCount,
    lastSeq: seq + eventCount - 1,
    isWaiting: false,
  }
}

/** 新建 fake socket 并捕获 handlers；返回 [socket, handlers] */
function setupSocket() {
  fakeSocket = {
    start: vi.fn(),
    subscribe: vi.fn(),
    sendInput: vi.fn(),
    stop: vi.fn(),
    reconnect: vi.fn(),
    isOpen: vi.fn(() => false),
  }
  capturedHandlers = null
  createTerminalSocketMock.mockImplementation((handlers: unknown) => {
    capturedHandlers = handlers as Record<string, any>
    return fakeSocket
  })
  return fakeSocket
}

async function flushAsync() {
  await new Promise((r) => setTimeout(r, 0))
  await new Promise((r) => setTimeout(r, 0))
}

describe('terminalBuffer store', () => {
  let store: ReturnType<typeof useTerminalBufferStore>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useTerminalBufferStore()
    emitMock.mockClear()
    setupSocket()
  })

  describe('订阅状态机', () => {
    it('完整流：subscribe_ok → HISTORY 写历史 → history_end → LIVE 直写实时', async () => {
      const outputs: string[] = []
      store.registerRealtimeHandler('s1', { onOutput: (d: Uint8Array) => outputs.push(new TextDecoder().decode(d)) })
      await store.subscribeSession('s1')

      // 连接建立：start + subscribe 挂起（auth_ok 后由 socket 内部发送）
      expect(fakeSocket.start).toHaveBeenCalledWith('s1')
      expect(fakeSocket.subscribe).toHaveBeenCalled()

      // subscribe_ok：进入 HISTORY
      capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })

      // 历史帧（lastSeq ≤ snapshotSeq）：写入 + 入缓存
      capturedHandlers!.onFrame(frame(1))
      capturedHandlers!.onFrame(frame(2))
      expect(outputs).toEqual(['data-1', 'data-2'])
      expect(store.getBuffer('s1')!.lastRenderedSeq).toBe(2)

      // history_end → LIVE
      capturedHandlers!.onHistoryEnd(10)
      capturedHandlers!.onFrame(frame(3))
      expect(outputs).toEqual(['data-1', 'data-2', 'data-3'])
      expect(store.getBuffer('s1')!.phase).toBe('live')
    })

    it('实时帧先于 history_end 到达：缓冲后统一 FLUSH（按序写入）', async () => {
      const outputs: string[] = []
      store.registerRealtimeHandler('s1', { onOutput: (d: Uint8Array) => outputs.push(new TextDecoder().decode(d)) })
      await store.subscribeSession('s1')
      capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })

      // 历史段内实时帧（> snapshotSeq）：入实时缓冲，不写入
      capturedHandlers!.onFrame(frame(12))
      capturedHandlers!.onFrame(frame(13))
      expect(outputs).toEqual([])

      // history_end：FLUSH 缓冲帧
      capturedHandlers!.onHistoryEnd(10)
      expect(outputs).toEqual(['data-12', 'data-13'])
      expect(store.getBuffer('s1')!.lastRenderedSeq).toBe(13)
    })

    it('已订阅会话重复订阅：直接返回快照元数据，不重建连接', async () => {
      await store.subscribeSession('s1')
      capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
      capturedHandlers!.onHistoryEnd(10)

      fakeSocket.start.mockClear()
      const result = await store.subscribeSession('s1')
      expect(result).toEqual({ snapshotSeq: 10, minSeq: 0, historyCount: 0 })
      expect(fakeSocket.start).not.toHaveBeenCalled()
      expect(store.getBuffer('s1')!.subscribed).toBe(true)
    })
  })

  describe('快照重播去重与缺口', () => {
    it('重播帧跳过 ≤ lastRenderedSeq，不双写', async () => {
      const outputs: string[] = []
      store.registerRealtimeHandler('s1', { onOutput: (d: Uint8Array) => outputs.push(new TextDecoder().decode(d)) })
      await store.subscribeSession('s1')
      capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
      capturedHandlers!.onFrame(frame(1, 3)) // 1-3
      capturedHandlers!.onFrame(frame(4, 3)) // 4-6
      capturedHandlers!.onHistoryEnd(10)

      // 快照重播（重连后）：帧与已渲染区重叠 → 整帧跳过
      capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
      capturedHandlers!.onFrame(frame(1, 3))
      capturedHandlers!.onFrame(frame(4, 3))
      expect(outputs).toEqual(['data-1', 'data-4'])

      // 未渲染部分正常写入
      capturedHandlers!.onFrame(frame(7))
      expect(outputs).toEqual(['data-1', 'data-4', 'data-7'])
    })

    it('seq 缺口（> lastRenderedSeq+1）：重发订阅，帧不写入', async () => {
      const outputs: string[] = []
      store.registerRealtimeHandler('s1', { onOutput: (d: Uint8Array) => outputs.push(new TextDecoder().decode(d)) })
      await store.subscribeSession('s1')
      capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
      capturedHandlers!.onFrame(frame(1))
      capturedHandlers!.onHistoryEnd(10)

      fakeSocket.subscribe.mockClear()
      capturedHandlers!.onFrame(frame(5)) // 缺口：2-4 缺失
      expect(fakeSocket.subscribe).toHaveBeenCalled()
      expect(outputs).toEqual(['data-1'])
    })

    it('环形淘汰截断：清屏 + onTruncated 一次 + 锚定重播', async () => {
      const outputs: string[] = []
      const onClear = vi.fn()
      const onTruncated = vi.fn()
      store.registerRealtimeHandler('s1', { onOutput: (d: Uint8Array) => outputs.push(new TextDecoder().decode(d)), onClear, onTruncated })
      await store.subscribeSession('s1')
      capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
      capturedHandlers!.onFrame(frame(1, 3))
      capturedHandlers!.onHistoryEnd(10)

      // 重订阅：min_seq 已越过游标（已渲染区被淘汰）
      capturedHandlers!.onSubscribed({ snapshotSeq: 30, minSeq: 20, historyCount: 5 })
      expect(onClear).toHaveBeenCalledTimes(1)
      expect(onTruncated).toHaveBeenCalledWith(20)

      // 锚定后重播帧（seq 20 起）正常写入，不再触发截断提示
      capturedHandlers!.onFrame(frame(20))
      capturedHandlers!.onFrame(frame(21))
      expect(outputs).toEqual(['data-1', 'data-20', 'data-21'])
      expect(onTruncated).toHaveBeenCalledTimes(1)
    })
  })

  describe('重连与生命周期', () => {
    it('onClose 后重连：重新 subscribe_ok → 快照重播按 lastRenderedSeq 跳过', async () => {
      const outputs: string[] = []
      store.registerRealtimeHandler('s1', { onOutput: (d: Uint8Array) => outputs.push(new TextDecoder().decode(d)) })
      await store.subscribeSession('s1')
      capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
      capturedHandlers!.onFrame(frame(1))
      capturedHandlers!.onHistoryEnd(10)

      capturedHandlers!.onClose()
      expect(store.getBuffer('s1')!.subscribing).toBe(false)
      expect(store.getBuffer('s1')!.lastRenderedSeq).toBe(1) // 游标保留

      // 重连后快照重播：重叠帧跳过，新帧写入
      capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
      capturedHandlers!.onFrame(frame(1))
      capturedHandlers!.onFrame(frame(2))
      capturedHandlers!.onHistoryEnd(10)
      expect(outputs).toEqual(['data-1', 'data-2'])
    })

    it('session_stopped：停 socket、置 idle；恢复运行后重新订阅', async () => {
      await store.subscribeSession('s1')
      capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })

      capturedHandlers!.onSessionStopped('s1')
      expect(fakeSocket.stop).toHaveBeenCalled()
      expect(store.getBuffer('s1')!.sessionStopped).toBe(true)
      expect(store.getBuffer('s1')!.subscribed).toBe(false)

      store.markSessionRunning('s1')
      expect(store.getBuffer('s1')!.sessionStopped).toBe(false)
    })

    it('SESSION_NOT_FOUND 错误：有限重试后停止；其他错误走 reconnect', async () => {
      await store.subscribeSession('s1')
      capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })

      capturedHandlers!.onError('SESSION_NOT_FOUND', 'not found')
      capturedHandlers!.onError('SESSION_NOT_FOUND', 'not found')
      expect(fakeSocket.reconnect).toHaveBeenCalledTimes(2)
      capturedHandlers!.onError('SESSION_NOT_FOUND', 'not found')
      expect(fakeSocket.stop).toHaveBeenCalled()
      expect(store.getBuffer('s1')!.subscribed).toBe(false)
    })

    it('markSessionStopped：重置游标 + 停 socket（会话重启后 seq 空间重建）', async () => {
      await store.subscribeSession('s1')
      capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
      capturedHandlers!.onFrame(frame(1))

      store.markSessionStopped('s1')
      expect(fakeSocket.stop).toHaveBeenCalled()
      expect(store.getBuffer('s1')!.lastRenderedSeq).toBeNull()
      expect(store.getBuffer('s1')!.sessionStopped).toBe(true)
    })
  })

  describe('历史缓存与页面重进', () => {
    it('无 handler 时帧入缓存；registerRealtimeHandler 回放全部并推进游标', async () => {
      // 预加载：无 handler 订阅（会话页 prepareSession）
      await store.subscribeSession('s1')
      capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
      capturedHandlers!.onFrame(frame(1))
      capturedHandlers!.onFrame(frame(2, 2)) // 2-3
      capturedHandlers!.onHistoryEnd(10)
      expect(store.getBuffer('s1')!.lastRenderedSeq).toBe(3) // 无 handler 也推进游标
      expect(store.getBuffer('s1')!.historyCache).toHaveLength(2)

      // 终端页挂载：回放缓存（lastRenderedSeq 为 null 的场景 = 页面重进 forceReplay 后）
      store.forceReplay('s1')
      const outputs: string[] = []
      store.registerRealtimeHandler('s1', { onOutput: (d: Uint8Array) => outputs.push(new TextDecoder().decode(d)) })
      expect(outputs).toEqual(['data-1', 'data-2'])
      expect(store.getBuffer('s1')!.lastRenderedSeq).toBe(3)

      // 后续 live 帧（seq > 3）正常写入，不双写
      capturedHandlers!.onFrame(frame(4))
      expect(outputs).toEqual(['data-1', 'data-2', 'data-4'])
    })

    it('历史缓存 LRU：超出 16MB 淘汰最旧帧', async () => {
      await store.subscribeSession('s1')
      capturedHandlers!.onSubscribed({ snapshotSeq: 100, minSeq: 0, historyCount: 5 })
      // 每帧 1MB：写入 20 帧（超 16MB 上限）
      const bigFrame = (seq: number) => ({
        data: new Uint8Array(1024 * 1024),
        seq,
        eventCount: 1,
        lastSeq: seq,
        isWaiting: false,
      })
      for (let i = 1; i <= 20; i++) {
        capturedHandlers!.onFrame(bigFrame(i))
      }
      const buffer = store.getBuffer('s1')!
      expect(buffer.historyBytes).toBeLessThanOrEqual(16 * 1024 * 1024)
      // 最旧帧被淘汰，最新帧保留
      const firstSeq = buffer.historyCache[0].seq
      expect(firstSeq).toBeGreaterThan(1)
      expect(buffer.historyCache[buffer.historyCache.length - 1].seq).toBe(20)
    })
  })

  describe('输入发送', () => {
    it('已订阅会话 sendInput 经 socket 发送', async () => {
      await store.subscribeSession('s1')
      capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
      capturedHandlers!.onHistoryEnd(10)
      fakeSocket.isOpen.mockReturnValue(true)

      const ok = store.sendInput('s1', 'ls -la', 'enter')
      expect(ok).toBe(true)
      expect(fakeSocket.sendInput).toHaveBeenCalledWith('ls -la', 'enter')
    })

    it('未订阅会话 sendInput 拒绝', async () => {
      const ok = store.sendInput('s1', 'ls')
      expect(ok).toBe(false)
      expect(fakeSocket.sendInput).not.toHaveBeenCalled()
    })
  })

  describe('输出活动通知', () => {
    it('每帧触发 terminal_output_activity（节流窗口内合并）', async () => {
      await store.subscribeSession('s1')
      capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
      capturedHandlers!.onFrame(frame(1))
      capturedHandlers!.onFrame(frame(2))
      expect(emitMock).toHaveBeenCalledWith('terminal_output_activity', { session_id: 's1' })
      expect(emitMock.mock.calls.length).toBeGreaterThanOrEqual(1)
    })
  })
})
