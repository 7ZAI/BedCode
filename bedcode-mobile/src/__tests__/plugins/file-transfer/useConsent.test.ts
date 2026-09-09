/**
 * useConsent 编排测试（ticket 04）
 *
 * mock 最小 PluginContext（commands.execute 记录调用 + events.on 捕获处理器
 * 手动派发 + storage 受控 Map + dialogs.showConfirm 受控应答），只测迁移规则
 * 与队列/超时/应答编排不测渲染。场景矩阵对齐 ticket 验收：迁移规则命中静默
 * 自动互信（含弹窗占用时插队）、名单损坏回退弹窗路径、单闸门排队、30s 超时
 * 按拒绝结算、迟到对话框结果静默无害、重复事件幂等、畸形载荷丢弃。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import {
  useConsent,
  consentDisplayName,
  matchesTerminalPairedDevice,
  normalizePairedNames,
  CONSENT_TIMEOUT_MS,
  _resetConsentForTest,
} from '../../../../plugins/file-transfer/src/composables/useConsent'

type EventHandler = (payload: any) => void

/** 最小 mock PluginContext：记录命令调用 + 捕获事件处理器 + 受控存储与对话框 */
function makeContext() {
  const calls: Array<{ id: string; args: any }> = []
  const handlers = new Map<string, EventHandler>()
  const responders = new Map<string, (args: any) => unknown>()
  const store = new Map<string, unknown>()
  const dialogCalls: any[] = []
  /** 预置的受控对话框结果队列；空时默认确认 */
  const dialogResults: Array<Promise<boolean>> = []
  const toasts: Array<{ message: string; type?: string }> = []

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
    storage: {
      async get(key: string) {
        return store.has(key) ? store.get(key) : undefined
      },
      async set(key: string, value: unknown) {
        store.set(key, value)
      },
      async delete(key: string) {
        store.delete(key)
      },
    },
    i18n: {
      t(key: string, params?: Record<string, any>) {
        return params ? `${key}:${JSON.stringify(params)}` : key
      },
    },
    dialogs: {
      showConfirm(options: any) {
        dialogCalls.push(options)
        return dialogResults.shift() ?? Promise.resolve(true)
      },
      showToast(message: string, type?: string) {
        toasts.push({ message, type })
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

  /** 挂起下一个对话框（返回手动放行句柄），模拟「用户未操作」 */
  function holdNextDialog(): { resolve: (v: boolean) => void } {
    let release!: (v: boolean) => void
    dialogResults.push(new Promise<boolean>((r) => (release = r)))
    return { resolve: release }
  }

  return {
    context,
    calls,
    emit,
    onCommand,
    store,
    dialogCalls,
    toasts,
    holdNextDialog,
  }
}

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

describe('consent pure rules', () => {
  it('matchesTerminalPairedDevice requires a known non-empty device name', () => {
    expect(matchesTerminalPairedDevice('iPad Pro', ['iPad Pro', 'Pixel 9'])).toBe(true)
    expect(matchesTerminalPairedDevice('Unknown', ['iPad Pro'])).toBe(false)
    // 无名记录不参与匹配：宁可多弹勿误信
    expect(matchesTerminalPairedDevice(null, ['iPad Pro'])).toBe(false)
    expect(matchesTerminalPairedDevice('', [''])).toBe(false)
    expect(matchesTerminalPairedDevice('iPad Pro', [])).toBe(false)
  })

  it('normalizePairedNames tolerates missing / corrupt / mixed-shape entries', () => {
    expect(normalizePairedNames(undefined)).toEqual([])
    expect(normalizePairedNames(null)).toEqual([])
    expect(normalizePairedNames('not-an-array')).toEqual([])
    expect(normalizePairedNames(42)).toEqual([])
    expect(normalizePairedNames({ name: 'object-not-array' })).toEqual([])
    // 字符串与 { name } 形状并存，畸形条目逐项剔除而非整体作废
    expect(
      normalizePairedNames(['iPad Pro', '', { name: 'Pixel 9' }, { name: 42 }, null, {}]),
    ).toEqual(['iPad Pro', 'Pixel 9'])
  })

  it('display name falls back to short fingerprint then truncated nodeId', () => {
    expect(
      consentDisplayName({
        requestId: 'r',
        nodeId: NODE_FULL,
        fingerprintShort: 'e70a4c92',
        deviceName: 'iPad Pro',
      }),
    ).toBe('iPad Pro')
    expect(
      consentDisplayName({
        requestId: 'r',
        nodeId: NODE_FULL,
        fingerprintShort: '44f19b7c',
        deviceName: null,
      }),
    ).toBe('44f19b7c')
    expect(
      consentDisplayName({ requestId: 'r', nodeId: NODE_FULL, fingerprintShort: '', deviceName: null }),
    ).toBe(NODE_FULL.slice(0, 8))
  })
})

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

  /** 派发 consent 事件并冲刷迁移规则的异步名单读取 */
  async function emitConsent(payload: unknown): Promise<void> {
    env.emit('plugin:file-transfer:consent-requested', payload)
    await vi.advanceTimersByTimeAsync(0)
  }

  function respondCalls() {
    return env.calls.filter((c) => c.id === 'file-transfer.respond-consent')
  }

  it('presents unpaired request through the plugin dialogs API with countdown hint', async () => {
    env.holdNextDialog()
    const consent = startController()

    await emitConsent(makeRequest('req-a'))

    expect(env.dialogCalls).toHaveLength(1)
    expect(env.dialogCalls[0].variant).toBe('warning')
    // 文案全部经 i18n（echo 断言），消息含短指纹与 30s 超时提示
    expect(env.dialogCalls[0].title).toContain('transfer.consent.title')
    expect(env.dialogCalls[0].message).toContain('transfer.consent.body')
    expect(env.dialogCalls[0].message).toContain('req-a'.slice(4))
    expect(env.dialogCalls[0].message).toContain('"seconds":30')
    expect(consent.currentRequest.value?.requestId).toBe('req-a')
    expect(consent.pendingCount.value).toBe(1)
  })

  it('auto-trusts terminal-paired devices silently with toast and no dialog', async () => {
    env.store.set('paired_devices', ['iPad Pro', 'Pixel 9'])
    const consent = startController()

    await emitConsent(
      makeRequest('req-paired', { deviceName: 'iPad Pro', fingerprintShort: 'a1b2c3d4' }),
    )

    expect(respondCalls()).toEqual([
      { id: 'file-transfer.respond-consent', args: { requestId: 'req-paired', accepted: true } },
    ])
    expect(env.dialogCalls).toHaveLength(0)
    expect(consent.currentRequest.value).toBeNull()
    expect(consent.autoTrustedName.value).toBe('iPad Pro')
    expect(env.toasts).toHaveLength(1)
    expect(env.toasts[0].type).toBe('success')
  })

  it('applies the migration rule even while another dialog occupies the gate', async () => {
    env.store.set('paired_devices', ['iPad Pro'])
    env.holdNextDialog()
    const consent = startController()

    await emitConsent(makeRequest('req-stranger'))
    await emitConsent(makeRequest('req-paired', { deviceName: 'iPad Pro' }))

    // 配对设备立即放行，不占用用户交互也不入队；原弹窗保持不动
    expect(respondCalls()).toEqual([
      { id: 'file-transfer.respond-consent', args: { requestId: 'req-paired', accepted: true } },
    ])
    expect(env.dialogCalls).toHaveLength(1)
    expect(consent.currentRequest.value?.requestId).toBe('req-stranger')
  })

  it('corrupt stored paired list falls back to empty and degrades to the dialog path', async () => {
    env.store.set('paired_devices', 'corrupted-not-an-array')
    env.holdNextDialog()
    const consent = startController()

    await emitConsent(makeRequest('req-x', { deviceName: 'iPad Pro' }))

    // 名单读取容错：退化为正常弹窗而非报错或误信
    expect(env.dialogCalls).toHaveLength(1)
    expect(consent.currentRequest.value?.deviceName).toBe('iPad Pro')
    expect(consent.autoTrustedName.value).toBeNull()
  })

  it('confirming the dialog routes accepted:true and denying routes accepted:false', async () => {
    const consent = startController()

    const yesGate = env.holdNextDialog()
    await emitConsent(makeRequest('req-yes'))
    yesGate.resolve(true)
    await vi.advanceTimersByTimeAsync(0)
    expect(respondCalls()).toEqual([
      { id: 'file-transfer.respond-consent', args: { requestId: 'req-yes', accepted: true } },
    ])

    const noGate = env.holdNextDialog()
    await emitConsent(makeRequest('req-no'))
    noGate.resolve(false)
    await vi.advanceTimersByTimeAsync(0)
    expect(respondCalls()).toContainEqual({
      id: 'file-transfer.respond-consent',
      args: { requestId: 'req-no', accepted: false },
    })
    expect(consent.pendingCount.value).toBe(0)
  })

  it('queues subsequent requests behind the single gate and presents them in order', async () => {
    const firstGate = env.holdNextDialog()
    const consent = startController()

    await emitConsent(makeRequest('req-a'))
    await emitConsent(makeRequest('req-b'))
    await emitConsent(makeRequest('req-c'))

    expect(env.dialogCalls).toHaveLength(1)
    expect(consent.pendingCount.value).toBe(3)

    // 预挂后续两个对话框，防止默认立即确认的对话框让队列瞬间清空
    const secondGate = env.holdNextDialog()
    firstGate.resolve(true)
    await vi.advanceTimersByTimeAsync(0)
    expect(consent.currentRequest.value?.requestId).toBe('req-b')

    const thirdGate = env.holdNextDialog()
    secondGate.resolve(false)
    await vi.advanceTimersByTimeAsync(0)
    expect(consent.currentRequest.value?.requestId).toBe('req-c')
    // 新展示项重新弹出对话框（非复用旧框）
    expect(env.dialogCalls).toHaveLength(3)
  })

  it('timeout settles as rejection first; the late dialog result is a silent no-op', async () => {
    const gate = env.holdNextDialog()
    const consent = startController()

    await emitConsent(makeRequest('req-late'))
    await vi.advanceTimersByTimeAsync(CONSENT_TIMEOUT_MS)

    // 超时按拒绝先行结算释放闸门（与宿主 sweeper CONFIRM_TIMEOUT 同值）
    expect(respondCalls()).toEqual([
      { id: 'file-transfer.respond-consent', args: { requestId: 'req-late', accepted: false } },
    ])
    expect(consent.currentRequest.value).toBeNull()

    // 迟到的用户信任未命中任何待确认项：静默无害，不发送第二次应答
    gate.resolve(true)
    await vi.advanceTimersByTimeAsync(0)
    expect(respondCalls()).toHaveLength(1)
  })

  it('programmatic deny settles the current request; late dialog result is ignored', async () => {
    const gate = env.holdNextDialog()
    const consent = startController()

    await emitConsent(makeRequest('req-deny'))
    await consent.deny()

    expect(respondCalls()).toEqual([
      { id: 'file-transfer.respond-consent', args: { requestId: 'req-deny', accepted: false } },
    ])
    expect(consent.currentRequest.value).toBeNull()

    gate.resolve(true)
    await vi.advanceTimersByTimeAsync(0)
    expect(respondCalls()).toHaveLength(1)
  })

  it('accept()/deny() without a presented request are silent no-ops', async () => {
    const consent = startController()
    await consent.accept()
    await consent.deny()
    expect(respondCalls()).toHaveLength(0)
  })

  it('duplicate events with the same requestId are idempotent', async () => {
    env.holdNextDialog()
    const consent = startController()

    const request = makeRequest('req-dup')
    await emitConsent(request)
    await emitConsent({ ...request })
    // 已受理过的 requestId 再次到达：不二次入队也不重复弹窗
    await emitConsent({ ...request })

    expect(consent.pendingCount.value).toBe(1)
    expect(env.dialogCalls).toHaveLength(1)
  })

  it('drops malformed payloads instead of presenting them', async () => {
    env.holdNextDialog()
    const consent = startController()

    await emitConsent(null)
    await emitConsent({})
    await emitConsent({ requestId: '', nodeId: 'x' })
    await emitConsent({ requestId: 'req-ok', nodeId: NODE_FULL })

    expect(consent.currentRequest.value?.requestId).toBe('req-ok')
    expect(consent.pendingCount.value).toBe(1)
    // 缺失字段归一化：短指纹回退截取完整 ID，无名归一为 null
    expect(consent.currentRequest.value!.fingerprintShort).toBe(NODE_FULL.slice(0, 8))
    expect(consent.currentRequest.value!.deviceName).toBeNull()
    // 无名请求的对话框文案带核对提示
    expect(env.dialogCalls[0].message).toContain('transfer.consent.namelessHint')
  })

  it('nameless requests never match the migration rule even with entries stored', async () => {
    env.store.set('paired_devices', ['', '  ', { name: '' }])
    env.holdNextDialog()
    const consent = startController()

    await emitConsent(makeRequest('req-noname', { deviceName: null }))

    expect(env.dialogCalls).toHaveLength(1)
    expect(respondCalls()).toHaveLength(0)
  })

  it('stop() disposes the subscription and ignores in-flight dialog results', async () => {
    const gate = env.holdNextDialog()
    const consent = startController()

    await emitConsent(makeRequest('req-stop'))
    consent.stop()

    expect(consent.currentRequest.value).toBeNull()
    expect(consent.pendingCount.value).toBe(0)
    // 订阅已注销：再次派发不再被受理
    expect(() => env.emit('plugin:file-transfer:consent-requested', makeRequest('req-after'))).toThrow(
      /no captured handler/,
    )
    // 停用后在途对话框晚到结果被忽略，不发送应答
    gate.resolve(true)
    await vi.advanceTimersByTimeAsync(0)
    expect(respondCalls()).toHaveLength(0)

    // 重启后恢复订阅且不残留旧状态
    consent.start()
    env.holdNextDialog()
    await emitConsent(makeRequest('req-fresh'))
    expect(consent.currentRequest.value?.requestId).toBe('req-fresh')
  })

  it('start() is idempotent and does not stack subscriptions', async () => {
    const consent = startController()
    consent.start()

    env.holdNextDialog()
    await emitConsent(makeRequest('req-once'))
    expect(env.dialogCalls).toHaveLength(1)
    expect(consent.pendingCount.value).toBe(1)
  })
})
