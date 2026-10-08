/**
 * 连接流组合集成测试（L2 场景 1）
 *
 * 协作实体：真实 useMobileConnection（模块级单例，事件驱动状态机） +
 * useHttpApi（HTTP 探测/API 基址） + useTerminalBufferStore（订阅联动） +
 * useForegroundService / useNotification（invoke 包装）。
 *
 * 测试 seam（与桌面端 integration 同模式）：
 * - 只 mock @tauri-apps/api 边界：core.invoke + event.listen（按事件名捕获
 *   回调，测试内手动触发模拟后端事件推送）+ invoke(http_request) 代理（HTTP API）
 * - composables / store 内部逻辑全部真实执行，fixture 数据取自工厂
 * - 模块级单例经 loadFreshModule 每次用例重新加载（resetModules 清除
 *   init() 的 initialized 标志与全部模块级 ref）
 *
 * 覆盖：连接成功全链路（探测 → WS 连接 → 事件驱动状态流转 → 凭据恢复 →
 * 已配对设备持久化）；探测不可达；12s 连接超时（fake timers 压缩）；
 * 取消连接；意外断开时前端**不**发起重连（重连已收敛到 Rust 侧
 * EventWsSupervisor，唯一自愈入口）；关闭码三档提示分流（认证类致命 /
 * 协议层不可重试 / 可重连）；自动重连开关同步到 Rust；Rust 端重连事件链
 * （ws_reconnecting → ws_reconnected → JWT 认证 → ws_paired）；重连耗尽终态；
 * 服务端关闭。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { toast } from 'vue-sonner'
import { createPinia, setActivePinia } from 'pinia'
import type { RemoteDevice } from '@/composables/model'
import { flushAsync, loadFreshModule, resetLocalStorage, clearEventHandlers } from './helpers'
import { makeAuthCredentials, makeSessionSummary } from '@/__tests__/fixtures/index'
import { useTerminalBufferStore } from '@/stores/terminalBuffer'

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
      case 'ws_get_auth_credentials':
        // 票 14 阶段 B：凭据零过境，受理后由前端从宿主引擎窄读
        return Promise.resolve(makeAuthCredentials())
      case 'egress_declare_desktop_target':
        return Promise.resolve(null)
      case 'http_request':
        // 默认探测可达（probe /api/health）；业务端点测试各自覆盖
        return Promise.resolve({
          status: 200,
          statusText: 'OK',
          headers: {},
          bodyText: JSON.stringify({ status: 'ok', port: 8765, uptime_secs: 120 }),
        })
      default:
        return Promise.resolve(undefined)
    }
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

    await conn.connect(DEVICE)
    await flushAsync()

    // HTTP 探测经统一代理（desktop 类 + 3 秒超时 + request_id）
    const probeCall = invokeCalls('http_request').find(([args]) =>
      (args as { request: { url: string } }).request.url.endsWith('/api/health'),
    )
    expect(probeCall).toBeTruthy()
    expect((probeCall![0] as { request: { url: string } }).request.url).toBe(
      'http://192.168.1.100:8765/api/health',
    )
    expect((probeCall![0] as { request: { kind?: string } }).request.kind).toBe('desktop')
    expect((probeCall![0] as { request: { timeoutMs?: number } }).request.timeoutMs).toBe(3000)
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
    // 连接建立 → 10 后输出经终端 socket 直连，不再注册全局 ws_output 监听
    expect(eventHandlers['ws_output']).toBeUndefined()

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
    mockInvoke.mockRejectedValue(new Error('Network error'))

    await expect(conn.connect(DEVICE)).rejects.toThrow('mobile.connection.unreachable')
    await flushAsync()

    expect(conn.connectionStatus.value).toBe('error')
    expect(conn.connectionError.value).toBe('mobile.connection.unreachable')
    expect(conn.isConnecting.value).toBe(false)
    expect(invokeCalls('ws_connect')).toHaveLength(0)
  })

  it('连接超时：12 秒未收到 ws_connected → timeoutToast + 状态 error', async () => {
    vi.useFakeTimers()

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

  it('意外断开：前端**不**发起重连（重连已收敛到 Rust 监督任务），仅更新 UI 与订阅信念', async () => {
    // 契约变更（2026-10-04 审计 P0-2）：重连循环只剩 Rust 侧
    // EventWsSupervisor 一处。前端收到 ws_unexpected_disconnect 后只做
    // UI/通知，绝不再 invoke ws_reconnect —— 旧实现两处并发触发，Rust 先赢、
    // 前端后到的调用撞上 is_reconnecting 直接 skip，但前端计数已 +1，
    // 3 次预算被空转烧掉（表现为“明明在线却说自动重连已放弃”）。
    await freshConnection(() => {
      const creds = makeAuthCredentials()
      localStorage.setItem('auth_pairing_id', creds.pairingId)
      localStorage.setItem('auth_fingerprint', creds.fingerprint)
      localStorage.setItem('auth_session_token', creds.sessionToken)
    })

    await conn.connect(DEVICE)
    await flushAsync()
    await emit('ws_connected')
    await flushAsync()
    expect(conn.connectionStatus.value).toBe('connected')

    // 订阅信念必须清除：服务端订阅随连接关闭，不清则重连后的重订阅会被
    // subscribed=true 跳过，桌面端新连接无订阅 → 终端只有历史没有实时
    const bufferStore = useTerminalBufferStore()
    bufferStore.ensureBuffer('s1')
    bufferStore.markSubscribed('s1')
    expect(bufferStore.getBuffer('s1')?.subscribed).toBe(true)

    vi.mocked(toast.error).mockClear()
    vi.mocked(toast.warning).mockClear()
    await emit('ws_unexpected_disconnect', { reason: 'Connection reset' })
    await flushAsync()

    // 核心断言：没有任何前端发起的重连
    expect(invokeCalls('ws_reconnect')).toHaveLength(0)
    // UI 如实反映「已断开」（断开是事实，与是否自愈无关）
    expect(conn.connectionStatus.value).toBe('disconnected')
    expect(conn.isConnecting.value).toBe(false)
    expect(conn.connectionError.value).toBe('common.notification.connectionDisconnected')
    // 但 Toast 不应直接标注「连接已断开」作为最终态：可重连断开应指向正在恢复
    expect(vi.mocked(toast.warning).mock.calls.at(-1)?.[0]).toContain('连接中断，正在自动重连')
    expect(vi.mocked(toast.error).mock.calls.some(([message]) => String(message).includes('连接已断开'))).toBe(false)
    expect(
      bufferStore.getBuffer('s1')?.subscribed,
      '断连后订阅信念未清除：重连后的重订阅会被 skipped，终端只剩历史没实时',
    ).toBe(false)
  })

  it('认证类致命关闭（fatal）：最后一次提示是「需重新配对」而非普通断连', async () => {
    await freshConnection(() => {
      const creds = makeAuthCredentials()
      localStorage.setItem('auth_session_token', creds.sessionToken)
    })
    await conn.connect(DEVICE)
    await flushAsync()
    await emit('ws_connected')
    await flushAsync()

    vi.mocked(toast.error).mockClear()
    await emit('ws_unexpected_disconnect', { reason: 'closed 4001', fatal: true })
    await flushAsync()

    // 仍不发起前端重连（Rust 监督任务已跳过自愈）
    expect(invokeCalls('ws_reconnect')).toHaveLength(0)
    expect(conn.connectionStatus.value).toBe('disconnected')
    // 最后一条提示必须点名「重新配对」——与普通断连文案区分，否则用户只会反复重试。
    // 断言渲染后的文案（而非 i18n key）：锁的是用户实际看到的那句话。
    const lastToast = String(vi.mocked(toast.error).mock.calls.at(-1)?.[0])
    expect(lastToast).toContain('请重新配对')
    expect(lastToast).not.toContain('连接协议不兼容')
  })

  it('协议/策略层不可重试关闭（non_retryable）：提示指向「升级应用」而非「重新配对」', async () => {
    // 第三档（2026-10-04 审计 P1-4）：1002/1003/1007/1008/1009/1010。
    // 重连必然同样失败，正确动作是升级一端；若沿用 authFailedRePair 文案，
    // 用户会按“重新配对”折腾一圈也解决不了版本/协议不匹配。
    await freshConnection(() => {
      const creds = makeAuthCredentials()
      localStorage.setItem('auth_session_token', creds.sessionToken)
    })
    await conn.connect(DEVICE)
    await flushAsync()
    await emit('ws_connected')
    await flushAsync()

    vi.mocked(toast.error).mockClear()
    await emit('ws_unexpected_disconnect', { reason: 'closed 1002', fatal: false, non_retryable: true })
    await flushAsync()

    expect(invokeCalls('ws_reconnect')).toHaveLength(0)
    expect(conn.connectionStatus.value).toBe('disconnected')
    const lastToast = String(vi.mocked(toast.error).mock.calls.at(-1)?.[0])
    expect(lastToast).toContain('连接协议不兼容')
    // 互斥：不得指向「重新配对」——那是认证类致命的文案，对版本不匹配无效
    expect(lastToast).not.toContain('请重新配对')
  })

  it('连接中意外断开（ws_connected 未到达）：UI 同样不发起重连', async () => {
    await freshConnection(() => {
      const creds = makeAuthCredentials()
      localStorage.setItem('auth_session_token', creds.sessionToken)
    })

    const p = conn.connect(DEVICE)
    await flushAsync()

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
    vi.mocked(toast.success).mockClear()
    await emit('ws_reconnected')
    await flushAsync()
    expect(invokeCalls('ws_authenticate')).toEqual([[{ sessionToken: 'test-jwt-token' }]])
    expect(conn.connectionStatus.value).toBe('connected')
    expect(conn.isConnecting.value).toBe(true)
    expect(vi.mocked(toast.success).mock.calls.at(-1)?.[0]).toBe('连接已恢复')

    // 认证成功 → ws_paired → 状态 paired（重连闭环完成）
    await emit('ws_paired')
    await flushAsync()
    expect(conn.isPaired.value).toBe(true)
  })

  it('重连失败事件：ws_reconnect_failed → 状态 disconnected，且后续断开不由前端接管', async () => {
    // Rust 侧退避耗尽的终态。前端不得在这里（也不会）另起一套重试——
    // 重连循环只有 EventWsSupervisor 一处。
    await freshConnection(() => {
      const creds = makeAuthCredentials()
      localStorage.setItem('auth_session_token', creds.sessionToken)
    })

    await emit('ws_reconnect_failed', { reason: 'Connection refused' })
    await flushAsync()

    expect(conn.connectionStatus.value).toBe('disconnected')
    expect(conn.isConnecting.value).toBe(false)
    expect(conn.connectionError.value).toBe('common.notification.connectionDisconnected')

    // 后续再触发意外断开：仍无前端发起的重连
    await emit('ws_unexpected_disconnect', { reason: 'again' })
    await flushAsync()
    expect(invokeCalls('ws_reconnect')).toHaveLength(0)
    expect(conn.connectionStatus.value).toBe('disconnected')
  })

  it('自动重连已关闭时的断连：提示指向「手动重连」而非「正在自愈」', async () => {
    // Rust 监督任务在用户关闭自动重连后跳过自愈；不告知的话前端只显示通用
    // 「连接已断开」，用户分不清「正在自愈」与「不会自愈」，只能干等退避耗尽。
    await freshConnection(() => {
      const creds = makeAuthCredentials()
      localStorage.setItem('auth_session_token', creds.sessionToken)
    })
    await conn.connect(DEVICE)
    await flushAsync()
    await emit('ws_connected')
    await flushAsync()

    vi.mocked(toast.error).mockClear()
    await emit('ws_unexpected_disconnect', { reason: 'peer gone', auto_reconnect_disabled: true })
    await flushAsync()

    expect(invokeCalls('ws_reconnect')).toHaveLength(0)
    expect(conn.connectionStatus.value).toBe('disconnected')
    const lastToast = String(vi.mocked(toast.error).mock.calls.at(-1)?.[0])
    expect(lastToast).toContain('自动重连已关闭')
    expect(lastToast).not.toContain('连接协议不兼容')
  })

  it('自动重连开关同步到 Rust：默认开启', async () => {
    // 契约：Rust 侧 flag 不会随 localStorage 自动恢复，init() 必须推一次。
    // 漏推的后果是用户关掉的开关在重启后悄悄变回开启。
    //
    // 取**最后一次**调用而非全量：loadFreshModule 只重置模块缓存，上一用例的
    // init() 异步链仍可能在本用例内落地（helpers 已记录同类跨用例残留）。
    // 要锁的是「本模块最终把哪个值递给了 Rust」。
    await freshConnection()
    expect(invokeCalls('set_auto_reconnect').at(-1)).toEqual([{ enabled: true }])
  })

  it('自动重连开关同步到 Rust：用户关掉时推 false', async () => {
    // 反例：同步逻辑存在但恒推 true，等于开关没接上
    await freshConnection(() => {
      localStorage.setItem('mobile-settings', JSON.stringify({ autoReconnect: false }))
    })
    expect(invokeCalls('set_auto_reconnect').at(-1)).toEqual([{ enabled: false }])
  })

  it('凭证被永久拒绝：ws_reauth_rejected → 状态 error（**不是** disconnected），且不再自愈重试', async () => {
    // 契约（ADR 0033 §F3 / 票 08 §8.2）：桌面端答复「凭据不认」（入场密钥换手 /
    // 设备被撤销）与「重连重试耗尽」是**两件事**：
    // - 状态不同：error（需用户重新配对）vs disconnected（网络类）；
    // - 后续行为不同：两者都不自愈，但本者必须让 UI 呈现「重新配对」而非
    //   「重连失败」——合并成一个终态正是本事件存在的理由。
    await freshConnection(() => {
      const creds = makeAuthCredentials()
      localStorage.setItem('auth_pairing_id', creds.pairingId)
      localStorage.setItem('auth_fingerprint', creds.fingerprint)
      localStorage.setItem('auth_session_token', creds.sessionToken)
    })

    await emit('ws_reauth_rejected', { reason: 'Authentication error: code 1007: Invalid token' })
    await flushAsync()

    // 正向断言先行：先证明它确实不是 reconnect_failed 的终态
    expect(conn.connectionStatus.value).toBe('error')
    expect(conn.connectionStatus.value).not.toBe('disconnected')
    expect(conn.isConnecting.value).toBe(false)

    // 重试已耗尽：后续意外断开不得再拉起重连（否则会对着一个故意拒绝的宿主刷重试）
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

  it('事件通道就绪：ws_event_channel_ready → 触发一次 HTTP 对账（重连期间变化靠全量拉取补齐，票 03）', async () => {
    // 通道就绪 = session-control 极简认证首帧发出（Rust 发射）；事件不重放，
    // 对账 = 拉 /api/sessions 全量 → store 收敛（消费端按 id 去重/状态收敛）
    const sessions = [
      makeSessionSummary({ id: 's1', name: 'dev', status: 'running' }),
      makeSessionSummary({ id: 's2', name: 'itest', status: 'stopped' }),
    ]

    // 先建立连接：setApiBaseUrl 在 fresh 模块实例上生效（基址是 request() 前置）
    await conn.connect(DEVICE)
    await flushAsync()

    // 通道就绪后换装 sessions 响应（默认 mock 只答 /api/health 探测波形）
    mockInvoke.mockImplementation((cmd: string, args?: any) => {
      if (cmd === 'http_request') {
        const url: string = args?.request?.url || ''
        if (url.endsWith('/api/sessions')) {
          return Promise.resolve({
            status: 200,
            statusText: 'OK',
            headers: {},
            bodyText: JSON.stringify({ code: 0, data: { sessions } }),
          })
        }
        return Promise.resolve({ status: 404, statusText: 'Not Found', headers: {}, bodyText: '' })
      }
      return Promise.resolve(undefined)
    })

    await emit('ws_event_channel_ready')
    await flushAsync()

    // 恰好一次 HTTP 全量拉取 → 列表落入 activeSessions
    const sessionCalls = invokeCalls('http_request').filter((c) =>
      String((c[0] as any)?.request?.url ?? '').endsWith('/api/sessions'),
    )
    expect(sessionCalls).toHaveLength(1)
    expect(conn.activeSessions.value.map((s: any) => s.id)).toEqual(['s1', 's2'])
  })

  it('sendInput 经 HTTP 输入面：成功形状 + 载荷透传（data/specialKey 原样携带）', async () => {
    // 控制面迁 HTTP 后输入走 POST /api/sessions/{id}/input（票 04）；
    // specialKey 原样透传（桌面端翻译），本端不解释
    const { setApiBaseUrl } = await import('@/composables/useHttpApi')
    setApiBaseUrl(DEVICE.address, DEVICE.port)

    mockInvoke.mockImplementation((cmd: string, args?: any) => {
      if (cmd === 'http_request') {
        const url: string = args?.request?.url || ''
        if (url.endsWith('/api/sessions/s1/input')) {
          return Promise.resolve({
            status: 200,
            statusText: 'OK',
            headers: {},
            bodyText: JSON.stringify({ code: 0, message: 'ok' }),
          })
        }
        return Promise.resolve({ status: 404, statusText: 'Not Found', headers: {}, bodyText: '' })
      }
      return Promise.resolve(undefined)
    })

    await conn.sendInput('s1', 'ls -la', 'ctrl+c')

    const calls = invokeCalls('http_request').filter((c) =>
      String((c[0] as any)?.request?.url ?? '').endsWith('/api/sessions/s1/input'),
    )
    expect(calls).toHaveLength(1)
    const req = (calls[0][0] as any).request
    expect(req.method).toBe('POST')
    expect(req.url).toBe('http://192.168.1.100:8765/api/sessions/s1/input')
    expect(JSON.parse(req.body)).toEqual({ data: 'ls -la', specialKey: 'ctrl+c' })
  })

  it('sendInput 业务错误：HTTP 200 + {code:1002} → sendInput throw（不静默吞错）', async () => {
    const { setApiBaseUrl } = await import('@/composables/useHttpApi')
    setApiBaseUrl(DEVICE.address, DEVICE.port)

    mockInvoke.mockImplementation((cmd: string, args?: any) => {
      if (cmd === 'http_request') {
        const url: string = args?.request?.url || ''
        if (url.endsWith('/api/sessions/s1/input')) {
          return Promise.resolve({
            status: 200,
            statusText: 'OK',
            headers: {},
            bodyText: JSON.stringify({ code: 1002, message: 'session not found' }),
          })
        }
        return Promise.resolve({ status: 404, statusText: 'Not Found', headers: {}, bodyText: '' })
      }
      return Promise.resolve(undefined)
    })

    await expect(conn.sendInput('s1', 'x')).rejects.toThrow('session not found')
  })
})
