/**
 * 终端流组合集成测试（L2 场景 3）
 *
 * 协作实体：terminalBuffer store（ws_output 全局监听 + 字节游标） +
 * useTerminalBuffer（实时 handler 注册） + writeCoalescer（直写管线） +
 * xterm Terminal（stub，遵循 writeCoalescer.test.ts 的项目惯例） +
 * useMobileConnection.sendInput（HTTP 输入回传 + JWT 注入）。
 *
 * 测试 seam：mock invoke（ws_subscribe_session 裁决）+ 脚本化 ws_output
 * 事件（data_base64 载荷，Rust forward_event 形状）驱动 store 全局监听器。
 *
 * 覆盖：ws_output 事件 → base64 解码 → handler → terminal.write 全链路 +
 * 游标推进；订阅裁决 incremental → 确认前回放帧缓冲 → 排空写入；输入回传
 * （HTTP URL/body 构造 + Authorization JWT 注入协作）。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import type { Terminal } from '@xterm/xterm'
import type { RemoteDevice } from '@/composables/model'
import { useTerminalBufferStore } from '@/stores/terminalBuffer'
import { flushAsync, loadFreshModule, resetLocalStorage, clearEventHandlers } from './helpers'
import { makeOutputEvent, makeSubscribeResult, makeAuthCredentials } from '@/__tests__/fixtures/index'

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

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
  convertFileSrc: (p: string) => p,
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: any[]) => mockListen(...args),
}))
vi.mock('@tauri-apps/plugin-http', () => ({
  fetch: (...args: any[]) => mockFetch(...args),
}))
vi.mock('vue-sonner', () => ({
  toast: { error: vi.fn(), success: vi.fn(), warning: vi.fn(), info: vi.fn(), message: vi.fn() },
}))
vi.mock('@tauri-apps/plugin-os', () => ({}))

// ==================== 测试基建 ====================

const DEVICE: RemoteDevice = {
  id: 'dev-1',
  name: 'DESKTOP-1',
  address: '192.168.1.100',
  port: 8765,
  isPaired: false,
}

type ConnectionModule = typeof import('@/composables/useMobileConnection')

async function emit(name: string, payload?: unknown): Promise<void> {
  for (const handler of eventHandlers[name] || []) {
    await handler({ payload })
  }
}

function invokeCalls(cmd: string): unknown[][] {
  return mockInvoke.mock.calls.filter(([c]) => c === cmd).map((call) => call.slice(1))
}

/** stub xterm（项目惯例：writeCoalescer.test.ts 同款；writeBytes 守卫 element） */
function makeMockTerminal() {
  return {
    element: {},
    write: vi.fn(),
    clear: vi.fn(),
  } as unknown as Terminal
}

/** 默认 invoke 分发 */
function installInvokeMock() {
  mockInvoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case 'ws_subscribe_session':
        return Promise.resolve(makeSubscribeResult({ mode: 'incremental' }))
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
  await freshConnection()
})

afterEach(() => {
  for (const k of Object.keys(eventHandlers)) delete eventHandlers[k]
})

describe('终端流：terminalBuffer store × useTerminalBuffer × xterm × 输入回传', () => {
  it('ws_output 事件全链路：base64 解码 → 实时 handler → terminal.write + 游标推进', async () => {
    const store = useTerminalBufferStore()
    const terminal = makeMockTerminal()

    // 订阅前先注册实时 handler（页面进入会话的路径）
    store.ensureBuffer('s1')
    store.markSubscribed('s1')
    const { useTerminalBuffer } = await import('@/composables/useTerminalBuffer')
    const term = useTerminalBuffer()
    term.registerRealtimeHandler('s1', terminal)

    // 启动全局 ws_output 监听（连接建立时 onConnected 自动执行的真实路径）
    await store.startGlobalListener()
    await flushAsync()
    expect(eventHandlers['ws_output']).toBeDefined()

    // 后端推送输出（Rust forward_event 形状：data_base64 + 字节偏移）
    await emit('ws_output', makeOutputEvent('hello', { session_id: 's1', start_offset: 0, end_offset: 5 }))
    await flushAsync()

    // 直写管线（ENABLE_RAF_COALESCE=false）：事件回调内立即写入
    expect(terminal.write).toHaveBeenCalledWith(new Uint8Array([104, 101, 108, 108, 111]))
    // 字节游标推进（会话流坐标）
    expect(store.getBuffer('s1')!.cursor).toBe(5)
  })

  it('订阅裁决 incremental：确认前回放帧缓冲 → 确认后排空写入 terminal', async () => {
    const store = useTerminalBufferStore()
    const terminal = makeMockTerminal()
    store.ensureBuffer('s1')
    const { useTerminalBuffer } = await import('@/composables/useTerminalBuffer')
    const term = useTerminalBuffer()
    term.registerRealtimeHandler('s1', terminal)

    // 先显式启动全局监听器（doSubscribe 内部也会启动，但那是异步 await 链，
    // 不先启动则 emit 可能落在 listen 注册前——事件无监听者被丢弃的竞态）
    await store.startGlobalListener()
    await flushAsync()
    expect(eventHandlers['ws_output']).toBeDefined()

    // 发起订阅（invoke 在途，裁决未返回）
    const subscribePromise = store.subscribeSession('s1')

    // 裁决消息与历史帧乱序：确认前回放帧到达 → 缓冲不写入（订阅仍在途）
    await emit('ws_output', makeOutputEvent('hi', { session_id: 's1', start_offset: 0, end_offset: 2 }))
    expect(terminal.write).not.toHaveBeenCalled()

    // 订阅确认（incremental，游标 0 在保留区间内）→ 排空缓冲写入
    await flushAsync()
    const result = await subscribePromise
    await flushAsync()
    expect(result?.mode).toBe('incremental')
    expect(store.getBuffer('s1')!.subscribed).toBe(true)
    expect(terminal.write).toHaveBeenCalledWith(new Uint8Array([104, 105]))
    expect(store.getBuffer('s1')!.cursor).toBe(2)
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
