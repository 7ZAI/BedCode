/**
 * terminalBuffer store 单元测试（Rust 后端持有终端 WS 后的重写）
 *
 * 覆盖：terminalSubscribe 触发、terminal-state 事件同步 phase/subscribed、
 * 历史拼接（terminalGetHistory → 写完历史才消费实时帧）、跨帧裁剪、
 * offset 缺口重拼接、截断（清屏 + 锚定）、生命周期（停止/恢复/删除）、
 * 输入、渲染背压 ack、双速模式切换。
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

// mock Rust 命令面（terminal_* 全部可观测）
const cmd = vi.hoisted(() => ({
  terminalSubscribe: vi.fn(async () => {}),
  terminalUnsubscribe: vi.fn(async () => {}),
  terminalUnsubscribeAll: vi.fn(async () => {}),
  terminalRemove: vi.fn(async () => {}),
  terminalSendInput: vi.fn(async () => {}),
  terminalAckRendered: vi.fn(async () => {}),
  terminalSetMode: vi.fn(async () => {}),
  terminalGetHistory: vi.fn(async (_s: string, _f: number) => ({
    from: 0,
    minOffset: 0,
    snapshotOffset: 0,
    historyBytes: 0,
    dataBase64: '',
  })),
}))
vi.mock('@/composables/useMobileCommands', () => cmd)

import { useTerminalBufferStore } from '@/stores/terminalBuffer'

/** base64 编码辅助（构造事件载荷） */
function b64(text: string): string {
  return btoa(unescape(encodeURIComponent(text))).replace(/\+/g, '-').replace(/\//g, '_')
}

/** 模拟 Rust 推送实时帧事件（Tauri 事件形状 { payload }） */
function emitFrame(sessionId: string, start: number, end: number, data: string) {
  eventHandlers['terminal-frame']!({
    payload: {
      session_id: sessionId,
      start_offset: start,
      end_offset: end,
      data_base64: b64(data),
    },
  })
}

/** 模拟 Rust 推送链路状态事件 */
function emitState(sessionId: string, phase: string, detail?: string) {
  eventHandlers['terminal-state']!({
    payload: { session_id: sessionId, phase, detail },
  })
}

async function flushAsync(n = 3) {
  for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0))
}

describe('terminalBuffer store（Rust 驱动）', () => {
  let store: ReturnType<typeof useTerminalBufferStore>

  beforeEach(() => {
    setActivePinia(createPinia())
    store = useTerminalBufferStore()
    vi.clearAllMocks()
    eventHandlers['terminal-frame'] = null
    eventHandlers['terminal-state'] = null
    listenMock.mockImplementation(async (name: string, cb: (p: unknown) => void) => {
      eventHandlers[name] = cb
      return () => {}
    })
    cmd.terminalGetHistory.mockImplementation(async (_s: string, _f: number) => ({
      from: 0,
      minOffset: 0,
      snapshotOffset: 0,
      historyBytes: 0,
      dataBase64: '',
    }))
  })

  describe('订阅触发（会话启动即订阅，前端只触发）', () => {
    it('subscribeSession 触发 terminalSubscribe；terminal-state 事件同步 subscribed', async () => {
      const p = store.subscribeSession('s1')
      await flushAsync()
      expect(cmd.terminalSubscribe).toHaveBeenCalledWith('s1')
      expect(store.getBuffer('s1')!.subscribing).toBe(true)
      expect(store.getBuffer('s1')!.subscribed).toBe(false)

      // Rust 链路事件：连接 → 已订阅
      emitState('s1', 'connecting')
      emitState('s1', 'history')
      await flushAsync()
      expect(store.getBuffer('s1')!.subscribed).toBe(true)
      expect(store.getBuffer('s1')!.phase).toBe('history')
      await p
    })

    it('已订阅会话重复订阅：直接返回快照、不重复建连', async () => {
      await store.subscribeSession('s1')
      emitState('s1', 'history')
      await store.subscribeSession('s1')
      cmd.terminalSubscribe.mockClear()
      const result = await store.subscribeSession('s1')
      expect(result).toEqual({ snapshotOffset: 0, minOffset: 0, historyBytes: 0 })
      expect(cmd.terminalSubscribe).not.toHaveBeenCalled()
      expect(store.getBuffer('s1')!.subscribed).toBe(true)
    })
  })

  describe('历史拼接（terminalGetHistory → 拼完才消费实时帧）', () => {
    it('完整流：历史段写入后 FLUSH 拼接期缓冲的实时帧，无重复', async () => {
      const outputs: string[] = []
      // 历史 [0,3)="abc" 在注册时拼接
      cmd.terminalGetHistory.mockResolvedValueOnce({
        from: 0,
        minOffset: 0,
        snapshotOffset: 3,
        historyBytes: 3,
        dataBase64: b64('abc'),
      })
      store.registerRealtimeHandler('s1', {
        onOutput: (d: Uint8Array) => outputs.push(new TextDecoder().decode(d)),
      })

      // 历史拼接完成前到达的实时帧：缓冲不写入（拼完才消费）
      emitFrame('s1', 3, 6, 'def')
      emitFrame('s1', 6, 9, 'ghi')
      expect(outputs).toEqual([])
      expect(store.getBuffer('s1')!.historyPreparing).toBe(true)

      // 拼接完成：写入历史 + 游标推进 + FLUSH 缓冲帧（按序、无重复）
      await flushAsync()
      expect(cmd.terminalGetHistory).toHaveBeenCalledWith('s1', 0)
      expect(cmd.terminalSetMode).toHaveBeenCalledWith('s1', 'realtime')
      expect(outputs).toEqual(['abc', 'def', 'ghi'])
      expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(9)
      expect(store.getBuffer('s1')!.historyPreparing).toBe(false)
    })

    it('重拼接从游标续补（Rust 端按 from 切片）；实时帧跨游标时裁剪前半段', async () => {
      const outputs: string[] = []
      // 历史 [0,6)="abcdef" 在注册时拼接
      cmd.terminalGetHistory.mockResolvedValueOnce({
        from: 0,
        minOffset: 0,
        snapshotOffset: 6,
        historyBytes: 6,
        dataBase64: b64('abcdef'),
      })
      store.registerRealtimeHandler('s1', {
        onOutput: (d: Uint8Array) => outputs.push(new TextDecoder().decode(d)),
      })
      await flushAsync()
      expect(outputs).toEqual(['abcdef'])
      expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(6)

      // 重拼接（forceReplay）：from=游标 6，Rust 返回 [6,10)="ghij"——不重叠
      cmd.terminalGetHistory.mockResolvedValueOnce({
        from: 6,
        minOffset: 6,
        snapshotOffset: 10,
        historyBytes: 4,
        dataBase64: b64('ghij'),
      })
      store.forceReplay('s1')
      await flushAsync()
      expect(cmd.terminalGetHistory).toHaveBeenCalledWith('s1', 6)
      expect(outputs).toEqual(['abcdef', 'ghij'])
      expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(10)

      // 实时帧跨游标（服务端重发 [8,14)，游标 10）→ 裁掉前半段 [8,10)，零重复
      emitFrame('s1', 8, 14, 'ijklmn')
      expect(outputs).toEqual(['abcdef', 'ghij', 'klmn'])
      expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(14)
    })

    it('offset 缺口：帧首越过游标 → 重拼接补回（带冷却）', async () => {
      const outputs: string[] = []
      cmd.terminalGetHistory.mockResolvedValueOnce({
        from: 0,
        minOffset: 0,
        snapshotOffset: 3,
        historyBytes: 3,
        dataBase64: b64('abc'),
      })
      store.registerRealtimeHandler('s1', {
        onOutput: (d: Uint8Array) => outputs.push(new TextDecoder().decode(d)),
      })
      await flushAsync()
      expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(3)

      // 缺口：字节 3-5 缺失（缓存被淘汰），帧从 6 起 → 从游标 3 重拼接
      emitFrame('s1', 6, 9, 'ghi')
      await flushAsync()
      expect(cmd.terminalGetHistory).toHaveBeenCalledWith('s1', 3)
      // 冷却期内再次缺口：不重复重拼接
      emitFrame('s1', 9, 12, 'jkl')
      await flushAsync()
      expect(cmd.terminalGetHistory).toHaveBeenCalledTimes(2)
    })

    it('截断：minOffset > 游标 → 清屏 + onTruncated 一次 + 锚定重播', async () => {
      const outputs: string[] = []
      const onClear = vi.fn()
      const onTruncated = vi.fn()
      cmd.terminalGetHistory.mockResolvedValueOnce({
        from: 0,
        minOffset: 0,
        snapshotOffset: 3,
        historyBytes: 3,
        dataBase64: b64('abc'),
      })
      store.registerRealtimeHandler('s1', {
        onOutput: (d: Uint8Array) => outputs.push(new TextDecoder().decode(d)),
        onClear,
        onTruncated,
      })
      await flushAsync()
      expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(3)

      // 重拼接：Rust 驻留头部已推进到 20（游标 3 已不可恢复）→ 清屏 + 提示
      cmd.terminalGetHistory.mockResolvedValueOnce({
        from: 3,
        minOffset: 20,
        snapshotOffset: 30,
        historyBytes: 10,
        dataBase64: b64('tuvwxyzxyz'),
      })
      store.forceReplay('s1')
      await flushAsync()
      expect(onClear).toHaveBeenCalledTimes(1)
      expect(onTruncated).toHaveBeenCalledWith(20)
      // 锚定重播正常写入
      expect(outputs).toEqual(['abc', 'tuvwxyzxyz'])
      expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(30)
    })
  })

  describe('生命周期（订阅由 Rust 管理，前端触发取消）', () => {
    it('markSessionStopped：terminalUnsubscribe + 游标重置；恢复运行重新订阅', async () => {
      await store.subscribeSession('s1')
      emitState('s1', 'live')
      store.markSessionStopped('s1')
      await flushAsync()
      expect(cmd.terminalUnsubscribe).toHaveBeenCalledWith('s1')
      expect(store.getBuffer('s1')!.sessionStopped).toBe(true)
      expect(store.getBuffer('s1')!.subscribed).toBe(false)

      store.markSessionRunning('s1')
      await flushAsync()
      expect(cmd.terminalSubscribe).toHaveBeenCalledWith('s1')
      expect(store.getBuffer('s1')!.sessionStopped).toBe(false)
    })

    it('Rust 推 stopped 状态：同步会话停止', async () => {
      await store.subscribeSession('s1')
      emitState('s1', 'live')
      emitState('s1', 'idle', 'stopped')
      await flushAsync()
      expect(store.getBuffer('s1')!.sessionStopped).toBe(true)
      expect(store.getBuffer('s1')!.subscribed).toBe(false)
    })

    it('markAllUnsubscribed：全量 terminalUnsubscribeAll（设备断开）', async () => {
      store.markAllUnsubscribed()
      await flushAsync()
      expect(cmd.terminalUnsubscribeAll).toHaveBeenCalled()
    })

    it('clearBuffer：terminalRemove（会话删除）', async () => {
      await store.subscribeSession('s1')
      store.clearBuffer('s1')
      await flushAsync()
      expect(cmd.terminalRemove).toHaveBeenCalledWith('s1')
      expect(store.buffers.has('s1')).toBe(false)
    })
  })

  describe('输入与背压（前端 → Rust）', () => {
    it('已订阅会话 sendInput 经 terminalSendInput 发送', async () => {
      await store.subscribeSession('s1')
      emitState('s1', 'live')
      const ok = store.sendInput('s1', 'ls -la', 'enter')
      expect(ok).toBe(true)
      await flushAsync()
      expect(cmd.terminalSendInput).toHaveBeenCalledWith('s1', 'ls -la', 'enter')
    })

    it('未订阅会话 sendInput 拒绝', async () => {
      const ok = store.sendInput('s1', 'ls')
      expect(ok).toBe(false)
      expect(cmd.terminalSendInput).not.toHaveBeenCalled()
    })

    it('ackRendered：按 lastRenderedOffset 推进 Rust 水位', async () => {
      const outputs: string[] = []
      cmd.terminalGetHistory.mockResolvedValueOnce({
        from: 0,
        minOffset: 0,
        snapshotOffset: 3,
        historyBytes: 3,
        dataBase64: b64('abc'),
      })
      store.registerRealtimeHandler('s1', {
        onOutput: (d: Uint8Array) => outputs.push(new TextDecoder().decode(d)),
      })
      await flushAsync()
      store.ackRendered('s1')
      await flushAsync()
      expect(cmd.terminalAckRendered).toHaveBeenCalledWith('s1', 3)
    })
  })

  describe('双速传播（页面进出）', () => {
    it('注册 → realtime（进终端页读即传）；注销 → batch（退出页面满批才转发）', async () => {
      store.registerRealtimeHandler('s1', { onOutput: vi.fn() })
      await flushAsync()
      expect(cmd.terminalSetMode).toHaveBeenCalledWith('s1', 'realtime')

      store.unregisterRealtimeHandler('s1')
      await flushAsync()
      expect(cmd.terminalSetMode).toHaveBeenCalledWith('s1', 'batch')
    })
  })

  describe('输出活动通知', () => {
    it('实时帧触发 terminal_output_activity（节流）', async () => {
      const outputs: string[] = []
      store.registerRealtimeHandler('s1', {
        onOutput: (d: Uint8Array) => outputs.push(new TextDecoder().decode(d)),
      })
      await flushAsync()
      cmd.terminalGetHistory.mockResolvedValueOnce({
        from: 0,
        minOffset: 0,
        snapshotOffset: 0,
        historyBytes: 0,
        dataBase64: '',
      })
      await flushAsync()
      emitFrame('s1', 0, 2, 'ab')
      expect(emitMock).toHaveBeenCalledWith('terminal_output_activity', { session_id: 's1' })
    })
  })
})