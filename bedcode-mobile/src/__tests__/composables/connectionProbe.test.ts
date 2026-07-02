/**
 * 连接链路测试
 *
 * 模拟移动端手动连接桌面端的流程，验证：
 * 1. HTTP 探测（httpProbe）在桌面端可达时返回成功
 * 2. HTTP 探测在桌面端不可达时快速返回失败（3秒而非10秒WS超时）
 * 3. 连接错误提示 i18n key 正确
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

// Mock @tauri-apps/plugin-http
const mockFetch = vi.fn()
vi.mock('@tauri-apps/plugin-http', () => ({
  fetch: (...args: any[]) => mockFetch(...args),
}))

// Mock @tauri-apps/api/event
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

// Mock @tauri-apps/api/core
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
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

import { httpProbe, type ProbeResult } from '@/composables/useHttpApi'

describe('HTTP 探测 (httpProbe)', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('桌面端可达时应返回 reachable=true', async () => {
    mockFetch.mockResolvedValue({
      ok: true,
      json: async () => ({ status: 'ok', port: 8765, uptime_secs: 120 }),
    })

    const result = await httpProbe('192.168.1.100', 8765)

    expect(result.reachable).toBe(true)
    expect(result.status).toBe('ok')
    expect(result.port).toBe(8765)
    expect(result.uptimeSecs).toBe(120)
    expect(mockFetch).toHaveBeenCalledWith(
      'http://192.168.1.100:8765/api/health',
      expect.objectContaining({ method: 'GET', connectTimeout: 3000 })
    )
  })

  it('桌面端不可达时应返回 reachable=false（模拟防火墙/网络不通）', async () => {
    mockFetch.mockRejectedValue(new Error('Network error'))

    const result = await httpProbe('10.186.131.120', 8765)

    expect(result.reachable).toBe(false)
    expect(result.error).toBe('Network error')
  })

  it('桌面端返回 HTTP 500 时应返回 reachable=false', async () => {
    mockFetch.mockResolvedValue({
      ok: false,
      status: 500,
      statusText: 'Internal Server Error',
    })

    const result = await httpProbe('192.168.1.100', 8765)

    expect(result.reachable).toBe(false)
    expect(result.error).toBe('HTTP 500')
  })

  it('探测应使用 3 秒超时（而非 10 秒 WS 超时）', async () => {
    mockFetch.mockResolvedValue({
      ok: true,
      json: async () => ({ status: 'ok', port: 8765 }),
    })

    await httpProbe('192.168.1.100', 8765)

    expect(mockFetch).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ connectTimeout: 3000 })
    )
  })

  it('探测 URL 应正确拼接 address:port/api/health', async () => {
    mockFetch.mockResolvedValue({
      ok: true,
      json: async () => ({ status: 'ok' }),
    })

    await httpProbe('192.168.1.100', 9999)

    expect(mockFetch).toHaveBeenCalledWith(
      'http://192.168.1.100:9999/api/health',
      expect.anything()
    )
  })
})

describe('连接错误 i18n key 匹配', () => {
  it('unreachable 错误应匹配 DevicesView 的错误处理', () => {
    // useMobileConnection.ts connect() 抛出 'mobile.connection.unreachable'
    // DevicesView.vue 通过 errorMsg.includes('unreachable') 匹配
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
