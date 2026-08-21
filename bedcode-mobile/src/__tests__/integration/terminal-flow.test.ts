/**
 * 终端流组合集成测试（L2 场景 3，10 号票重写）
 *
 * 协作实体：terminalBuffer store（终端 WS 状态机 + lastRenderedSeq） +
 * useTerminalBuffer（实时 handler 注册） + writeCoalescer（直写管线） +
 * xterm Terminal（stub，遵循 writeCoalescer.test.ts 的项目惯例） +
 * useMobileConnection.sendInput（HTTP 输入回传 + JWT 注入）。
 *
 * 测试 seam：mock createTerminalSocket（fake socket 捕获 handlers 回调）+
 * 手动驱动 onSubscribed/onFrame/onHistoryEnd，模拟桌面端新路由帧流。
 *
 * 覆盖：TB v2 帧 → handler → terminal.write 全链路 + lastRenderedSeq 推进；
 * 历史段实时帧缓冲 → history_end FLUSH；输入回传（HTTP URL/body 构造 +
 * Authorization JWT 注入协作）。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import type { Terminal } from '@xterm/xterm'
import type { RemoteDevice } from '@/composables/model'
import { useTerminalBufferStore } from '@/stores/terminalBuffer'
import { flushAsync, loadFreshModule, resetLocalStorage, clearEventHandlers } from './helpers'
import { makeAuthCredentials } from '@/__tests__/fixtures/index'

// ==================== mock Tauri 边界 ====================

const mockInvoke = vi.fn()
const eventHandlers: Record<string, Array<(payload: unknown) => void>> = {}
const mockListen = vi.fn((event: string, handler: (payload: unknown) => void) => {
  if (!eventHandlers[event]) eventHandlers[event] = []
  eventHandlers[event].push(handler)
  return Promise.resolve(() => {
    eventHandlers[event] = (eventHandlers[event] || []).filter((h) => h !== handler)
  })
})
const mockFetch = vi.fn()

// fake 终端 socket：捕获 handlers，测试手动驱动
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

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
  convertFileSrc: (p: string) => p,
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: any[]) => mockListen(...args),
  emit: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('@tauri-apps/plugin-http', () => ({
  fetch: (...args: any[]) => mockFetch(...args),
}))
vi.mock('vue-sonner', () => ({
  toast: { error: vi.fn(), success: vi.fn(), warning: vi.fn(), info: vi.fn(), message: vi.fn() },
}))
vi.mock('@tauri-apps/plugin-os', () => ({}))
vi.mock('@/composables/useTerminalSocket', () => ({
  createTerminalSocket: (...args: any[]) => createTerminalSocketMock(...args),
}))
vi.mock('@/composables/useMobileCommands', async (importOriginal) => {
  const mod = await importOriginal<typeof import('@/composables/useMobileCommands')>()
  return {
    ...mod,
    getTerminalWsInfo: vi.fn().mockResolvedValue({ url: 'ws://192.168.1.100:8765/ws/terminal/session/s1', token: 'jwt' }),
  }
})

// ==================== 测试基建 ====================

const DEVICE: RemoteDevice = {
  id: 'dev-1',
  name: 'DESKTOP-1',
  address: '192.168.1.100',
  port: 8765,
  isPaired: false,
}

type ConnectionModule = typeof import('@/composables/useMobileConnection')

/** stub xterm（项目惯例：writeCoalescer.test.ts 同款；writeBytes 守卫 element） */
function makeMockTerminal() {
  return {
    element: {},
    write: vi.fn(),
    clear: vi.fn(),
    // 渲染背压接线（registerRealtimeHandler 挂 onWriteParsed）
    onWriteParsed: vi.fn(() => ({ dispose: vi.fn() })),
  } as unknown as Terminal
}

/** 构造 TB v2 帧对象（store 消费的解析结果） */
function frame(seq: number, eventCount = 1, data?: string) {
  return {
    data: new TextEncoder().encode(data ?? `data-${seq}`),
    seq,
    eventCount,
    lastSeq: seq + eventCount - 1,
    isWaiting: false,
  }
}

function setupSocket() {
  fakeSocket = {
    start: vi.fn(),
    subscribe: vi.fn(),
    sendInput: vi.fn(),
    stop: vi.fn(),
    reconnect: vi.fn(),
    isOpen: vi.fn(() => false),
    ackRendered: vi.fn(),
  }
  capturedHandlers = null
  createTerminalSocketMock.mockImplementation((handlers: unknown) => {
    capturedHandlers = handlers as Record<string, any>
    return fakeSocket
  })
  return fakeSocket
}

function installInvokeMock() {
  mockInvoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case 'ws_connect':
        return Promise.resolve({ address: DEVICE.address, port: DEVICE.port, status: 'connected' })
      default:
        return Promise.resolve(undefined)
    }
  })
}

let conn: ReturnType<ConnectionModule['useMobileConnection']>

async function freshConnection(preset?: () => void): Promise<void> {
  clearEventHandlers(eventHandlers)
  resetLocalStorage()
  preset?.()
  const mod = await loadFreshModule<ConnectionModule>('@/composables/useMobileConnection')
  conn = mod.useMobileConnection()
  await flushAsync()
}

beforeEach(async () => {
  vi.clearAllMocks()
  installInvokeMock()
  setActivePinia(createPinia())
  setupSocket()
  await freshConnection()
})

afterEach(() => {
  for (const k of Object.keys(eventHandlers)) delete eventHandlers[k]
})

describe('终端流：terminalBuffer store × useTerminalBuffer × xterm × 输入回传', () => {
  it('socket 帧全链路：TB v2 帧 → 实时 handler → terminal.write + lastRenderedSeq 推进', async () => {
    const store = useTerminalBufferStore()
    const terminal = makeMockTerminal()
    const { useTerminalBuffer } = await import('@/composables/useTerminalBuffer')
    const term = useTerminalBuffer()
    term.registerRealtimeHandler('s1', terminal)

    // 订阅：fake socket 建连 + 订阅帧
    await store.subscribeSession('s1')
    expect(fakeSocket.start).toHaveBeenCalledWith('s1')

    // 桌面端帧流：subscribe_ok → 历史帧 → history_end → 实时帧
    capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
    capturedHandlers!.onFrame(frame(1, 1, 'hello'))
    capturedHandlers!.onHistoryEnd(10)
    await flushAsync()

    // 直写管线（ENABLE_RAF_COALESCE=false）：事件回调内立即写入
    expect(terminal.write).toHaveBeenCalledWith(new TextEncoder().encode('hello'))
    // lastRenderedSeq 推进
    expect(store.getBuffer('s1')!.lastRenderedSeq).toBe(1)
  })

  it('历史段实时帧缓冲 → history_end FLUSH 写入 terminal（快照拼接无缝隙）', async () => {
    const store = useTerminalBufferStore()
    const terminal = makeMockTerminal()
    const { useTerminalBuffer } = await import('@/composables/useTerminalBuffer')
    const term = useTerminalBuffer()
    term.registerRealtimeHandler('s1', terminal)

    await store.subscribeSession('s1')
    capturedHandlers!.onSubscribed({ snapshotSeq: 10, minSeq: 0, historyCount: 5 })
    capturedHandlers!.onFrame(frame(1, 10, 'his')) // 历史帧覆盖到 snapshot 边界（1-10）
    // 历史段内实时帧（> snapshot_seq）：缓冲不写入
    capturedHandlers!.onFrame(frame(11, 1, 'live'))
    expect(terminal.write).toHaveBeenCalledTimes(1)

    // history_end：FLUSH 实时缓冲
    capturedHandlers!.onHistoryEnd(10)
    await flushAsync()
    expect(terminal.write).toHaveBeenCalledWith(new TextEncoder().encode('live'))
    expect(store.getBuffer('s1')!.lastRenderedSeq).toBe(11)
  })

  it('输入回传：sendInput → HTTP POST 参数构造 + JWT 注入协作', async () => {
    // 预置凭据：useHttpApi 的 request() 应为非 auth 路径注入 Authorization
    await freshConnection(() => {
      const creds = makeAuthCredentials()
      localStorage.setItem('auth_pairing_id', creds.pairingId)
      localStorage.setItem('auth_fingerprint', creds.fingerprint)
      localStorage.setItem('auth_session_token', creds.sessionToken)
    })

    // 连接建立（setApiBaseUrl 设置 HTTP 基址）；fetch 按路径分发：
    // /api/health 为探测响应形状（status/port），其余为 HTTP API 响应形状（code/message）
    mockFetch.mockImplementation((url: string) => {
      if (url.endsWith('/api/health')) {
        return Promise.resolve({ ok: true, json: async () => ({ status: 'ok', port: 8765, uptime_secs: 120 }) })
      }
      return Promise.resolve({ ok: true, json: async () => ({ code: 0, message: 'ok' }) })
    })
    await conn.connect(DEVICE)
    await flushAsync()

    // 发送输入 → HTTP POST /api/sessions/s1/input
    await conn.sendInput('s1', 'ls -la\n')
    await flushAsync()

    const lastCall = mockFetch.mock.calls[mockFetch.mock.calls.length - 1]
    expect(lastCall[0]).toBe('http://192.168.1.100:8765/api/sessions/s1/input')
    expect(lastCall[1]).toMatchObject({
      method: 'POST',
      body: JSON.stringify({ data: 'ls -la\n', specialKey: null }),
      headers: { Authorization: 'Bearer test-jwt-token' },
    })
    expect(invokeCalls('ws_send_input_async')).toHaveLength(0)
  })
})

function invokeCalls(cmd: string): unknown[][] {
  return mockInvoke.mock.calls.filter(([c]) => c === cmd).map((call) => call.slice(1))
}
