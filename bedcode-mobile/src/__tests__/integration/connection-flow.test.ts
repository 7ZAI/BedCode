/**
 * 连接流组合集成测试（L2 场景 1）
 *
 * 协作实体：真实 useMobileConnection（模块级单例，事件驱动状态机） +
 * useHttpApi（HTTP 探测/API 基址） + useTerminalBufferStore（订阅联动） +
 * useForegroundService / useNotification（invoke 包装）。
 *
 * 测试 seam（与桌面端 integration 同模式）：
 * - 只 mock @tauri-apps/api 边界：core.invoke + event.listen（按事件名捕获
 *   回调，测试内手动触发模拟后端事件推送）+ plugin-http.fetch（HTTP API）
 * - composables / store 内部逻辑全部真实执行，fixture 数据取自工厂
 * - 模块级单例经 loadFreshModule 每次用例重新加载（resetModules 清除
 *   init() 的 initialized 标志与全部模块级 ref）
 *
 * 覆盖：连接成功全链路（探测 → WS 连接 → 事件驱动状态流转 → 凭据恢复 →
 * 已配对设备持久化）；探测不可达；12s 连接超时（fake timers 压缩）；
 * 取消连接；意外断开自动重连（ws_reconnect → ws_reconnected → JWT 认证 →
 * ws_paired 闭环）；重连次数耗尽放弃；服务端关闭。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import type { RemoteDevice } from '@/composables/model'
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

/** 触发后端事件推送（listen 捕获的回调，payload 包装为 Tauri 事件形状） */
async function emit(name: string, payload?: unknown): Promise<void> {
  for (const handler of eventHandlers[name] || []) {
    await handler({ payload })
  }
}

function invokeCalls(cmd: string): unknown[][] {
  return mockInvoke.mock.calls.filter(([c]) => c === cmd).map((call) => call.slice(1))
}

/** 默认 invoke 分发：连接流程涉及的命令返回安全值 */
function installInvokeMock() {
  mockInvoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case 'ws_connect':
        return Promise.resolve({ address: DEVICE.address, port: DEVICE.port, status: 'connected' })
      case 'ws_is_connected':
        return Promise.resolve(false)
      case 'ws_authenticate':
        return Promise.resolve(true)
      case 'ws_verify_pairing_code':
        return Promise.resolve(makeAuthCredentials())
      default:
        return Promise.resolve(undefined)
    }
  })
}

/** HTTP 探测可达 */
function mockDesktopReachable(): void {
  mockFetch.mockResolvedValue({
    ok: true,
    json: async () => ({ status: 'ok', port: 8765, uptime_secs: 120 }),
  })
}

let conn: ReturnType<ConnectionModule['useMobileConnection']>

/**
 * 重新加载模块（fresh 单例）：清空事件捕获（旧模块 handler 不会自动 unlisten）+
 * 清空 localStorage + 可选预置（凭据/设置需在 init() 读取前写入）
 */
async function freshConnection(preset?: () => void): Promise<void> {
  clearEventHandlers(eventHandlers)
  resetLocalStorage()
  preset?.()
  const mod = await loadFreshModule<ConnectionModule>('@/composables/useMobileConnection')
  conn = mod.useMobileConnection()
  await flushAsync() // 等待模块级 init()（22+ 个 listen await）完成
}

beforeEach(async () => {
  vi.clearAllMocks()
  installInvokeMock()
  setActivePinia(createPinia())
  await freshConnection()
})

afterEach(() => {
  for (const k of Object.keys(eventHandlers)) delete eventHandlers[k]
  vi.useRealTimers()
})

describe('连接流：useMobileConnection × useHttpApi × terminalBuffer store', () => {
  it('连接成功全链路：探测 → WS 连接 → 事件驱动状态流转 → 凭据恢复 → 已配对设备持久化', async () => {
    // 预置已保存凭据：init() 应恢复 authCredentials 并调用 ws_set_token 恢复
    // Rust 侧全局 token（JWT 重连响应不触发 AuthHandler 补写的兜底路径）
    const creds = makeAuthCredentials({ fingerprint: 'fp-desktop-1' })
    localStorage.setItem('auth_pairing_id', creds.pairingId)
    localStorage.setItem('auth_fingerprint', creds.fingerprint)
    localStorage.setItem('auth_session_token', creds.sessionToken)

    // 重新加载模块使 init() 读到预置凭据
    await freshConnection(() => {
      localStorage.setItem('auth_pairing_id', creds.pairingId)
      localStorage.setItem('auth_fingerprint', creds.fingerprint)
      localStorage.setItem('auth_session_token', creds.sessionToken)
    })

    // 凭据恢复：authCredentials 加载 + Rust 侧全局 token 恢复（invoke 参数为 { token }）
    expect(conn.authCredentials.value?.sessionToken).toBe('test-jwt-token')
    expect(invokeCalls('ws_set_token')).toEqual([[{ token: 'test-jwt-token' }]])

    mockDesktopReachable()
    await conn.connect(DEVICE)
    await flushAsync()

    // HTTP 探测 URL 构造 + ws_connect 参数
    expect(mockFetch).toHaveBeenCalledWith(
      'http://192.168.1.100:8765/api/health',
      expect.objectContaining({ method: 'GET', connectTimeout: 3000 }),
    )
    expect(invokeCalls('ws_connect')).toEqual([[
      { address: DEVICE.address, port: DEVICE.port, name: DEVICE.name },
    ]])

    // 事件驱动状态机：connecting → connected
    await emit('ws_connecting')
    expect(conn.connectionStatus.value).toBe('connecting')
    await emit('ws_connected')
    await flushAsync()
    expect(conn.connectionStatus.value).toBe('connected')
    expect(conn.isConnected.value).toBe(true)
    // 连接建立 → 全局输出监听器启动（listen('ws_output') 被捕获）
    expect(eventHandlers['ws_output']).toBeDefined()

    // paired：已配对设备持久化（onPaired 需要 authCredentials + currentDevice 齐备）
    await emit('ws_paired')
    await flushAsync()
    expect(conn.isPaired.value).toBe(true)
    expect(conn.pairedDevices.value).toHaveLength(1)
    expect(conn.pairedDevices.value[0]).toMatchObject({
      address: '192.168.1.100',
      port: 8765,
      name: 'DESKTOP-1',
      fingerprint: 'fp-desktop-1',
      connectCount: 1,
    })
    // localStorage 持久化（重启后列表恢复）
    const stored = JSON.parse(localStorage.getItem('paired_devices') || '[]')
    expect(stored).toHaveLength(1)
    expect(stored[0].fingerprint).toBe('fp-desktop-1')
  })

  it('HTTP 探测不可达：快速失败（不调 ws_connect）+ 状态 error', async () => {
    mockFetch.mockRejectedValue(new Error('Network error'))

    await expect(conn.connect(DEVICE)).rejects.toThrow('mobile.connection.unreachable')
    await flushAsync()

    expect(conn.connectionStatus.value).toBe('error')
    expect(conn.connectionError.value).toBe('mobile.connection.unreachable')
    expect(conn.isConnecting.value).toBe(false)
    expect(invokeCalls('ws_connect')).toHaveLength(0)
  })

  it('连接超时：12 秒未收到 ws_connected → timeoutToast + 状态 error', async () => {
    vi.useFakeTimers()
    mockDesktopReachable()

    const p = conn.connect(DEVICE)
    await vi.advanceTimersByTimeAsync(0)
    await p.catch(() => {})
    await flushAsync()

    // 连接中状态确认（防恒真：connecting 由事件驱动，先触发 ws_connecting 再正向断言）
    await emit('ws_connecting')
    expect(conn.connectionStatus.value).toBe('connecting')
    expect(conn.isConnecting.value).toBe(true)

    // 12 秒后超时（Rust 端 10s WS 超时 + 前端 12s 兜底）
    await vi.advanceTimersByTimeAsync(12000)
    await flushAsync()

    expect(conn.connectionStatus.value).toBe('error')
    expect(conn.connectionError.value).toBe('mobile.connection.timeoutToast')
    expect(conn.isConnecting.value).toBe(false)
  })

  it('取消连接：cancelConnection → 状态 disconnected + ws_disconnect', async () => {
    mockDesktopReachable()
    const p = conn.connect(DEVICE)
    await flushAsync()

    await conn.cancelConnection()
    await flushAsync()

    expect(conn.connectionStatus.value).toBe('disconnected')
    expect(conn.isConnecting.value).toBe(false)
    expect(conn.connectionError.value).toBe('mobile.connection.userCancelled')
    expect(invokeCalls('ws_disconnect')).toHaveLength(1)
    await p.catch(() => {})
  })

  it('意外断开自动重连闭环：ws_unexpected_disconnect → ws_reconnect → ws_reconnected → JWT 认证 → ws_paired', async () => {
    // 预置重连设置（间隔 0 立即重试）与凭据
    await freshConnection(() => {
      localStorage.setItem('mobile-settings', JSON.stringify({ autoReconnect: true, reconnectInterval: 0 }))
      const creds = makeAuthCredentials()
      localStorage.setItem('auth_pairing_id', creds.pairingId)
      localStorage.setItem('auth_fingerprint', creds.fingerprint)
      localStorage.setItem('auth_session_token', creds.sessionToken)
    })

    // 连接建立：onConnected 复位 aborted（本场景验证的修复点——
    // connect() 置位的取消标记在连接成功后必须复位，否则重连不可达）
    mockDesktopReachable()
    await conn.connect(DEVICE)
    await flushAsync()
    await emit('ws_connected')
    await flushAsync()
    expect(conn.connectionStatus.value).toBe('connected')

    // 意外断开 → 前端自动重连启动（携带已存 token）
    await emit('ws_unexpected_disconnect', { reason: 'Connection reset' })
    await flushAsync()
    expect(invokeCalls('ws_reconnect')).toEqual([[{ sessionToken: 'test-jwt-token' }]])
    expect(conn.connectionStatus.value).toBe('connecting')
    expect(conn.isConnecting.value).toBe(true)

    // Rust 端重连成功 → JWT 认证 → ws_paired 事件闭环
    await emit('ws_reconnected')
    await flushAsync()
    expect(invokeCalls('ws_authenticate')).toEqual([[{ sessionToken: 'test-jwt-token' }]])
    expect(conn.connectionStatus.value).toBe('connected')

    await emit('ws_paired')
    await flushAsync()
    expect(conn.isPaired.value).toBe(true)
  })

  it('连接中意外断开（ws_connected 未到达）：不触发自动重连（aborted 尚未复位）', async () => {
    // 修复边界验证：connect() 置位后、onConnected 复位前的窗口期内意外断开
    // 不重连——取消标记语义在此窗口内仍然生效（连接未成功建立，重连无意义）
    await freshConnection(() => {
      localStorage.setItem('mobile-settings', JSON.stringify({ autoReconnect: true, reconnectInterval: 0 }))
      const creds = makeAuthCredentials()
      localStorage.setItem('auth_pairing_id', creds.pairingId)
      localStorage.setItem('auth_fingerprint', creds.fingerprint)
      localStorage.setItem('auth_session_token', creds.sessionToken)
    })

    mockDesktopReachable()
    const p = conn.connect(DEVICE)
    await flushAsync()

    // ws_connected 尚未触发 → aborted 仍为 true
    await emit('ws_unexpected_disconnect', { reason: 'handshake lost' })
    await flushAsync()
    expect(invokeCalls('ws_reconnect')).toHaveLength(0)
    expect(conn.connectionStatus.value).toBe('disconnected')
    await p.catch(() => {})
  })

  it('Rust 端重连成功事件链：ws_reconnecting → ws_reconnected → JWT 认证 → ws_paired', async () => {
    // 预置凭据（Rust 端重连成功后的重新认证依赖已存 JWT）
    await freshConnection(() => {
      const creds = makeAuthCredentials()
      localStorage.setItem('auth_pairing_id', creds.pairingId)
      localStorage.setItem('auth_fingerprint', creds.fingerprint)
      localStorage.setItem('auth_session_token', creds.sessionToken)
    })

    // 重连开始（Rust 端自动重连）：状态 connecting + 前台通知更新
    await emit('ws_reconnecting', { retry: 1, max_retry: 3 })
    await flushAsync()
    expect(conn.connectionStatus.value).toBe('connecting')

    // 重连成功 → 自动 JWT 重新认证（isConnecting 保持 true 直到认证完成）
    await emit('ws_reconnected')
    await flushAsync()
    expect(invokeCalls('ws_authenticate')).toEqual([[{ sessionToken: 'test-jwt-token' }]])
    expect(conn.connectionStatus.value).toBe('connected')
    expect(conn.isConnecting.value).toBe(true)

    // 认证成功 → ws_paired → 状态 paired（重连闭环完成）
    await emit('ws_paired')
    await flushAsync()
    expect(conn.isPaired.value).toBe(true)
  })

  it('重连失败事件：ws_reconnect_failed → 状态 disconnected + 重连计数耗尽标记', async () => {
    // 预置凭据与重连设置（重连失败事件的前端处理不依赖 aborted 状态）
    await freshConnection(() => {
      const creds = makeAuthCredentials()
      localStorage.setItem('auth_pairing_id', creds.pairingId)
      localStorage.setItem('auth_fingerprint', creds.fingerprint)
      localStorage.setItem('auth_session_token', creds.sessionToken)
    })

    await emit('ws_reconnect_failed', { reason: 'Connection refused' })
    await flushAsync()

    expect(conn.connectionStatus.value).toBe('disconnected')
    expect(conn.isConnecting.value).toBe(false)
    expect(conn.connectionError.value).toBe('common.notification.connectionDisconnected')
    // 后续再触发意外断开：attempt 已耗尽（MAX_AUTO_RECONNECT_ATTEMPTS），不进入重连
    await emit('ws_unexpected_disconnect', { reason: 'again' })
    await flushAsync()
    expect(invokeCalls('ws_reconnect')).toHaveLength(0)
  })

  it('服务端关闭：ws_server_closed → 状态 disconnected + 错误原因透传', async () => {
    await emit('ws_server_closed', { reason: 'Server shutdown' })
    await flushAsync()

    expect(conn.connectionStatus.value).toBe('disconnected')
    expect(conn.connectionError.value).toBe('Server shutdown')
    expect(conn.isConnecting.value).toBe(false)
  })
})
