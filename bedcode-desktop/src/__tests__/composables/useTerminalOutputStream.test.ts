/**
 * useTerminalOutputStream 单元测试（07 快照模型 + TB v3 字节化）
 *
 * 覆盖核心契约：连接/订阅消息构造、TB v3 帧解析（start_offset/len）、
 * last_rendered_offset 游标推进、offset 缺口检测（快照重订阅）、跨帧裁剪
 * （重播首帧跨游标 → 裁掉前半段，零重复）、历史截断清屏判定
 * （min_offset > last_rendered_offset）、订阅确认前的帧缓冲、断线重连。
 * WebSocket 与 Tauri invoke 均以 mock 替身模拟。
 */

import { describe, it, expect, vi, beforeEach, beforeAll } from 'vitest'

// mock Tauri invoke（按命令分发：服务器状态 + 本地令牌）
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

import { invoke } from '@tauri-apps/api/core'
import { logger } from '@/utils/frontendLogger'
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

  /** TB v3 帧：start_offset = 帧内首字节累计偏移；len = 字节数（无事件数编码） */
  binary(bytes: Uint8Array | number[], startOffset = 0, isWaiting = false) {
    const data = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes)
    const buf = new ArrayBuffer(16 + data.length)
    const view = new DataView(buf)
    view.setUint8(0, 0x54)
    view.setUint8(1, 0x42)
    view.setUint8(2, 3)
    view.setUint8(3, isWaiting ? 1 : 0)
    view.setBigUint64(4, BigInt(startOffset), true)
    view.setUint32(12, data.length, true)
    new Uint8Array(buf, 16).set(data)
    this.onmessage?.({ data: buf })
  }

  closeFromServer() {
    this.readyState = 3
    this.onclose?.({})
  }
}

/** 快照订阅响应（旧路由 wire 字段名承载字节语义值：min_seq=min_offset /
 *  max_seq=snapshot_offset / history_count=history_bytes） */
function subscribeResponse(minOffset: number, snapshotOffset: number, historyBytes: number) {
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
          min_seq: minOffset,
          max_seq: snapshotOffset,
          history_count: historyBytes,
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

/** 解码背压 ack 帧（TB v3 头 + ACK 标志位 0x02 + acked_offset(8 LE) + session_id UTF-8 负载） */
function decodeAck(ab: ArrayBuffer): { ackedOffset: number; sessionId: string } {
  const view = new DataView(ab)
  expect(view.getUint8(0)).toBe(0x54)
  expect(view.getUint8(1)).toBe(0x42)
  expect(view.getUint8(2)).toBe(3)
  expect(view.getUint8(3)).toBe(0x02)
  const ackedOffset = Number(view.getBigUint64(4, true))
  const len = view.getUint32(12, true)
  const sessionId = new TextDecoder().decode(new Uint8Array(ab, 16, len))
  return { ackedOffset, sessionId }
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

  it('快照订阅：帧推进 last_rendered_offset 并逐帧回调', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    ws.text(subscribeResponse(0, 5, 3))

    ws.binary([1, 2, 3], 0)
    ws.binary([4, 5], 3)
    expect(frames.map((f) => [f.startOffset, f.endOffset, [...f.data]])).toEqual([
      [0, 3, [1, 2, 3]],
      [3, 5, [4, 5]],
    ])
  })

  it('TB v3 帧解析：start_offset / len / end_offset 字段（无事件数编码）', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    ws.text(subscribeResponse(0, 5, 3))

    ws.binary([97, 98, 99], 7, true)
    expect(frames).toHaveLength(1)
    expect(frames[0].startOffset).toBe(7)
    expect(frames[0].endOffset).toBe(10)
    expect(frames[0].isWaiting).toBe(true)
    expect([...frames[0].data]).toEqual([97, 98, 99])
  })

  it('min_offset > 0 时触发 onTruncated（历史头部被环形淘汰）', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    ws.text(subscribeResponse(128, 200, 3))

    expect(truncated).toEqual([128])
    expect(resets).toBe(0) // 未截断（128 ≤ null），不清屏
  })

  it('订阅确认前的回放帧先缓冲，确认后按序交付', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()

    // 帧先于控制消息到达（服务端两条消息路径的竞态）
    ws.binary([9, 9, 9], 0)
    ws.binary([8, 8], 3)
    expect(frames).toHaveLength(0) // 未确认，缓冲

    ws.text(subscribeResponse(0, 100, 3))
    expect(frames.map((f) => [f.startOffset, [...f.data]])).toEqual([
      [0, [9, 9, 9]],
      [3, [8, 8]],
    ])
  })

  it('跨帧裁剪：重播首帧跨过已渲染游标 → 裁掉前半段，零重复渲染', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    ws.text(subscribeResponse(0, 100, 3))

    // 已渲染 [0,2)：字节 ab（游标 = 2）
    ws.binary([97, 98], 0)
    expect(frames).toHaveLength(1)

    // 断线重连 → 服务端全量重播：首帧 [0,5) 覆盖游标 2，重叠 2 字节
    ws.closeFromServer()
    await new Promise((r) => setTimeout(r, 600))
    await flushAsync()
    const ws2 = MockWebSocket.instances[MockWebSocket.instances.length - 1]
    ws2.open()
    ws2.text(subscribeResponse(0, 100, 3))
    // 重播帧 [0,5) = "abcde"：跨游标裁剪 → 只渲染"cde"（[2,5)），零重复
    ws2.binary([97, 98, 99, 100, 101], 0)
    expect(frames).toHaveLength(2)
    expect(frames[1].startOffset).toBe(0) // 原帧区间标注（数据已裁剪）
    expect([...frames[1].data]).toEqual([99, 100, 101])
    expect(frames[1].endOffset).toBe(5) // 游标推进到帧末

    // 后续连续帧正常渲染
    ws2.binary([102], 5)
    expect([...frames[2].data]).toEqual([102])
  })

  it('offset 缺口：立即快照重订阅补回缺失字节，冷却期内缺口跳过不渲染', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    ws.text(subscribeResponse(0, 100, 3))

    // 正常推进到 last_rendered_offset=5（5 个单字节帧 [0,5)）
    for (let i = 0; i < 5; i++) {
      ws.binary([i], i)
    }
    expect(frames).toHaveLength(5)

    const warnSpy = vi.spyOn(logger, 'warn').mockImplementation(() => {})

    // 缺口：字节 5 丢失，帧从 6 起。立即快照重订阅补回；缺口帧不交付、不推进游标
    ws.binary([9], 6)
    expect(warnSpy).toHaveBeenCalledWith(expect.stringMatching(/offset gap, re-subscribing/))
    expect(frames).toHaveLength(5) // 缺口帧未交付（缺失字节由重播补回）

    // 强制重连：旧连接关闭，新连接建立后自动重新订阅（服务端恒全量重播）
    await flushAsync()
    expect(MockWebSocket.instances.length).toBeGreaterThanOrEqual(2)
    const ws2 = MockWebSocket.instances[MockWebSocket.instances.length - 1]
    ws2.open()
    expect(ws2.sent).toHaveLength(1)
    const msg = JSON.parse(ws2.sent[0])
    expect(msg.payload.payload.action).toEqual({ type: 'subscribe' })

    // 快照重播：已渲染部分（≤ 5）整帧跳过/剪切，缺失的 5 从重播补回，无缝衔接
    ws2.text(subscribeResponse(0, 100, 3))
    ws2.binary([5, 6, 7, 8], 5)
    expect(frames[frames.length - 1]!.endOffset).toBe(9)

    // 冷却期内（3s）再次缺口：不重复重订阅，缺口帧跳过不交付
    ws2.binary([10], 10)
    expect(warnSpy).toHaveBeenCalledTimes(1) // 冷却期内不再打 re-subscribing
    expect(MockWebSocket.instances.length).toBe(2) // 未再触发重连
    expect(frames[frames.length - 1]!.endOffset).toBe(9) // 缺口帧未交付

    warnSpy.mockRestore()
  })

  it('断线自动重连：保留 last_rendered_offset，重播跳过已渲染部分', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    ws.text(subscribeResponse(0, 100, 3))

    ws.binary([1, 2], 0)
    ws.binary([3], 2)

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

    // 重播帧 [0,2)/[2,3)（≤ lastRendered 3）跳过，[3,5) 起继续渲染
    ws2.text(subscribeResponse(0, 100, 3))
    ws2.binary([0], 0)
    ws2.binary([1], 1)
    ws2.binary([4, 5], 3)
    expect(frames.map((f) => f.endOffset)).toEqual([2, 3, 5])
  })

  it('历史截断：min_offset > last_rendered_offset → 清屏 + 全量重播', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    ws.text(subscribeResponse(0, 100, 3))

    // 已渲染 [0,2)（lastRenderedOffset = 2）
    ws.binary([1], 0)
    ws.binary([2], 1)
    expect(frames).toHaveLength(2)

    // 重订阅响应 min_offset=200：已渲染区被环形淘汰 → 截断 → 清屏全量重播
    const warnSpy = vi.spyOn(logger, 'warn').mockImplementation(() => {})
    ws.text(subscribeResponse(200, 300, 3))
    warnSpy.mockRestore()

    expect(resets).toBe(1) // onReset 清屏
    expect(truncated).toEqual([200])

    // 重播帧从 min_offset 起：全量渲染（无跳过—— lastRenderedOffset 已重置）
    ws.binary([200], 200)
    expect(frames.map((f) => f.endOffset)).toEqual([1, 2, 201])
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

    const errorSpy = vi.spyOn(logger, 'error').mockImplementation(() => {})
    const warnSpy = vi.spyOn(logger, 'warn').mockImplementation(() => {})

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

    const errorSpy = vi.spyOn(logger, 'error').mockImplementation(() => {})
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

    it('写解析完成后回发 ack 帧：携已渲染到的 last_rendered_offset 与 session_id', async () => {
      const ws = await setupAck()
      // 交付若干帧，游标推进到 endOffset=6；尚未写解析 → 无 ack
      ws.binary([1, 2, 3], 0)
      ws.binary([4, 5], 3)
      ws.binary([6], 5)
      expect(ws.sentBinary).toHaveLength(0)

      stream.confirmWriteParsed()
      expect(ws.sentBinary).toHaveLength(1)
      expect(decodeAck(ws.sentBinary[0])).toEqual({ ackedOffset: 6, sessionId: 's1' })
    })

    it('ack 节流：达 64KB 阈值才回发，且携最新推进水位', async () => {
      const ws = await setupAck()
      // 首帧建立 ack 基线（无条件立即回发）
      ws.binary([1, 2, 3], 0)
      stream.confirmWriteParsed()
      expect(ws.sentBinary).toHaveLength(1)
      expect(decodeAck(ws.sentBinary[0]).ackedOffset).toBe(3)

      // 66KB 帧（> 64KB 阈值）+ 1B 帧：累计 66KB+1 > 阈值 → 立即回发最新水位
      ws.binary(new Uint8Array(66 * 1024), 3)
      ws.binary([9], 3 + 66 * 1024)
      stream.confirmWriteParsed()
      expect(ws.sentBinary).toHaveLength(2)
      expect(decodeAck(ws.sentBinary[1]).ackedOffset).toBe(3 + 66 * 1024 + 1)
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
      expect(decodeAck(ws2.sentBinary[0])).toEqual({ ackedOffset: 1, sessionId: 's2' })
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