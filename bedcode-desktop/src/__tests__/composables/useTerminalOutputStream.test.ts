/**
 * useTerminalOutputStream 单元测试（07 快照模型）
 *
 * 覆盖核心契约：连接/订阅消息构造、TB v2 帧解析（seq/事件数/len）、
 * last_rendered_seq 游标推进、seq 缺口检测（快照重订阅）、历史截断清屏
 * 判定（min_seq > last_rendered_seq + 1）、订阅确认前的帧缓冲、断线重连。
 * WebSocket 与 Tauri invoke 均以 mock 替身模拟。
 */

import { describe, it, expect, vi, beforeEach, beforeAll } from 'vitest'

// mock Tauri invoke（按命令分发：服务器状态 + 本地令牌）
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

import { invoke } from '@tauri-apps/api/core'
import {
  useTerminalOutputStream,
  type OutputStreamFrame,
} from '@/composables/useTerminalOutputStream'
import { makeServerStatusInfo } from '@/__tests__/fixtures/server'

/** mock invoke 分发：get_server_status → 端口；get_local_ws_token → 令牌 */
function mockInvoke() {
  ;(invoke as unknown as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
    if (cmd === 'get_server_status') return Promise.resolve(makeServerStatusInfo({ port: 8765 }))
    if (cmd === 'get_local_ws_token') return Promise.resolve('test-token-abc')
    return Promise.resolve({})
  })
}

/** 最小 WebSocket mock：记录发送消息、支持手动触发事件 */
class MockWebSocket {
  static OPEN = 1
  static instances: MockWebSocket[] = []
  url: string
  readyState = 0
  binaryType = ''
  sent: string[] = []
  sentBinary: ArrayBuffer[] = []
  onopen: ((ev: unknown) => void) | null = null
  onmessage: ((ev: { data: unknown }) => void) | null = null
  onclose: ((ev: unknown) => void) | null = null
  onerror: ((ev: unknown) => void) | null = null

  constructor(url: string) {
    this.url = url
    MockWebSocket.instances.push(this)
  }

  send(data: string | ArrayBuffer) {
    if (typeof data === 'string') {
      this.sent.push(data)
    } else {
      this.sentBinary.push(data)
    }
  }

  close() {
    this.readyState = 3
  }

  // ===== 测试辅助 =====
  open() {
    this.readyState = 1
    this.onopen?.({})
  }

  text(raw: string) {
    this.onmessage?.({ data: raw })
  }

  /** TB v2 帧：seq = 帧内首事件 index；flags 高 7 位 = 事件数 - 1；len = 字节数 */
  binary(bytes: Uint8Array | number[], seq = 0, eventCount = 1, isWaiting = false) {
    const buf = new ArrayBuffer(16 + bytes.length)
    const view = new DataView(buf)
    view.setUint8(0, 0x54)
    view.setUint8(1, 0x42)
    view.setUint8(2, 2)
    view.setUint8(3, ((eventCount - 1) << 1) | (isWaiting ? 1 : 0))
    view.setBigUint64(4, BigInt(seq), true)
    view.setUint32(12, bytes.length, true)
    new Uint8Array(buf, 16).set(bytes)
    this.onmessage?.({ data: buf })
  }

  closeFromServer() {
    this.readyState = 3
    this.onclose?.({})
  }
}

/** 快照订阅响应：min_seq / max_seq(=snapshot_seq) / history_count */
function subscribeResponse(minSeq: number, snapshotSeq: number, historyCount: number) {
  return JSON.stringify({
    type: 'terminal',
    payload: {
      message_id: 'req-1',
      expect_response: false,
      timestamp: 1,
      session_id: 's1',
      token: '',
      payload: {
        action: {
          type: 'subscribe_response',
          min_seq: minSeq,
          max_seq: snapshotSeq,
          history_count: historyCount,
          mode: 'reset',
          min_offset: 0,
          max_offset: 0,
        },
      },
    },
  })
}

/** 等待异步 connect 流程完成 */
async function flushAsync() {
  await new Promise((r) => setTimeout(r, 0))
  await new Promise((r) => setTimeout(r, 0))
}

/** 解码背压 ack 帧（TB v2 头 + ACK 标志位 0x02 + acked_seq(8 LE) + session_id UTF-8 负载） */
function decodeAck(ab: ArrayBuffer): { ackedSeq: number; sessionId: string } {
  const view = new DataView(ab)
  expect(view.getUint8(0)).toBe(0x54)
  expect(view.getUint8(1)).toBe(0x42)
  expect(view.getUint8(2)).toBe(2)
  expect(view.getUint8(3)).toBe(0x02)
  const ackedSeq = Number(view.getBigUint64(4, true))
  const len = view.getUint32(12, true)
  const sessionId = new TextDecoder().decode(new Uint8Array(ab, 16, len))
  return { ackedSeq, sessionId }
}

describe('useTerminalOutputStream', () => {
  let frames: OutputStreamFrame[]
  let resets: number
  let truncated: number[]
  let stream: ReturnType<typeof useTerminalOutputStream>

  beforeAll(() => {
    // composable 内部使用全局 WebSocket 构造连接
    ;(globalThis as Record<string, unknown>).WebSocket = MockWebSocket
  })

  beforeEach(() => {
    MockWebSocket.instances = []
    frames = []
    resets = 0
    truncated = []
    vi.clearAllMocks()
    mockInvoke()
    stream = useTerminalOutputStream({
      onData: (f) => frames.push(f),
      onReset: () => resets++,
      onTruncated: (m) => truncated.push(m),
    })
  })

  it('连接本地环回端点并以无参快照订阅（首次全量）', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    // 携带一次性令牌（服务端握手校验，环回 IP 之外的第二道防线）
    expect(ws.url).toBe('ws://127.0.0.1:8765/ws/terminal/local?token=test-token-abc')
    expect(ws.sent).toHaveLength(0) // 未订阅前不发消息

    // 订阅先于握手完成：挂起，onopen 时自动发送
    stream.subscribe()
    expect(ws.sent).toHaveLength(0)
    ws.open()
    expect(ws.sent).toHaveLength(1)
    const msg = JSON.parse(ws.sent[0])
    expect(msg.type).toBe('terminal')
    expect(msg.payload.session_id).toBe('s1')
    expect(msg.payload.token).toBe('')
    expect(msg.payload.timestamp).toEqual(expect.any(Number))
    expect(msg.payload.payload.action).toEqual({ type: 'subscribe' })
  })

  it('快照订阅：帧推进 last_rendered_seq 并逐帧回调', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    ws.text(subscribeResponse(0, 5, 3))

    ws.binary([1, 2, 3], 0)
    ws.binary([4, 5], 1)
    expect(frames.map((f) => [f.seq, f.lastSeq, [...f.data]])).toEqual([
      [0, 0, [1, 2, 3]],
      [1, 1, [4, 5]],
    ])
  })

  it('TB v2 帧解析：seq / 事件数 / len 字段', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    ws.text(subscribeResponse(0, 5, 3))

    // 合并帧：3 事件 seq 7..9 → lastSeq = 9
    ws.binary([97, 98, 99], 7, 3, true)
    expect(frames).toHaveLength(1)
    expect(frames[0].seq).toBe(7)
    expect(frames[0].eventCount).toBe(3)
    expect(frames[0].lastSeq).toBe(9)
    expect(frames[0].isWaiting).toBe(true)
    expect([...frames[0].data]).toEqual([97, 98, 99])
  })

  it('min_seq > 0 时触发 onTruncated（历史头部被环形淘汰）', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    ws.text(subscribeResponse(128, 200, 3))

    expect(truncated).toEqual([128])
    expect(resets).toBe(0) // 未截断（128 ≤ null+1），不清屏
  })

  it('订阅确认前的回放帧先缓冲，确认后按序交付', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()

    // 帧先于控制消息到达（服务端两条消息路径的竞态）
    ws.binary([9, 9, 9], 0)
    ws.binary([8, 8], 1)
    expect(frames).toHaveLength(0) // 未确认，缓冲

    ws.text(subscribeResponse(0, 100, 3))
    expect(frames.map((f) => [f.seq, [...f.data]])).toEqual([
      [0, [9, 9, 9]],
      [1, [8, 8]],
    ])
  })

  it('seq 缺口：单帧偶发跳过不重订阅，连续 3 次才真正快照重订阅', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    ws.text(subscribeResponse(0, 100, 3))

    // 正常推进到 last_rendered_seq=4
    for (let i = 0; i < 5; i++) {
      ws.binary([i], i)
    }
    expect(frames).toHaveLength(5)

    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})

    // 第 1 次缺口：帧首 seq 6 ≠ lastRendered(4)+1（事件 5 丢失）
    // 偶发缺口不 resubscribe，按非严格路径推进游标：lastSeq 之后的字节仍写入管线
    ws.binary([9], 6)
    expect(errorSpy).not.toHaveBeenCalled()
    expect(warnSpy).toHaveBeenCalled()
    expect(frames).toHaveLength(6) // 跳过缺口帧，但仍交付
    expect(MockWebSocket.instances.length).toBe(1) // 未触发重连

    // 第 2 次缺口：frame.start=8、lastRendered=6，仍 <3 次不 resubscribe
    ws.binary([10], 8)
    expect(errorSpy).not.toHaveBeenCalled()
    expect(warnSpy).toHaveBeenCalledTimes(2)
    expect(MockWebSocket.instances.length).toBe(1)

    // 第 3 次缺口：frame.start=10、lastRendered=8，连续 3 次 → 真正 resubscribe
    ws.binary([11], 10)
    expect(errorSpy).toHaveBeenCalled()
    expect(errorSpy.mock.calls[0]![0]).toMatch(/persistent seq gap \(3x\)/)

    // 强制重连：旧连接关闭，新连接建立后自动重新订阅（无参——服务端恒全量重播）
    await flushAsync()
    expect(MockWebSocket.instances.length).toBeGreaterThanOrEqual(2)
    const ws2 = MockWebSocket.instances[MockWebSocket.instances.length - 1]
    ws2.open()
    expect(ws2.sent).toHaveLength(1)
    const msg = JSON.parse(ws2.sent[0])
    expect(msg.payload.payload.action).toEqual({ type: 'subscribe' })

    // 快照重播：已渲染部分（seq ≤ 10）跳过，缺口从 seq 11 无缝衔接
    ws2.text(subscribeResponse(0, 100, 3))
    ws2.binary([11, 12, 13], 11, 3)
    expect(frames[frames.length - 1]!.lastSeq).toBe(13)

    errorSpy.mockRestore()
    warnSpy.mockRestore()
  })

  it('断线自动重连：保留 last_rendered_seq，重播跳过已渲染部分', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    ws.text(subscribeResponse(0, 100, 3))

    ws.binary([1, 2], 0)
    ws.binary([3], 1)

    // 服务端断线（重连退避 500ms）
    ws.closeFromServer()
    await new Promise((r) => setTimeout(r, 600))
    await flushAsync()
    expect(MockWebSocket.instances.length).toBeGreaterThanOrEqual(2)
    const ws2 = MockWebSocket.instances[MockWebSocket.instances.length - 1]
    ws2.open()

    // 重连后自动恢复订阅（无参）
    expect(ws2.sent).toHaveLength(1)
    const msg = JSON.parse(ws2.sent[0])
    expect(msg.payload.payload.action).toEqual({ type: 'subscribe' })

    // 重播帧 seq 0/1（≤ lastRendered 1）被跳过，seq 2 起继续渲染
    ws2.text(subscribeResponse(0, 100, 3))
    ws2.binary([0], 0)
    ws2.binary([1], 1)
    ws2.binary([4, 5], 2)
    expect(frames.map((f) => f.lastSeq)).toEqual([0, 1, 2])
  })

  it('历史截断：min_seq > last_rendered_seq + 1 → 清屏 + 全量重播', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    ws.text(subscribeResponse(0, 100, 3))

    // 已渲染 seq 0、1（lastRenderedSeq = 1）
    ws.binary([1], 0)
    ws.binary([2], 1)
    expect(frames).toHaveLength(2)

    // 重订阅响应 min_seq=200：已渲染区被环形淘汰 → 截断 → 清屏全量重播
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
    ws.text(subscribeResponse(200, 300, 3))
    warnSpy.mockRestore()

    expect(resets).toBe(1) // onReset 清屏
    expect(truncated).toEqual([200])

    // 重播帧从 min_seq 起：全量渲染（无跳过—— lastRenderedSeq 已重置）
    ws.binary([200], 200)
    expect(frames.map((f) => f.lastSeq)).toEqual([0, 1, 200])
  })

  it('stop 后不再重连', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    stream.stop()

    ws.closeFromServer()
    await flushAsync()
    expect(MockWebSocket.instances).toHaveLength(1) // 无新连接
  })

  it('SESSION_NOT_FOUND：有限重试后停止（会话启动中/已停止场景）', async () => {
    stream.start('s1')
    await flushAsync()
    stream.subscribe()
    MockWebSocket.instances[0].open()

    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})

    const sendError = () =>
      JSON.stringify({
        type: 'error',
        payload: { code: 'SESSION_NOT_FOUND', message: 'Session s1 not found' },
      })

    // 第 1 次错误 → 退避重连
    MockWebSocket.instances[0].text(sendError())
    await new Promise((r) => setTimeout(r, 600))
    await flushAsync()
    expect(MockWebSocket.instances.length).toBe(2)
    MockWebSocket.instances[1].open()
    MockWebSocket.instances[1].text(sendError())

    // 第 2 次错误 → 退避重连
    await new Promise((r) => setTimeout(r, 1100))
    await flushAsync()
    expect(MockWebSocket.instances.length).toBe(3)
    MockWebSocket.instances[2].open()
    MockWebSocket.instances[2].text(sendError())

    // 第 3 次错误 → 达到上限，停止重连
    await new Promise((r) => setTimeout(r, 2100))
    await flushAsync()
    expect(MockWebSocket.instances.length).toBe(3) // 无第 4 个连接
    expect(warnSpy).toHaveBeenCalled()

    errorSpy.mockRestore()
    warnSpy.mockRestore()
  })

  it('非 SESSION_NOT_FOUND 错误：无限退避重连（如瞬时网络错误）', async () => {
    stream.start('s1')
    await flushAsync()
    stream.subscribe()
    MockWebSocket.instances[0].open()

    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
    MockWebSocket.instances[0].text(
      JSON.stringify({ type: 'error', payload: { code: 'INTERNAL', message: 'boom' } }),
    )
    await new Promise((r) => setTimeout(r, 600))
    await flushAsync()

    // 仍会重连（错误类型不触发停止）
    expect(MockWebSocket.instances.length).toBe(2)
    errorSpy.mockRestore()
  })

  describe('背压 ack（04-06：写作解析完成 confirmWriteParsed 回发）', () => {
    async function setupAck() {
      stream.start('s1')
      await flushAsync()
      const ws = MockWebSocket.instances[0]
      stream.subscribe()
      ws.open()
      ws.text(subscribeResponse(0, 1000, 3))
      return ws
    }

    it('写解析完成后回发 ack 帧：携已渲染到的 last_rendered_seq 与 session_id', async () => {
      const ws = await setupAck()
      // 交付若干帧，游标推进到 lastSeq=2；尚未写解析 → 无 ack
      ws.binary([1, 2, 3], 0)
      ws.binary([4, 5], 1)
      ws.binary([6], 2)
      expect(ws.sentBinary).toHaveLength(0)

      stream.confirmWriteParsed()
      expect(ws.sentBinary).toHaveLength(1)
      expect(decodeAck(ws.sentBinary[0])).toEqual({ ackedSeq: 2, sessionId: 's1' })
    })

    it('ack 节流：达 64KB 阈值才回发，且携最新推进水位', async () => {
      const ws = await setupAck()
      // 首帧建立 ack 基线（无条件立即回发）
      ws.binary([1, 2, 3], 0)
      stream.confirmWriteParsed()
      expect(ws.sentBinary).toHaveLength(1)
      expect(decodeAck(ws.sentBinary[0]).ackedSeq).toBe(0)

      // 66KB 帧（> 64KB 阈值）+ 1B 帧：累计 66KB+1 > 阈值 → 立即回发最新水位
      ws.binary(new Uint8Array(66 * 1024), 1)
      ws.binary([9], 2)
      stream.confirmWriteParsed()
      expect(ws.sentBinary).toHaveLength(2)
      expect(decodeAck(ws.sentBinary[1]).ackedSeq).toBe(2)
    })

    it('节流窗口内未达阈值不逐帧回发（空闲兜底 timer 挂起）', async () => {
      const ws = await setupAck()
      ws.binary([1], 0)
      ws.binary([2], 1)
      ws.binary([3], 2)
      stream.confirmWriteParsed() // 首 ack 基线
      expect(ws.sentBinary).toHaveLength(1)

      // 后续小批累计 11B << 64KB：挂起兜底 timer，不立即回发
      for (let i = 3; i < 14; i++) ws.binary([i], i)
      stream.confirmWriteParsed()
      expect(ws.sentBinary).toHaveLength(1)
      stream.stop() // 清理兜底 timer，避免测试尾部遗留下定时器
    })

    it('新会话 start 后 ack 水位重置（新坐标空间），ack 基线立即回发', async () => {
      const ws = await setupAck()
      ws.binary([1, 2, 3], 0)
      stream.confirmWriteParsed()
      expect(ws.sentBinary).toHaveLength(1)

      stream.start('s2')
      await flushAsync()
      const ws2 = MockWebSocket.instances[MockWebSocket.instances.length - 1]
      stream.subscribe()
      ws2.open()
      ws2.text(subscribeResponse(0, 100, 3))

      // 新会话首帧：ack 基线立即回发（旧 ack 水位不干扰）
      ws2.binary([7], 0)
      stream.confirmWriteParsed()
      expect(ws2.sentBinary).toHaveLength(1)
      expect(decodeAck(ws2.sentBinary[0])).toEqual({ ackedSeq: 0, sessionId: 's2' })
    })

    it('stop 后 confirmWriteParsed 不再回发 ack', async () => {
      const ws = await setupAck()
      ws.binary([1], 0)
      stream.stop()
      stream.confirmWriteParsed()
      expect(ws.sentBinary).toHaveLength(0)
    })
  })
})
