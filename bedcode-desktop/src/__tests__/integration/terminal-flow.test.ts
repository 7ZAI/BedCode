/**
 * 终端流组合集成测试（L2 场景 3）
 *
 * 协作实体：真实 xterm Terminal（happy-dom 中 open() 进带尺寸 DOM 元素可用，
 * 已实测 buffer 解析/渲染正常——无需桩）+ useTerminalOutputStream（真实，
 * 帧解析/游标连续性/订阅消息构造全部真实执行）+ useSessionStore（真实 Pinia）
 * + useTerminalInputMarkers（真实，输入标记联动）。
 *
 * 覆盖用户路径：会话创建 → 启动 → 输出流订阅 → 二进制帧渲染到 xterm buffer
 * → 连续性不变量（缺口触发保留游标重订阅）→ 输入回传参数构造（write_to_session
 * 参数）→ 输入标记记录 → 会话终止。
 *
 * 测试 seam：
 * - 只 mock @tauri-apps/api/core 的 invoke 边界 + 全局 WebSocket（本地环回
 *   WS 是网络边界，测试内无真实服务器；与 useTerminalOutputStream.test.ts
 *   的 MockWebSocket 模式一致，连接/握手/帧注入全部由测试驱动）
 * - xterm / Pinia / composables / store 全部真实执行
 * - fixture 数据取自工厂（makeServerStatusInfo / makeSessionInfo）
 *
 * 环境限制说明：
 * - 真实 timers：xterm 解析/渲染依赖真实异步 tick，不使用 fake timers
 * - invokeWithTimeout（session store 用）的 30s 兜底 setTimeout 在测试结束
 *   时被 vitest worker 回收，不影响结果
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { Terminal } from '@xterm/xterm'
import { useTerminalOutputStream, type OutputStreamFrame, type StreamSubscribeResult } from '@/composables/useTerminalOutputStream'
import { useSessionStore } from '@/stores/session'
import { useTerminalInputMarkers } from '@/composables/useTerminalInputMarkers'
import { makeServerStatusInfo, makeSessionInfo } from '@/__tests__/fixtures/index'

// ==================== mock Tauri invoke 边界 ====================

const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}))

// ==================== WebSocket 网络边界桩（同单测模式） ====================

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
}

function subscribeResponse(mode: 'incremental' | 'reset', minOffset = 0, maxOffset = 0) {
  return JSON.stringify({
    type: 'terminal',
    payload: {
      message_id: 'req-1',
      expect_response: false,
      timestamp: 1,
      session_id: 'session-1',
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

/** 等待异步连接流程 + xterm 解析 tick（真实 timers，xterm 需真实异步推进） */
async function flushAsync() {
  await new Promise((r) => setTimeout(r, 0))
  await new Promise((r) => setTimeout(r, 0))
  await new Promise((r) => setTimeout(r, 20))
}

// ==================== 测试基建 ====================

/** 会话 DB（可变） */
let sessionDb: ReturnType<typeof makeSessionInfo>[]

function installInvokeMock() {
  mockInvoke.mockImplementation((cmd: string, args?: any) => {
    switch (cmd) {
      case 'get_server_status':
        return Promise.resolve(makeServerStatusInfo({ status: 'running', port: 8765 }))
      case 'get_local_ws_token':
        return Promise.resolve('test-token-abc')
      case 'create_session_no_start':
        return Promise.resolve('session-1')
      case 'start_existing_session':
        return Promise.resolve(undefined)
      case 'kill_session':
        return Promise.resolve(undefined)
      case 'list_sessions':
        return Promise.resolve([...sessionDb])
      case 'write_to_session':
        return Promise.resolve(undefined)
      case 'resize_session':
        return Promise.resolve(undefined)
      default:
        return Promise.resolve(undefined)
    }
  })
}

function invokeCalls(cmd: string): unknown[][] {
  // 去掉调用数组首元素（命令名），只保留参数：与 toHaveBeenCalledWith 的参数形态一致
  return mockInvoke.mock.calls.filter(([c]) => c === cmd).map((call) => call.slice(1))
}

let term: Terminal | null = null
let host: HTMLDivElement | null = null

function createTerminal() {
  host = document.createElement('div')
  host.style.width = '800px'
  host.style.height = '400px'
  document.body.appendChild(host)
  term = new Terminal({ cols: 80, rows: 24 })
  term.open(host)
  return term
}

/** 读取 xterm buffer 指定行文本 */
function bufferLineText(term: Terminal, line: number): string {
  return term.buffer.active.getLine(line)?.translateToString().trim() ?? ''
}

const originalWebSocket = globalThis.WebSocket

beforeAll(() => {
  // composable 内部使用全局 WebSocket 构造本地环回连接：测试内无真实 WS 服务器，
  // 以可控桩替换（与 useTerminalOutputStream.test.ts 同模式，连接/帧注入测试驱动）
  ;(globalThis as Record<string, unknown>).WebSocket = MockWebSocket
})

afterAll(() => {
  ;(globalThis as Record<string, unknown>).WebSocket = originalWebSocket
})

beforeEach(() => {
  vi.clearAllMocks()
  setActivePinia(createPinia())
  sessionDb = []
  installInvokeMock()
  MockWebSocket.instances = []
})

afterEach(() => {
  term?.dispose()
  term = null
  host?.remove()
  host = null
})

// ==================== 场景 ====================

describe('终端流：xterm × useTerminalOutputStream × useSessionStore × useTerminalInputMarkers', () => {
  it('会话控制：创建 → 启动（两阶段）→ 终止，invoke 参数与 activeSession 状态联动', async () => {
    const sessionStore = useSessionStore()

    // 两阶段启动第一阶段：创建会话（不启动 PTY）；后端 DB 同步出现 starting 会话
    sessionDb = [makeSessionInfo({ id: 'session-1', status: 'starting' })]
    const sessionId = await sessionStore.createSession('config-1')
    expect(sessionId).toBe('session-1')
    expect(invokeCalls('create_session_no_start')).toEqual([[{ configId: 'config-1' }]])
    expect(sessionStore.sessions).toHaveLength(1)
    expect(sessionStore.sessions[0].status).toBe('starting')

    // 第二阶段：启动已创建会话 → 状态 running + activeSession 联动
    sessionDb = [makeSessionInfo({ id: 'session-1', status: 'running' })]
    await sessionStore.startSession('session-1')
    expect(invokeCalls('start_existing_session')).toEqual([[{ sessionId: 'session-1' }]])
    expect(sessionStore.activeSession?.id).toBe('session-1')
    expect(sessionStore.sessions[0].status).toBe('running')

    // 终止会话 → 列表清空 + activeSession 置空
    sessionDb = []
    await sessionStore.killSession('session-1')
    expect(invokeCalls('kill_session')).toEqual([[{ sessionId: 'session-1' }]])
    expect(sessionStore.sessions).toHaveLength(0)
    expect(sessionStore.activeSession).toBeNull()
  })

  it('输出流渲染：订阅消息参数构造 → 二进制帧写入 xterm buffer（含续传帧）', async () => {
    const term = createTerminal()
    const frames: OutputStreamFrame[] = []
    const stream = useTerminalOutputStream({
      onData: (frame) => {
        frames.push(frame)
        term.write(frame.data)
      },
      onReset: () => term.clear(),
    })

    stream.start('session-1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    // 本地环回端点 + 一次性令牌
    expect(ws.url).toBe('ws://127.0.0.1:8765/ws/terminal/local?token=test-token-abc')

    // 订阅消息参数构造：相邻标记结构 + 空 token（本地通道免 JWT）+ 首次全量（start_seq null）
    stream.subscribe()
    ws.open()
    const subscribe = JSON.parse(ws.sent[0])
    expect(subscribe.type).toBe('terminal')
    expect(subscribe.payload.session_id).toBe('session-1')
    expect(subscribe.payload.token).toBe('')
    expect(subscribe.payload.payload.action).toEqual({ type: 'subscribe', start_seq: null })

    // 服务端订阅确认（incremental 续传）
    ws.text(subscribeResponse('incremental'))

    // 首帧 [start=0, end=3) → 渲染到 buffer 第 0 行（\n 换行：xterm 中 \r 仅回车不换行）
    ws.binary([104, 105, 10], 0, 3) // "hi\n"
    await flushAsync()
    expect(frames).toHaveLength(1)
    expect(bufferLineText(term, 0)).toBe('hi')

    // 续传帧 [start=3, end=6) → 游标连续，渲染到第 1 行
    ws.binary([98, 121, 101], 3, 6) // "bye"
    await flushAsync()
    expect(frames).toHaveLength(2)
    expect(bufferLineText(term, 1)).toBe('bye')

    stream.stop()
  })

  it('连续性不变量：缺口帧触发保留游标重订阅（start_seq=游标，不置 null）', async () => {
    const term = createTerminal()
    const resets: StreamSubscribeResult[] = []
    const stream = useTerminalOutputStream({
      onData: ({ data }) => term.write(data),
      onReset: (r) => {
        resets.push(r)
        term.clear()
      },
    })

    stream.start('session-1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    ws.text(subscribeResponse('incremental'))
    // 推进游标到 6（首帧无换行 → 'bye' 写在同一行，末尾 \n 使游标到第 2 行行首）
    ws.binary([104, 105], 0, 2)
    ws.binary([98, 121, 101, 10], 2, 6)
    await flushAsync()
    expect(bufferLineText(term, 0)).toBe('hibye')

    // 缺口帧：start=99 ≠ 游标 6 → 连续性违反 → 重订阅
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
    ws.binary([120], 99, 100)
    await flushAsync()
    errorSpy.mockRestore()

    // 重订阅保留游标（增量语义）：不置 null 避免服务端裁决全量重播
    const ws2 = MockWebSocket.instances[1]
    expect(ws2).toBeTruthy()
    ws2.open()
    const resubscribe = JSON.parse(ws2.sent[0])
    expect(resubscribe.payload.payload.action).toEqual({ type: 'subscribe', start_seq: 6 })

    // 服务端从游标补缺口：incremental 确认 + 从 6 续传（游标在第 1 行行首）
    ws2.text(subscribeResponse('incremental', 6, 8))
    ws2.binary([121, 101, 115], 6, 9) // "yes"
    await flushAsync()
    expect(bufferLineText(term, 1)).toBe('yes')
    expect(resets).toHaveLength(0)

    stream.stop()
  })

  it('reset 裁决：onReset 清屏回调 → 回放帧从 minOffset 重渲染', async () => {
    const term = createTerminal()
    const resets: StreamSubscribeResult[] = []
    const truncated: number[] = []
    const stream = useTerminalOutputStream({
      onData: ({ data }) => term.write(data),
      onReset: (r) => {
        resets.push(r)
        term.clear()
      },
      onTruncated: (minOffset) => truncated.push(minOffset),
    })

    stream.start('session-1')
    await flushAsync()
    const ws = MockWebSocket.instances[0]
    stream.subscribe()
    ws.open()
    // 服务端裁决 reset：游标失效，清屏后全量重播
    ws.text(subscribeResponse('reset', 0, 10))
    ws.binary([104, 105, 13], 0, 3)
    await flushAsync()

    expect(resets).toHaveLength(1)
    expect(resets[0].mode).toBe('reset')
    expect(bufferLineText(term, 0)).toBe('hi')
    expect(truncated).toEqual([])

    stream.stop()
  })

  it('输入回传：write_to_session 参数构造 + 输入标记（xterm IMarker）联动', async () => {
    const term = createTerminal()
    const sessionStore = useSessionStore()
    const inputMarkers = useTerminalInputMarkers()
    sessionDb = [makeSessionInfo({ id: 'session-1', status: 'running' })]
    await sessionStore.loadSessions()

    // 模拟 TerminalPreview 的 onData 接线：单字符输入 + 回车提交记录标记
    let currentLine = ''
    const onData = (data: string) => {
      sessionStore.writeToSession('session-1', data)
      if (data === '\r' || data === '\n') {
        inputMarkers.record(term, currentLine)
        currentLine = ''
      } else if (data.length === 1 && data.charCodeAt(0) >= 32) {
        currentLine += data
      }
    }

    onData('e')
    onData('c')
    onData('h')
    onData('o')
    onData(' ')
    onData('h')
    onData('i')
    onData('\r')

    // 输入回传参数构造：逐次原样传递，无任何变换（xterm onData 语义）
    expect(invokeCalls('write_to_session')).toEqual([
      [{ sessionId: 'session-1', data: 'e' }],
      [{ sessionId: 'session-1', data: 'c' }],
      [{ sessionId: 'session-1', data: 'h' }],
      [{ sessionId: 'session-1', data: 'o' }],
      [{ sessionId: 'session-1', data: ' ' }],
      [{ sessionId: 'session-1', data: 'h' }],
      [{ sessionId: 'session-1', data: 'i' }],
      [{ sessionId: 'session-1', data: '\r' }],
    ])

    // 输入标记联动：回车提交 → 记录一行输入，marker 指向真实 xterm buffer 行
    expect(inputMarkers.visibleMarkers.value).toHaveLength(1)
    expect(inputMarkers.visibleMarkers.value[0].text).toBe('echo hi')
    expect(inputMarkers.visibleMarkers.value[0].line).toBe(0)

    // PTY 尺寸同步参数构造（TerminalPreview onResize 的 store 路径）
    await sessionStore.resizeSession('session-1', 80, 24)
    expect(invokeCalls('resize_session')).toEqual([[{ sessionId: 'session-1', cols: 80, rows: 24 }]])
  })
})
