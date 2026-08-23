/**
 * usePeerConsent 测试（issue 04）
 *
 * 覆盖编排逻辑（不测渲染）：迁移规则匹配、事件入队/弹窗展示、接受/拒绝
 * 结算与队列推进、终端配对自动互信（不弹窗直接放行）、超时自动拒绝。
 * listen 经 mock 捕获处理器后手动派发事件载荷驱动。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import {
  matchesTerminalPairedDevice,
  usePeerConsent,
  _resetPeerConsentForTest,
  type PeerConsentRequest,
} from '@/composables/usePeerConsent'

// Mock Tauri invoke / event
const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}))

type ConsentHandler = (event: { payload: PeerConsentRequest }) => void
let capturedConsentHandler: ConsentHandler | null = null

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (_event: string, handler: ConsentHandler) => {
    capturedConsentHandler = handler
    return () => {}
  }),
}))

function makeRequest(overrides: Partial<PeerConsentRequest> = {}): PeerConsentRequest {
  return {
    requestId: 'req-1',
    nodeId: 'aa'.repeat(32),
    fingerprintShort: 'aaaaaaaa',
    deviceName: '张三的手机',
    ...overrides,
  }
}

/** 经捕获的 listen 处理器派发一条 consent 请求 */
async function emitRequest(payload: PeerConsentRequest) {
  await capturedConsentHandler!({ payload })
}

describe('matchesTerminalPairedDevice', () => {
  it('matches by exact device name only', () => {
    expect(matchesTerminalPairedDevice('张三的手机', ['张三的手机'])).toBe(true)
    expect(matchesTerminalPairedDevice('李四的电脑', ['张三的手机'])).toBe(false)
  })

  it('never matches unnamed requests (fail-safe to manual confirm)', () => {
    expect(matchesTerminalPairedDevice(null, ['张三的手机'])).toBe(false)
    expect(matchesTerminalPairedDevice('', ['张三的手机'])).toBe(false)
  })
})

describe('usePeerConsent flow', () => {
  const consent = usePeerConsent()

  beforeEach(() => {
    vi.clearAllMocks()
    _resetPeerConsentForTest()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('shows dialog for unknown device and settles on accept', async () => {
    mockInvoke.mockResolvedValue(true)
    await consent.start(async () => [])
    await emitRequest(makeRequest())

    expect(consent.currentRequest.value?.requestId).toBe('req-1')

    await consent.accept()

    expect(mockInvoke).toHaveBeenCalledWith('respond_peer_consent', {
      requestId: 'req-1',
      accepted: true,
    })
    expect(consent.currentRequest.value).toBeNull()
  })

  it('deny closes dialog and responds false', async () => {
    mockInvoke.mockResolvedValue(true)
    await consent.start(async () => [])
    await emitRequest(makeRequest({ requestId: 'req-2' }))

    await consent.deny()

    expect(mockInvoke).toHaveBeenCalledWith('respond_peer_consent', {
      requestId: 'req-2',
      accepted: false,
    })
    expect(consent.currentRequest.value).toBeNull()
  })

  it('queues concurrent requests and shows them one at a time', async () => {
    mockInvoke.mockResolvedValue(true)
    await consent.start(async () => [])
    await emitRequest(makeRequest({ requestId: 'req-a' }))
    await emitRequest(makeRequest({ requestId: 'req-b' }))

    // 第二条排队，弹窗仍显示第一条
    expect(consent.currentRequest.value?.requestId).toBe('req-a')

    await consent.accept()
    // 第一条结算后第二条顶上
    expect(consent.currentRequest.value?.requestId).toBe('req-b')

    await consent.deny()
    expect(consent.currentRequest.value).toBeNull()
    expect(mockInvoke).toHaveBeenCalledTimes(2)
  })

  it('auto-trusts terminal-paired device by name without showing dialog', async () => {
    mockInvoke.mockResolvedValue(true)
    await consent.start(async () => ['张三的手机'])
    await emitRequest(makeRequest({ requestId: 'req-paired' }))

    // 不弹窗：currentRequest 保持空；回执已按接受发出
    expect(consent.currentRequest.value).toBeNull()
    expect(consent.autoTrustedName.value).toBe('张三的手机')
    expect(mockInvoke).toHaveBeenCalledWith('respond_peer_consent', {
      requestId: 'req-paired',
      accepted: true,
    })
  })

  it('falls back to manual dialog when paired names cannot be loaded', async () => {
    mockInvoke.mockResolvedValue(true)
    await consent.start(async () => {
      throw new Error('store not ready')
    })
    await emitRequest(makeRequest({ requestId: 'req-fallback' }))

    expect(consent.currentRequest.value?.requestId).toBe('req-fallback')
  })

  it('auto-denies after 30s without user action', async () => {
    vi.useFakeTimers()
    mockInvoke.mockResolvedValue(true)
    await consent.start(async () => [])
    await emitRequest(makeRequest({ requestId: 'req-slow' }))

    expect(consent.currentRequest.value?.requestId).toBe('req-slow')

    vi.advanceTimersByTime(30_000)

    expect(mockInvoke).toHaveBeenCalledWith('respond_peer_consent', {
      requestId: 'req-slow',
      accepted: false,
    })
    expect(consent.currentRequest.value).toBeNull()
  })

  it('auto-trusts queued paired devices while dialog is open for another peer', async () => {
    mockInvoke.mockResolvedValue(true)
    let names = ['张三的手机']
    await consent.start(async () => names)

    // 先来一个陌生设备占住弹窗
    await emitRequest(
      makeRequest({ requestId: 'req-stranger', deviceName: '陌生设备' }),
    )
    // 再来一条已配对设备：应被自动放行而不打断当前弹窗
    await emitRequest(makeRequest({ requestId: 'req-mate', deviceName: '张三的手机' }))

    expect(consent.currentRequest.value?.requestId).toBe('req-stranger')
    expect(mockInvoke).toHaveBeenCalledWith('respond_peer_consent', {
      requestId: 'req-mate',
      accepted: true,
    })

    // 配对名单动态变化：撤销后同名请求重新走人工确认
    names = []
    await consent.accept()
    expect(consent.currentRequest.value).toBeNull()
  })
})
