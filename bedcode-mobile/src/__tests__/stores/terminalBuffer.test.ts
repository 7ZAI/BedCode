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
  terminalPageSubscribe: vi.fn(async () => {}),
  terminalPageUnsubscribe: vi.fn(async () => {}),
  terminalGetHistory: vi.fn(async (_s: string, _f: number) => ({
    from: 0,
    minOffset: 0,
    snapshotOffset: 0,
    historyBytes: 0,
    dataBase64: '',
  })),
  terminalGetState: vi.fn(async (sessionId: string) => ({
    sessionId,
    phase: 'idle',
    cursor: 0,
    snapshotOffset: 0,
    minOffset: 0,
    acked: 0,
    mode: 'realtime',
    stopped: false,
    historyBytes: 0,
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

  beforeEach(async () => {
    setActivePinia(createPinia())
    store = useTerminalBufferStore()
    vi.clearAllMocks()
    eventHandlers['terminal-frame'] = null
    eventHandlers['terminal-state'] = null
    eventHandlers['terminal-resync'] = null
    listenMock.mockImplementation(async (name: string, cb: (p: unknown) => void) => {
      eventHandlers[name] = cb
      return () => {}
    })
    // 预注册事件监听：真实链路中 subscribeSession（会话启动）即 ensureEventListeners，
    // 挂载/测试中 emitState/emitFrame 依赖监听已就位（惰性注册是异步的）
    store.subscribeSession('s1').catch(() => {})
    cmd.terminalSubscribe.mockClear()
    await flushAsync()
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

  describe('订阅信念收敛（Rust 幂等订阅不发事件 → 必须主动对账）', () => {
    it('链路已 live 而前端信念为假：对账后置真并放行输入', async () => {
      // Rust 侧链路已在运行：终端的 terminal_subscribe 幂等静默返回，无任何事件
      cmd.terminalGetState.mockResolvedValueOnce({
        sessionId: 's2',
        phase: 'live',
        cursor: 120,
        snapshotOffset: 120,
        minOffset: 0,
        acked: 120,
        mode: 'realtime',
        stopped: false,
        historyBytes: 120,
      })

      await store.subscribeSession('s2')

      expect(cmd.terminalSubscribe).toHaveBeenCalledWith('s2')
      const buffer = store.getBuffer('s2')!
      expect(buffer.phase).toBe('live')
      expect(buffer.subscribed).toBe(true)
      expect(buffer.subscribing).toBe(false)
      // 输入门控放行（修复前永久 false → 输入无反应）
      expect(store.sendInput('s2', 'ls')).toBe(true)
      expect(cmd.terminalSendInput).toHaveBeenCalledWith('s2', 'ls', null)
    })

    it('Rust 侧仍未订阅（idle）：对账不得把信念置真、输入被拒', async () => {
      await store.subscribeSession('s3')

      const buffer = store.getBuffer('s3')!
      expect(buffer.subscribed).toBe(false)
      expect(store.sendInput('s3', 'ls')).toBe(false)
      expect(cmd.terminalSendInput).not.toHaveBeenCalledWith('s3', 'ls', null)
    })

    it('对账只前进不回退：晚到的陈旧 idle 响应不覆盖已 live 的 phase', async () => {
      await store.subscribeSession('s4')
      emitState('s4', 'live')
      expect(store.getBuffer('s4')!.phase).toBe('live')

      // 命令响应晚于状态事件到达（陈旧快照）
      await store.reconcileState('s4')

      expect(store.getBuffer('s4')!.phase).toBe('live')
      expect(store.getBuffer('s4')!.subscribed).toBe(true)
    })

    it('对账检出 stopped：标记会话停止并清订阅信念', async () => {
      await store.subscribeSession('s5')
      emitState('s5', 'live')
      cmd.terminalGetState.mockResolvedValueOnce({
        sessionId: 's5',
        phase: 'idle',
        cursor: 0,
        snapshotOffset: 0,
        minOffset: 0,
        acked: 0,
        mode: 'realtime',
        stopped: true,
        historyBytes: 0,
      })

      await store.reconcileState('s5')

      const buffer = store.getBuffer('s5')!
      expect(buffer.sessionStopped).toBe(true)
      expect(buffer.subscribed).toBe(false)
      expect(store.sendInput('s5', 'ls')).toBe(false)
    })

    it('对账命令异常：保留既有信念（不误判为未订阅）', async () => {
      await store.subscribeSession('s6')
      emitState('s6', 'live')
      cmd.terminalGetState.mockRejectedValueOnce(new Error('link command failed'))

      await store.reconcileState('s6')

      expect(store.getBuffer('s6')!.subscribed).toBe(true)
    })
  })

  describe('running 广播不得破坏存活会话', () => {
    it('未停止的 buffer：markSessionRunning 不清订阅信念与渲染游标', async () => {
      await store.subscribeSession('s7')
      emitState('s7', 'live')
      const buffer = store.getBuffer('s7')!
      buffer.lastRenderedOffset = 256
      cmd.terminalSubscribe.mockClear()

      store.markSessionRunning('s7')
      await flushAsync()

      expect(buffer.subscribed).toBe(true)
      expect(buffer.phase).toBe('live')
      expect(buffer.lastRenderedOffset).toBe(256)
      expect(store.sendInput('s7', 'ls')).toBe(true)
      // 已订阅：不重建链路
      expect(cmd.terminalSubscribe).not.toHaveBeenCalled()
    })

    it('已停止的 buffer：markSessionRunning 复位游标与信念并重新订阅', async () => {
      await store.subscribeSession('s8')
      emitState('s8', 'live')
      store.getBuffer('s8')!.lastRenderedOffset = 256
      store.markSessionStopped('s8')
      cmd.terminalSubscribe.mockClear()

      store.markSessionRunning('s8')
      await flushAsync()

      const buffer = store.getBuffer('s8')!
      expect(buffer.sessionStopped).toBe(false)
      expect(buffer.lastRenderedOffset).toBeNull()
      expect(buffer.subscribed).toBe(false)
      expect(cmd.terminalSubscribe).toHaveBeenCalledWith('s8')
    })
  })

  describe('历史跨洞（Rust 缓存检出丢帧字节洞）', () => {
    it('检出字节洞：清屏 + 锚定重播 + 一次性提示历史不完整', async () => {
      await store.subscribeSession('s9')
      emitState('s9', 'live')
      store.getBuffer('s9')!.lastRenderedOffset = 64
      const onClear = vi.fn()
      const onTruncated = vi.fn()
      cmd.terminalGetHistory.mockResolvedValueOnce({
        from: 64,
        minOffset: 0,
        snapshotOffset: 128,
        historyBytes: 64,
        dataBase64: b64('xy'),
        gapDetected: true,
      })

      store.registerRealtimeHandler('s9', { onOutput: vi.fn(), onClear, onTruncated })
      await flushAsync()

      expect(onClear).toHaveBeenCalledTimes(1)
      expect(onTruncated).toHaveBeenCalledTimes(1)
      expect(store.getBuffer('s9')!.headTrimmed).toBe(true)
      // 清屏后按快照重新锚定游标
      expect(store.getBuffer('s9')!.lastRenderedOffset).toBe(128)
    })

    it('无洞（gapDetected 缺省）：不清屏', async () => {
      await store.subscribeSession('s10')
      emitState('s10', 'live')
      store.getBuffer('s10')!.lastRenderedOffset = 64
      const onClear = vi.fn()
      cmd.terminalGetHistory.mockResolvedValueOnce({
        from: 64,
        minOffset: 0,
        snapshotOffset: 128,
        historyBytes: 64,
        dataBase64: b64('xy'),
      })

      store.registerRealtimeHandler('s10', { onOutput: vi.fn(), onClear })
      await flushAsync()

      expect(onClear).not.toHaveBeenCalled()
      expect(store.getBuffer('s10')!.lastRenderedOffset).toBe(128)
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
      emitState('s1', 'live')
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
      emitState('s1', 'live')
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
      emitState('s1', 'live')
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
      emitState('s1', 'live')
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

    it('重同步信号：清屏 + 游标重锚到 min_offset + 提示一次；重播帧按重锚游标无缺口接收', async () => {
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
      emitState('s1', 'live')
      store.registerRealtimeHandler('s1', {
        onOutput: (d: Uint8Array) => outputs.push(new TextDecoder().decode(d)),
        onClear,
        onTruncated,
      })
      await flushAsync()
      expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(3)

      // 桌面端显式重同步：驻留起点 20（游标 3 已被环淘汰）→ 清屏 + 重锚 + 提示
      eventHandlers['terminal-resync']!({
        payload: { session_id: 's1', min_offset: 20, snapshot_offset: 30 },
      })
      await flushAsync()
      expect(onClear).toHaveBeenCalledTimes(1)
      expect(onTruncated).toHaveBeenCalledTimes(1)
      expect(onTruncated).toHaveBeenCalledWith(20)
      expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(20)
      expect(store.getBuffer('s1')!.minOffset).toBe(20)

      // 重播帧恰从 20 起（Rust 重锚后作为实时帧直达）：无缺口判定，直接交付
      emitFrame('s1', 20, 23, 'tuv')
      await flushAsync()
      expect(outputs).toEqual(['abc', 'tuv'])
      expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(23)

      // 重复重同步：清屏继续，但同一订阅者只提示一次
      eventHandlers['terminal-resync']!({
        payload: { session_id: 's1', min_offset: 40, snapshot_offset: 50 },
      })
      await flushAsync()
      expect(onClear).toHaveBeenCalledTimes(2)
      expect(onTruncated).toHaveBeenCalledTimes(1)
      expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(40)
    })

    it('P1：重播未完成（phase≠live）时 spliceHistory 等待，history_end 到达后才取历史', async () => {
      const outputs: string[] = []
      // 注意此处不先 emitState('live')——正是测试等待路径
      store.registerRealtimeHandler('s1', {
        onOutput: (d: Uint8Array) => outputs.push(new TextDecoder().decode(d)),
      })
      // 未到 live：getHistory 不得发起（WS 重播段未入缓存，缓存优先会命中部分历史）
      await flushAsync()
      expect(cmd.terminalGetHistory).not.toHaveBeenCalled()
      // history_end 落地 → 等待被唤醒 → 全量拉取
      cmd.terminalGetHistory.mockResolvedValueOnce({
        from: 0,
        minOffset: 0,
        snapshotOffset: 3,
        historyBytes: 3,
        dataBase64: b64('abc'),
      })
      emitState('s1', 'live')
      await flushAsync()
      expect(cmd.terminalGetHistory).toHaveBeenCalledWith('s1', 0)
      expect(outputs).toEqual(['abc'])
      expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(3)
      expect(store.getBuffer('s1')!.historyPreparing).toBe(false)
    })

    it('P1：等不到 live（订阅失败/停止）→ 超时复位拼接态，不悬挂', async () => {
      vi.useFakeTimers()
      try {
        const onReplayDone = vi.fn()
        store.registerRealtimeHandler('s1', { onOutput: vi.fn(), onReplayDone })
        // 越过 SPLICE_WAIT_LIVE_TIMEOUT_MS（8000）：超时兜底复位
        await vi.advanceTimersByTimeAsync(8001)
        expect(cmd.terminalGetHistory).not.toHaveBeenCalled()
        expect(store.getBuffer('s1')!.historyPreparing).toBe(false)
        expect(onReplayDone).toHaveBeenCalled()
      } finally {
        vi.useRealTimers()
      }
    })

    it('P2：resetCursor 后重拼接 from=0 全量回放（重进页面语义，xterm 全新实例）', async () => {
      const outputs: string[] = []
      emitState('s1', 'live')
      // 首次进入：历史 [0,6) 写入，游标 6
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

      // 重进：resetCursor + forceReplay → from=0 全量回放（含 [0,6) 既有历史）
      cmd.terminalGetHistory.mockClear()
      cmd.terminalGetHistory.mockResolvedValueOnce({
        from: 0,
        minOffset: 0,
        snapshotOffset: 9,
        historyBytes: 9,
        dataBase64: b64('abcdefghi'),
      })
      store.resetCursor('s1')
      store.forceReplay('s1')
      await flushAsync()
      expect(cmd.terminalGetHistory).toHaveBeenCalledWith('s1', 0)
      expect(outputs).toEqual(['abcdef', 'abcdefghi'])
      expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(9)
    })

    it('P4：历史写入管线异常 → 复位拼接态不悬挂，后续实时帧正常消费', async () => {
      const outputs: string[] = []
      const onReplayDone = vi.fn()
      emitState('s1', 'live')
      cmd.terminalGetHistory.mockResolvedValueOnce({
        from: 0,
        minOffset: 0,
        snapshotOffset: 3,
        historyBytes: 3,
        dataBase64: b64('abc'),
      })
      store.registerRealtimeHandler('s1', {
        onOutput: (d: Uint8Array) => outputs.push(new TextDecoder().decode(d)),
        writeParsed: () => Promise.reject(new Error('xterm disposed')),
        onReplayDone,
      })
      await flushAsync()
      // 写入失败：拼接态复位 + onReplayDone（不永久缓冲）；游标未推进（缺口自愈兜底）
      expect(store.getBuffer('s1')!.historyPreparing).toBe(false)
      expect(onReplayDone).toHaveBeenCalled()
      expect(store.getBuffer('s1')!.lastRenderedOffset).toBeNull()
      // 失败后可继续消费实时帧（不进入永久缓冲）
      cmd.terminalGetHistory.mockClear()
      emitFrame('s1', 3, 6, 'def')
      expect(outputs).toEqual(['def'])
    })

    it('P5：缺口冷却期内无新 gap 帧 → 定时器到期主动续传补回（防流尾缺口永久）', async () => {
      vi.useFakeTimers()
      try {
        const outputs: string[] = []
        emitState('s1', 'live')
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
        await vi.advanceTimersByTimeAsync(0)
        expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(3)

        // 缺口帧（start 6 > 游标 3）：冷却期外 → 立即重拼接补 [3,9)
        cmd.terminalGetHistory.mockClear()
        cmd.terminalGetHistory.mockResolvedValueOnce({
          from: 3,
          minOffset: 3,
          snapshotOffset: 9,
          historyBytes: 6,
          dataBase64: b64('defghi'),
        })
        emitFrame('s1', 6, 9, 'ghi')
        await vi.advanceTimersByTimeAsync(0)
        expect(outputs).toEqual(['abc', 'defghi'])
        expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(9)

        // 再次缺口（冷却期内）：不立即重拼接；冷期结束后定时器主动补一次
        cmd.terminalGetHistory.mockClear()
        cmd.terminalGetHistory.mockResolvedValueOnce({
          from: 9,
          minOffset: 9,
          snapshotOffset: 12,
          historyBytes: 3,
          dataBase64: b64('jkl'),
        })
        emitFrame('s1', 12, 15, 'mno')
        await vi.advanceTimersByTimeAsync(0)
        expect(cmd.terminalGetHistory).not.toHaveBeenCalled()
        // 越过 GAP_RESPLICE_COOLDOWN_MS（3000）：定时器触发续传
        await vi.advanceTimersByTimeAsync(3000)
        expect(cmd.terminalGetHistory).toHaveBeenCalledWith('s1', 9)
        expect(outputs).toEqual(['abc', 'defghi', 'jkl'])
        expect(store.getBuffer('s1')!.lastRenderedOffset).toBe(12)
      } finally {
        vi.useRealTimers()
      }
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
      emitState('s1', 'live')
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
      emitState('s1', 'live')
      store.registerRealtimeHandler('s1', { onOutput: vi.fn() })
      await flushAsync()
      expect(cmd.terminalSetMode).toHaveBeenCalledWith('s1', 'realtime')

      store.unregisterRealtimeHandler('s1')
      await flushAsync()
      expect(cmd.terminalSetMode).toHaveBeenCalledWith('s1', 'batch')
    })

    it('段2 订阅与页面进出严格配对：注册 → page_subscribe；注销 → page_unsubscribe', async () => {
      emitState('s1', 'live')
      store.registerRealtimeHandler('s1', { onOutput: vi.fn() })
      await flushAsync()
      expect(cmd.terminalPageSubscribe).toHaveBeenCalledWith('s1')
      expect(cmd.terminalPageUnsubscribe).not.toHaveBeenCalled()

      store.unregisterRealtimeHandler('s1')
      await flushAsync()
      expect(cmd.terminalPageUnsubscribe).toHaveBeenCalledWith('s1')
    })

    it('段1 订阅（会话级）不经页面进出触发：注册/注销不新增 terminalSubscribe', async () => {
      // beforeEach 已按「会话启动」语义订阅过一次（预注册事件监听），故以次数
      // 增量为准：页面进出不得再次触发段1 订阅、也不得取消段1
      const before = cmd.terminalSubscribe.mock.calls.length
      emitState('s1', 'live')
      store.registerRealtimeHandler('s1', { onOutput: vi.fn() })
      await flushAsync()
      store.unregisterRealtimeHandler('s1')
      await flushAsync()
      expect(cmd.terminalSubscribe.mock.calls.length).toBe(before)
      expect(cmd.terminalUnsubscribe).not.toHaveBeenCalled()
    })
  })

  describe('输出活动通知', () => {
    it('实时帧触发 terminal_output_activity（节流）', async () => {
      const outputs: string[] = []
      emitState('s1', 'live')
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