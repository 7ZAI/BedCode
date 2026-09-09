/**
 * useConsent 编排测试（ticket 03）
 *
 * mock 最小 PluginContext（commands.execute 记录调用 + events.on 捕获处理器
 * 手动派发），只测队列/倒计时/应答编排不测渲染。场景矩阵对齐 ticket 验收：
 * 入队闸门、30s 超时按拒绝结算释放闸门、接受/拒绝命令路由带 requestId、
 * 事件重复到达幂等、迟到应答静默无害、畸形载荷丢弃。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import {
  useConsent,
  consentDisplayName,
  CONSENT_TIMEOUT_MS,
  _resetConsentForTest,
} from '../../../../plugins/file-transfer/src/composables/useConsent'

type EventHandler = (payload: any) => void

/** 最小 mock PluginContext：记录命令调用 + 捕获事件处理器 */
function makeContext() {
  const calls: Array<{ id: string; args: any }> = []
  const handlers = new Map<string, EventHandler>()
  const responders = new Map<string, (args: any) => unknown>()

  const context = {
    commands: {
      async execute(id: string, args?: any) {
        calls.push({ id, args })
        const respond = responders.get(id)
        if (!respond) throw new Error(`no responder for ${id}`)
        return respond(args)
      },
      register: vi.fn(),
    },
    events: {
      on(event: string, handler: EventHandler) {
        handlers.set(event, handler)
        return { dispose: () => handlers.delete(event) }
      },
    },
  } as unknown as PluginContext

  function emit(event: string, payload: unknown): void {
    const handler = handlers.get(event)
    if (!handler) throw new Error(`no captured handler for ${event}`)
    handler(payload)
  }

  function onCommand(id: string, respond: (args: any) => unknown): void {
    responders.set(id, respond)
  }

  return { context, calls, emit, onCommand }
}

const REQ_A = 'req-aaa'
const REQ_B = 'req-bbb'
const REQ_C = 'req-ccc'
const NODE_FULL = 'e70a4c92d85f41b6a0d3c97f52e1b834'

function makeRequest(requestId: string, overrides: Record<string, any> = {}) {
  return {
    requestId,
    nodeId: `node-${requestId}`,
    fingerprintShort: requestId.slice(4),
    deviceName: `设备-${requestId.slice(4)}`,
    ...overrides,
  }
}

describe('useConsent orchestration', () => {
  let env: ReturnType<typeof makeContext>

  beforeEach(() => {
    vi.useFakeTimers()
    _resetConsentForTest()
    env = makeContext()
    env.onCommand('file-transfer.respond-consent', () => ({ hit: true }))
  })

  afterEach(() => {
    _resetConsentForTest()
    vi.useRealTimers()
  })

  function startController() {
    const consent = useConsent(env.context)
    consent.start()
    return consent
  }

  it('presents the first request and queues subsequent ones behind a single gate', () => {
    const consent = startController()

    env.emit('plugin:file-transfer:consent-requested', makeRequest(REQ_A))
    env.emit('plugin:file-transfer:consent-requested', makeRequest(REQ_B))
    env.emit('plugin:file-transfer:consent-requested', makeRequest(REQ_C))

    expect(consent.currentRequest.value?.requestId).toBe(REQ_A)
    expect(consent.pendingCount.value).toBe(3)
    // 展示即启动倒计时：满秒起步
    expect(consent.remainingSeconds.value).toBe(CONSENT_TIMEOUT_MS / 1000)
  })

  it('accept routes respond-consent with requestId and advances the queue', async () => {
    const consent = startController()

    env.emit('plugin:file-transfer:consent-requested', makeRequest(REQ_A))
    env.emit('plugin:file-transfer:consent-requested', makeRequest(REQ_B))

    await consent.accept()

    expect(env.calls).toContainEqual({
      id: 'file-transfer.respond-consent',
      args: { requestId: REQ_A, accepted: true },
    })
    // 闸门立即释放并展示下一排队项
    expect(consent.currentRequest.value?.requestId).toBe(REQ_B)
    expect(consent.pendingCount.value).toBe(1)
    // 新展示项的倒计时重置
    expect(consent.remainingSeconds.value).toBe(CONSENT_TIMEOUT_MS / 1000)
  })

  it('deny routes accepted:false and closing the last request empties the gate', async () => {
    const consent = startController()

    env.emit('plugin:file-transfer:consent-requested', makeRequest(REQ_A))

    await consent.deny()

    expect(env.calls).toContainEqual({
      id: 'file-transfer.respond-consent',
      args: { requestId: REQ_A, accepted: false },
    })
    expect(consent.currentRequest.value).toBeNull()
    expect(consent.pendingCount.value).toBe(0)
  })

  it('timeout settles as rejection first and releases the gate to the next queued item', async () => {
    const consent = startController()

    env.emit('plugin:file-transfer:consent-requested', makeRequest(REQ_A))
    env.emit('plugin:file-transfer:consent-requested', makeRequest(REQ_B))

    await vi.advanceTimersByTimeAsync(CONSENT_TIMEOUT_MS)

    // 超时按拒绝结算，与宿主 sweeper 语义对齐；随后自动弹出下一项
    expect(env.calls).toContainEqual({
      id: 'file-transfer.respond-consent',
      args: { requestId: REQ_A, accepted: false },
    })
    expect(consent.currentRequest.value?.requestId).toBe(REQ_B)
    expect(consent.pendingCount.value).toBe(1)
  })

  it('late answer after timeout is a silent no-op without extra commands', async () => {
    const consent = startController()

    env.emit('plugin:file-transfer:consent-requested', makeRequest(REQ_A))
    await vi.advanceTimersByTimeAsync(CONSENT_TIMEOUT_MS + 1000)

    const callsBefore = env.calls.length
    await consent.accept()
    await consent.deny()

    expect(consent.currentRequest.value).toBeNull()
    expect(env.calls.length).toBe(callsBefore)
  })

  it('duplicate events with the same requestId are idempotent', async () => {
    const consent = startController()

    const request = makeRequest(REQ_A)
    env.emit('plugin:file-transfer:consent-requested', request)
    env.emit('plugin:file-transfer:consent-requested', { ...request })

    expect(consent.pendingCount.value).toBe(1)

    await consent.accept()
    // 已结算的 requestId 再次到达也不重新弹窗/入队
    env.emit('plugin:file-transfer:consent-requested', { ...request })
    expect(consent.currentRequest.value).toBeNull()
    expect(env.calls.filter((c) => c.id === 'file-transfer.respond-consent')).toHaveLength(1)
  })

  it('drops malformed payloads instead of presenting them', () => {
    const consent = startController()

    env.emit('plugin:file-transfer:consent-requested', null)
    env.emit('plugin:file-transfer:consent-requested', {})
    env.emit('plugin:file-transfer:consent-requested', { requestId: '', nodeId: 'x' })
    env.emit('plugin:file-transfer:consent-requested', { requestId: REQ_A, nodeId: NODE_FULL })

    expect(consent.currentRequest.value?.requestId).toBe(REQ_A)
    expect(consent.pendingCount.value).toBe(1)
    // 缺失字段归一化：短指纹回退截取完整 ID，无名归一为 null
    expect(consent.currentRequest.value!.fingerprintShort).toBe(NODE_FULL.slice(0, 8))
    expect(consent.currentRequest.value!.deviceName).toBeNull()
  })

  it('stop() disposes the subscription and resets all state', async () => {
    const consent = startController()

    env.emit('plugin:file-transfer:consent-requested', makeRequest(REQ_A))
    env.emit('plugin:file-transfer:consent-requested', makeRequest(REQ_B))

    consent.stop()

    expect(consent.currentRequest.value).toBeNull()
    expect(consent.pendingCount.value).toBe(0)
    // 订阅已注销：再次派发不再被受理
    expect(() =>
      env.emit('plugin:file-transfer:consent-requested', makeRequest(REQ_C)),
    ).toThrow(/no captured handler/)
    // 重启后恢复订阅且不残留旧状态
    consent.start()
    env.emit('plugin:file-transfer:consent-requested', makeRequest(REQ_C))
    expect(consent.currentRequest.value?.requestId).toBe(REQ_C)
    expect(consent.pendingCount.value).toBe(1)
  })

  it('start() is idempotent and does not stack subscriptions', () => {
    const consent = startController()
    consent.start()

    env.emit('plugin:file-transfer:consent-requested', makeRequest(REQ_A))
    expect(consent.pendingCount.value).toBe(1)
  })

  it('display name falls back to short fingerprint then truncated nodeId', () => {
    expect(
      consentDisplayName({ requestId: 'r', nodeId: NODE_FULL, fingerprintShort: 'e70a4c92', deviceName: 'iPad Pro' }),
    ).toBe('iPad Pro')
    expect(
      consentDisplayName({ requestId: 'r', nodeId: NODE_FULL, fingerprintShort: '44f19b7c', deviceName: null }),
    ).toBe('44f19b7c')
    expect(
      consentDisplayName({ requestId: 'r', nodeId: NODE_FULL, fingerprintShort: '', deviceName: null }),
    ).toBe(NODE_FULL.slice(0, 8))
  })
})
