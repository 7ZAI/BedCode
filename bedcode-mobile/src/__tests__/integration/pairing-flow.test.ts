/**
 * 配对流组合集成测试（L2 场景 2）
 *
 * 协作实体：真实 useMobileConnection（配对状态机） + useMobileCommands
 * （verifyPairingCode 凭据保存） + localStorage 持久化（auth_* 凭据 /
 * paired_devices 已配对设备）。
 *
 * 测试 seam：mock invoke + 脚本化事件（ws_pairing_request →
 * ws_pairing_verified → ws_paired 序列驱动状态机），composable 逻辑真实执行。
 *
 * 覆盖：配对全链路（配对码验证 → 凭据保存 → 事件驱动状态流转 → 已配对设备
 * 持久化）；配对码错误（invoke 返回 null 不保存凭据）；认证失败事件；
 * 错误事件。
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

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
  convertFileSrc: (p: string) => p,
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: any[]) => mockListen(...args),
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

/** 默认 invoke 分发（票 14 阶段 B：验证配对码走插件命令面 + 宿主窄读凭据） */
function installInvokeMock() {
  mockInvoke.mockImplementation((cmd: string, payload?: { command?: string }) => {
    switch (cmd) {
      case 'ws_connect':
        return Promise.resolve({ address: DEVICE.address, port: DEVICE.port, status: 'connected' })
      case 'plugin_invoke':
        // 配对编排域命令：verify-pairing-code 默认受理（凭据零过境，插件只拿 accepted）
        if (payload?.command === 'terminal-session.verify-pairing-code') {
          return Promise.resolve({ accepted: true })
        }
        return Promise.resolve(undefined)
      case 'ws_get_auth_credentials':
        return Promise.resolve(makeAuthCredentials({ fingerprint: 'fp-desktop-1', pairingId: 'pairing-1' }))
      case 'egress_declare_desktop_target':
        return Promise.resolve(null)
      case 'http_request':
        return Promise.resolve({
          status: 200,
          statusText: 'OK',
          headers: {},
          bodyText: JSON.stringify({ status: 'ok' }),
        })
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

describe('配对流：useMobileConnection 配对状态机 × 凭据持久化', () => {
  it('配对全链路：验证配对码 → 凭据保存 → ws_pairing_verified/ws_paired → 已配对设备持久化', async () => {
    // 连接建立
    await conn.connect(DEVICE)
    await flushAsync()

    // 桌面端请求配对：状态机进入 pairing
    await emit('ws_pairing_request')
    await flushAsync()
    expect(conn.connectionStatus.value).toBe('pairing')

    // 用户输入配对码 → 插件命令受理 → 宿主窄读凭据（含 JWT）
    const ok = await conn.verifyPairingCode('123456')
    expect(ok).toBe(true)
    const verifyCall = mockInvoke.mock.calls.find(
      ([cmd, payload]) => cmd === 'plugin_invoke' && (payload as { command?: string })?.command === 'terminal-session.verify-pairing-code',
    )
    expect(verifyCall).toBeDefined()
    expect((verifyCall?.[1] as { args: { code: string } }).args).toEqual({ code: '123456' })
    expect(invokeCalls('ws_get_auth_credentials')).toHaveLength(1)

    // 凭据保存：authCredentials + localStorage 三项
    expect(conn.authCredentials.value).toMatchObject({
      pairingId: 'pairing-1',
      fingerprint: 'fp-desktop-1',
      sessionToken: 'test-jwt-token',
    })
    expect(localStorage.getItem('auth_pairing_id')).toBe('pairing-1')
    expect(localStorage.getItem('auth_fingerprint')).toBe('fp-desktop-1')
    expect(localStorage.getItem('auth_session_token')).toBe('test-jwt-token')

    // 后端确认配对：事件序列驱动状态机
    await emit('ws_pairing_verified')
    await emit('ws_paired')
    await flushAsync()
    expect(conn.connectionStatus.value).toBe('paired')
    expect(conn.isPaired.value).toBe(true)
    expect(conn.isConnected.value).toBe(true)

    // 已配对设备记录 + localStorage 持久化（重启后列表恢复）
    expect(conn.pairedDevices.value).toHaveLength(1)
    expect(conn.pairedDevices.value[0]).toMatchObject({
      address: '192.168.1.100',
      port: 8765,
      name: 'DESKTOP-1',
      fingerprint: 'fp-desktop-1',
      connectCount: 1,
    })
    const stored = JSON.parse(localStorage.getItem('paired_devices') || '[]')
    expect(stored).toHaveLength(1)
    expect(stored[0].fingerprint).toBe('fp-desktop-1')
  })

  it('重复配对同一设备：指纹唯一，连接次数递增而非重复记录', async () => {
    await conn.connect(DEVICE)
    await flushAsync()

    // 两次配对（同一指纹）
    for (let i = 0; i < 2; i++) {
      const ok = await conn.verifyPairingCode('123456')
      expect(ok).toBe(true)
      await emit('ws_paired')
      await flushAsync()
    }

    expect(conn.pairedDevices.value).toHaveLength(1)
    expect(conn.pairedDevices.value[0].connectCount).toBe(2)
    const stored = JSON.parse(localStorage.getItem('paired_devices') || '[]')
    expect(stored).toHaveLength(1)
    expect(stored[0].connectCount).toBe(2)
  })

  it('配对码错误：插件返回 accepted=false → 不取凭据、不保存、返回 false、状态不进入 paired', async () => {
    // 配对码验证失败（桌面端业务拒绝，插件只拿到 accepted=false）
    mockInvoke.mockImplementation((cmd: string, payload?: { command?: string }) => {
      if (cmd === 'plugin_invoke' && payload?.command === 'terminal-session.verify-pairing-code') {
        return Promise.resolve({ accepted: false })
      }
      return Promise.resolve(undefined)
    })

    const ok = await conn.verifyPairingCode('000000')
    expect(ok).toBe(false)
    expect(invokeCalls('ws_get_auth_credentials')).toHaveLength(0)
    expect(conn.authCredentials.value).toBeNull()
    expect(localStorage.getItem('auth_session_token')).toBeNull()
    expect(conn.connectionStatus.value).toBe('disconnected')
    expect(conn.isPaired.value).toBe(false)
  })

  it('认证失败事件：ws_auth_failed → 状态 error + 原因透传', async () => {
    await emit('ws_auth_failed', { reason: 'Invalid pairing code' })
    await flushAsync()

    expect(conn.connectionStatus.value).toBe('error')
    expect(conn.connectionError.value).toBe('Invalid pairing code')
    // 认证失败不断开 isConnecting（startConnection 流程可能继续请求配对）
    expect(conn.isConnecting.value).toBe(false)
  })

  it('错误事件：ws_error → 状态 error + isConnecting 复位', async () => {
    await emit('ws_error', { message: 'Connection lost' })
    await flushAsync()

    expect(conn.connectionStatus.value).toBe('error')
    expect(conn.connectionError.value).toBe('Connection lost')
    expect(conn.isConnecting.value).toBe(false)
  })
})
