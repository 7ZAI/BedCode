/**
 * useTerminalOutputStream 单元测试
 *
 * 覆盖核心契约：连接/订阅消息构造、字节游标推进、连续性校验（不变量守护）、
 * reset 裁决清屏回调、订阅确认前的帧缓冲、断线重连自动恢复订阅。
 * WebSocket 与 Tauri invoke 均以 mock 替身模拟。
 */

import { describe, it, expect, vi, beforeEach, beforeAll } from 'vitest'

// mock Tauri invoke（按命令分发：服务器状态 + 本地令牌）
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

import { invoke } from '@tauri-apps/api/core'
import { useTerminalOutputStream, type OutputStreamFrame } from '@/composables/useTerminalOutputStream'

/** mock invoke 分发：get_server_status → 端口；get_local_ws_token → 令牌 */
function mockInvoke() {
  ;(invoke as unknown as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
    if (cmd === 'get_server_status') return Promise.resolve({ status: 'ok', port: 8765 })
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
  onopen: ((ev: unknown) => void) | null = null
  onmessage: ((ev: { data: unknown }) => void) | null = null
  onclose: ((ev: unknown) => void) | null = null
  onerror: ((ev: unknown) => void) | null = null

  constructor(url: string) {
    this.url = url
    MockWebSocket.instances.push(this)
  }

  send(data: string) {
    this.sent.push(data)
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

  binary(bytes: number[], startOffset = 0, endOffset = bytes.length, isWaiting = false) {
    const buf = new ArrayBuffer(20 + bytes.length)
    const view = new DataView(buf)
    view.setUint8(0, 0x54)
    view.setUint8(1, 0x42)
    view.setUint8(2, 1)
    view.setUint8(3, isWaiting ? 1 : 0)
    view.setBigUint64(4, BigInt(startOffset), true)
    view.setBigUint64(12, BigInt(endOffset), true)
    new Uint8Array(buf, 20).set(bytes)
    this.onmessage?.({ data: buf })
  }

  closeFromServer() {
    this.readyState = 3
    this.onclose?.({})
  }
}

function subscribeResponse(mode: 'incremental' | 'reset', minOffset = 0, maxOffset = 0) {
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
          min_seq: 0,
          max_seq: 5,
          history_count: 3,
          mode,
          min_offset: minOffset,
          max_offset: maxOffset,
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

describe('useTerminalOutputStream', () => {
  let frames: OutputStreamFrame[]
  let resets: Array<{ mode: string; minOffset: number }>
  let truncated: number[]
  let stream: ReturnType<typeof useTerminalOutputStream>

  beforeAll(() => {
    // composable 内部使用全局 WebSocket 构造连接
    ;(globalThis as Record<string, unknown>).WebSocket = MockWebSocket
  })

  beforeEach(() => {
    MockWebSocket.instances = []
    frames = []
    resets = []
    truncated = []
    vi.clearAllMocks()
    mockInvoke()
    stream = useTerminalOutputStream({
      onData: (f) => frames.push(f),
      onReset: (r) => resets.push(r),
      onTruncated: (m) => truncated.push(m),
    })
  })

  it('连接本地环回端点并以 null 游标订阅（首次全量）', async () => {
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
    expect(msg.payload.payload.action.type).toBe('subscribe')
    expect(msg.payload.payload.action.start_seq).toBeNull()
  })

  it('incremental 订阅：帧推进游标并逐帧回调', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    ws.text(subscribeResponse('incremental', 0, 100))

    ws.binary([1, 2, 3], 0, 3)
    ws.binary([4, 5], 3, 5)
    expect(frames.map((f) => [f.startOffset, f.endOffset, [...f.data]])).toEqual([
      [0, 3, [1, 2, 3]],
      [3, 5, [4, 5]],
    ])
  })

  it('reset 订阅：触发 onReset 清屏回调，游标重置后从 minOffset 开始回放', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    ws.text(subscribeResponse('reset', 40, 100))

    expect(resets).toHaveLength(1)
    expect(resets[0].minOffset).toBe(40)

    ws.binary([7, 8], 40, 42)
    expect(frames).toHaveLength(1)
    expect(frames[0].startOffset).toBe(40)
  })

  it('min_offset > 0 时触发 onTruncated（历史头部被环形淘汰）', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    ws.text(subscribeResponse('reset', 128, 200))

    expect(truncated).toEqual([128])
  })

  it('订阅确认前的回放帧先缓冲，确认后按序交付', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()

    // 帧先于控制消息到达（服务端两条消息路径的竞态）
    ws.binary([9, 9, 9], 0, 3)
    ws.binary([8, 8], 3, 5)
    expect(frames).toHaveLength(0) // 未确认，缓冲

    ws.text(subscribeResponse('incremental', 0, 100))
    expect(frames.map((f) => [f.startOffset, [...f.data]])).toEqual([
      [0, [9, 9, 9]],
      [3, [8, 8]],
    ])
  })

  it('连续性不变量破坏：保留游标按增量重订阅（避免全量重播风暴）', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    ws.text(subscribeResponse('incremental', 0, 100))

    // 正常推进到 5
    ws.binary([1], 0, 1)
    ws.binary([2], 1, 2)
    ws.binary([3], 2, 3)
    ws.binary([4], 3, 4)
    ws.binary([5], 4, 5)
    expect(frames).toHaveLength(5)

    // 违反：帧起点 6 而非 5（服务端背压丢事件 → 字节缺口）
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
    ws.binary([9], 6, 7)
    expect(errorSpy).toHaveBeenCalled()
    errorSpy.mockRestore()

    // 强制重连：旧连接关闭，新连接建立后自动重新订阅
    // 游标必须保留（start_seq=5）→ 服务端裁决 incremental，只补缺口；
    // 置 null 会导致全量重播（mode=Reset），大历史会话下反复重播形成自持风暴
    await flushAsync()
    expect(MockWebSocket.instances.length).toBeGreaterThanOrEqual(2)
    const ws2 = MockWebSocket.instances[MockWebSocket.instances.length - 1]
    ws2.open()
    expect(ws2.sent).toHaveLength(1)
    const msg = JSON.parse(ws2.sent[0])
    expect(msg.payload.payload.action.start_seq).toBe(5)

    // 服务端以 incremental 续传：缺口帧从游标处无缝衔接
    ws2.text(subscribeResponse('incremental', 0, 100))
    ws2.binary([6, 7, 8], 5, 8)
    expect(frames.map((f) => f.endOffset)).toEqual([1, 2, 3, 4, 5, 8])
  })

  it('断线自动重连：保留游标并从断点续传', async () => {
    stream.start('s1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    ws.text(subscribeResponse('incremental', 0, 100))

    ws.binary([1, 2], 0, 2)
    ws.binary([3], 2, 3)

    // 服务端断线（重连退避 500ms）
    ws.closeFromServer()
    await new Promise((r) => setTimeout(r, 600))
    await flushAsync()
    expect(MockWebSocket.instances.length).toBeGreaterThanOrEqual(2)
    const ws2 = MockWebSocket.instances[MockWebSocket.instances.length - 1]
    ws2.open()

    // 重连后自动恢复订阅，游标保留为 3
    expect(ws2.sent).toHaveLength(1)
    const msg = JSON.parse(ws2.sent[0])
    expect(msg.payload.payload.action.start_seq).toBe(3)

    // 续传帧与游标无缝衔接
    ws2.text(subscribeResponse('incremental', 0, 100))
    ws2.binary([4, 5], 3, 5)
    expect(frames.map((f) => f.endOffset)).toEqual([2, 3, 5])
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
      JSON.stringify({ type: 'error', payload: { code: 'INTERNAL', message: 'boom' } })
    )
    await new Promise((r) => setTimeout(r, 600))
    await flushAsync()

    // 仍会重连（错误类型不触发停止）
    expect(MockWebSocket.instances.length).toBe(2)
    errorSpy.mockRestore()
  })
})
