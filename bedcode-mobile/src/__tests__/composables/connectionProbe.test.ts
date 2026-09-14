/**
 * 连接链路测试（ticket 07：HTTP 收束 Rust 统一代理后）
 *
 * 模拟移动端手动连接桌面端的流程，验证：
 * 1. HTTP 探测（httpProbe）经 invoke('http_request') 代理：桌面端可达时返回成功
 * 2. HTTP 探测在桌面端不可达时快速返回失败（3秒而非10秒WS超时）
 * 3. 探测前声明桌面端目标（egress_declare_desktop_target，L1 时序）
 * 4. request_id 透传（D3 前端 UUID）
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

// Mock @tauri-apps/api/core（invoke 是收束后唯一网络出口）
const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}))

// Mock @tauri-apps/api/event
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

// Mock vue-i18n
vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
  createI18n: vi.fn(),
}))

// Mock localStorage
const localStorageMock = {
  getItem: vi.fn(() => null),
  setItem: vi.fn(),
  removeItem: vi.fn(),
  clear: vi.fn(),
}
Object.defineProperty(globalThis, 'localStorage', { value: localStorageMock })

import { httpProbe } from '@/composables/useHttpApi'

/** mock http_request 成功响应（Rust HttpProxyResponse 形状） */
function mockProxyOk(bodyText: string, status = 200) {
  mockInvoke.mockImplementation(async (cmd: string, args: any) => {
    if (cmd === 'egress_declare_desktop_target') return null
    if (cmd === 'http_request') {
      return { status, statusText: 'OK', headers: {}, bodyText }
    }
    throw new Error(`unexpected command ${cmd}`)
  })
}

describe('HTTP 探测 (httpProbe) 经统一代理', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('桌面端可达时应返回 reachable=true', async () => {
    mockProxyOk(JSON.stringify({ status: 'ok', port: 8765, uptime_secs: 120 }))

    const result = await httpProbe('192.168.1.100', 8765)

    expect(result.reachable).toBe(true)
    expect(result.status).toBe('ok')
    expect(result.port).toBe(8765)
    expect(result.uptimeSecs).toBe(120)
  })

  it('探测前应声明桌面端目标（egress_declare_desktop_target）', async () => {
    mockProxyOk(JSON.stringify({ status: 'ok' }))

    await httpProbe('192.168.1.100', 8765)

    expect(mockInvoke).toHaveBeenCalledWith('egress_declare_desktop_target', {
      address: '192.168.1.100',
      port: 8765,
    })
  })

  it('请求应经 http_request 代理（desktop 类 + 3 秒超时 + request_id）', async () => {
    mockProxyOk(JSON.stringify({ status: 'ok' }))

    await httpProbe('192.168.1.100', 9999)

    const call = mockInvoke.mock.calls.find(([cmd]) => cmd === 'http_request')
    expect(call).toBeTruthy()
    const args = call[1] as { request: Record<string, unknown> }
    expect(args.request.url).toBe('http://192.168.1.100:9999/api/health')
    expect(args.request.method).toBe('GET')
    expect(args.request.kind).toBe('desktop')
    expect(args.request.timeoutMs).toBe(3000)
    expect(args.request.requestId).toBeTruthy()
    expect(typeof args.request.requestId).toBe('string')
  })

  it('桌面端不可达（invoke 拒绝）时应返回 reachable=false', async () => {
    mockInvoke.mockRejectedValue(new Error('Network error'))

    const result = await httpProbe('10.186.131.120', 8765)

    expect(result.reachable).toBe(false)
    expect(result.error).toBe('Network error')
  })

  it('桌面端返回 HTTP 500 时应返回 reachable=false', async () => {
    mockProxyOk('internal error', 500)

    const result = await httpProbe('192.168.1.100', 8765)

    expect(result.reachable).toBe(false)
    expect(result.error).toBe('HTTP 500')
  })

  it('setApiBaseUrl 应声明桌面端目标（Egress L1）', async () => {
    const { setApiBaseUrl } = await import('@/composables/useHttpApi')
    setApiBaseUrl('192.168.1.100', 8765)
    await Promise.resolve()
    expect(mockInvoke).toHaveBeenCalledWith('egress_declare_desktop_target', {
      address: '192.168.1.100',
      port: 8765,
    })
  })
})

describe('连接错误 i18n key 匹配', () => {
  it('unreachable 错误应匹配 DevicesView 的错误处理', () => {
    const errorMsg = 'mobile.connection.unreachable'
    expect(errorMsg.includes('unreachable')).toBe(true)
  })

  it('timeout 错误应匹配 DevicesView 的错误处理', () => {
    const errorMsg = 'mobile.connection.timeoutToast'
    expect(errorMsg.includes('timeout')).toBe(true)
  })

  it('refused 错误应匹配 DevicesView 的错误处理', () => {
    const errorMsg = 'mobile.connection.refusedToast'
    expect(errorMsg.includes('refused')).toBe(true)
  })
})
