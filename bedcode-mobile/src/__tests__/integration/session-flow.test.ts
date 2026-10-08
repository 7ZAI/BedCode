/**
 * 会话流组合集成测试（L2 场景 4）
 *
 * 协作实体：真实 useMobileConnection（配置加载 / 会话启停 / sync 事件状态
 * 联动） + useHttpApi（配置 HTTP REST：/api/configs） +
 * plugin/sessionCommands（会话控制：plugin_invoke → com.bedcode.terminal-session，
 * 票 13 自宿主 HTTP 代理迁入）。
 * 终端 buffer 联动（markSessionRunning / markSessionStopped / clearBuffer）已随
 * 终端 UI 域迁终端插件（票 15），其契约测试在
 * `wasm-apps/terminal-session/src/terminal/__tests__/`。
 *
 * 测试 seam：mock invoke（http_request 按 URL 分发配置面；plugin_invoke 按
 * 命令 id 分发会话控制面）+ 脚本化 ws_sync_* 事件驱动状态联动。
 *
 * 契约注意：HTTP API 响应为 camelCase（wslDistro / workingDir，来自桌面端
 * HTTP 层），ws_sync_* 事件内嵌 DTO 为 snake_case（Rust serde）——两套形状
 * 并存，fixtures 分别对齐（makeSessionConfigSummary 用于事件、HTTP 响应
 * 手写 camelCase 对象）。
 *
 * 覆盖：配置加载（HTTP → sessionConfigs 映射）；启动会话 + sync created
 * 联动；状态变更（activeSessions 更新）；停止（HTTP 本地状态 + sync stopped
 * 双通道）；删除（本地 + sync removed）；配置同步增改删。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import type { RemoteDevice } from '@/composables/model'
import { flushAsync, loadFreshModule, resetLocalStorage, clearEventHandlers, mockProxyResponse } from './helpers'
import {
  makeSessionSummary,
  makeSessionConfigSummary,
  makeSyncSessionCreated,
  makeSyncSessionStatusChanged,
  makeSyncSessionStopped,
  makeSyncSessionRemoved,
  makeSyncConfigCreated,
  makeSyncConfigUpdated,
  makeSyncConfigRemoved,
} from '@/__tests__/fixtures/index'

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

/** 插件命令调用（plugin_invoke 的 args 按命令 id 过滤；票 13 会话控制面） */
function pluginInvokeCalls(command: string): unknown[] {
  return mockInvoke.mock.calls
    .filter(([c]) => c === 'plugin_invoke')
    .map(([, args]) => args)
    .filter((args) => (args as { command?: string }).command === command)
}

/** HTTP API 响应形状（camelCase，桌面端 HTTP 层契约） */
function httpConfig(cfg: ReturnType<typeof makeSessionConfigSummary>) {
  return {
    id: cfg.id,
    name: cfg.name,
    environment: cfg.environment,
    wslDistro: cfg.wsl_distro,
    workingDir: cfg.working_dir,
    command: cfg.command,
  }
}

/** invoke 分发：http_request 按 URL（配置面）+ plugin_invoke 按命令 id（会话控制面，票 13） */
function installProxyMock(): void {
  mockInvoke.mockImplementation((cmd: string, args: any) => {
    if (cmd === 'egress_declare_desktop_target') return Promise.resolve(null)
    if (cmd === 'http_request') {
      // 与真实 tauri 反序列化一致：命令签名 http_request(request: HttpProxyRequest)
      const url: string = args?.request?.url || ''
      const method: string = args?.request?.method || 'GET'
      if (url.endsWith('/api/health')) {
        return Promise.resolve(mockProxyResponse({ status: 'ok', port: 8765, uptime_secs: 120 }))
      }
      // 配置列表（会话控制面已迁插件命令，票 13）
      if (url.endsWith('/api/configs') && method === 'GET') {
        return Promise.resolve(mockProxyResponse({ code: 0, message: 'ok', data: { configs: [httpConfig(makeSessionConfigSummary())] } }))
      }
      return Promise.resolve(mockProxyResponse({ code: 0, message: 'ok' }))
    }
    if (cmd === 'plugin_invoke') {
      // 会话控制命令面（com.bedcode.terminal-session；返回形状与退役前 HTTP 信封一致）
      const command: string = args?.command || ''
      if (command === 'terminal-session.list-sessions') {
        return Promise.resolve({ code: 0, message: 'ok', data: { sessions: [] } })
      }
      if (command === 'terminal-session.start-session') {
        return Promise.resolve({ code: 0, message: 'ok', data: { sessionId: 'session-1', status: 'running' } })
      }
      if (command === 'terminal-session.stop-session' || command === 'terminal-session.remove-session') {
        return Promise.resolve({ code: 0, message: 'ok' })
      }
      return Promise.resolve(undefined)
    }
    return Promise.resolve(undefined)
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

/** 建立连接（HTTP 基址就绪）并进入 paired（sync 事件链路需已认证） */
async function connectAndPair(): Promise<void> {
  await conn.connect(DEVICE)
  await flushAsync()
  await emit('ws_connected')
  await emit('ws_paired')
  await flushAsync()
  expect(conn.isPaired.value).toBe(true)
}

beforeEach(async () => {
  vi.clearAllMocks()
  installProxyMock()
  setActivePinia(createPinia())
  await freshConnection()
})

afterEach(() => {
  for (const k of Object.keys(eventHandlers)) delete eventHandlers[k]
})

describe('会话流：useMobileConnection 会话管理 × useHttpApi × sync 事件 × buffer store', () => {
  it('配置加载：HTTP /api/configs → sessionConfigs 映射（camelCase → snake_case）+ hasLoadedConfigs', async () => {
    // 先建立连接（setApiBaseUrl 设置 HTTP 基址；request() 无 baseUrl 直接失败）
    await conn.connect(DEVICE)
    await flushAsync()
    await emit('ws_connected')
    await flushAsync()

    const configs = await conn.loadSessionConfigs()
    await flushAsync()

    expect(configs).toHaveLength(1)
    expect(conn.sessionConfigs.value).toEqual([
      expect.objectContaining({
        id: 'config-1',
        name: '默认',
        environment: 'windows',
        wsl_distro: null,
        working_dir: 'D:/workspace',
        command: 'claude',
      }),
    ])
    expect(conn.hasLoadedConfigs.value).toBe(true)
  })

  it('启动会话：HTTP /api/sessions/start → sync created 事件 → activeSessions 联动', async () => {
    await connectAndPair()

    // 启动会话（插件命令面返回 sessionId）
    const result = await conn.startSession('config-1')
    await flushAsync()
    expect(result.sessionId).toBe('session-1')
    // 票 13：会话控制经 plugin_invoke → com.bedcode.terminal-session（不再经 http_request）
    const startCall = pluginInvokeCalls('terminal-session.start-session')[0] as
      | { pluginId: string; args: { configId: string } }
      | undefined
    expect(startCall?.pluginId).toBe('com.bedcode.terminal-session')
    expect(startCall?.args).toEqual({ configId: 'config-1' })
    expect(invokeCalls('http_request').filter(([a]) =>
      (a as { request: { url: string } }).request.url.includes('/api/sessions'),
    )).toHaveLength(0)

    // 桌面端广播会话创建（sync 事件）→ 活跃会话列表联动
    const session = makeSessionSummary({ id: 'session-1', status: 'running' })
    await emit('ws_sync_session_created', makeSyncSessionCreated(session))
    await flushAsync()

    expect(conn.activeSessions.value).toHaveLength(1)
    expect(conn.activeSessions.value[0]).toMatchObject({ id: 'session-1', status: 'running' })

    // 重复创建事件（网络重放）：防重不重复添加
    await emit('ws_sync_session_created', makeSyncSessionCreated(session))
    await flushAsync()
    expect(conn.activeSessions.value).toHaveLength(1)
  })

  it('状态变更联动：ws_sync_session_status_changed → activeSessions 状态更新（buffer 联动已迁终端插件，票 15）', async () => {
    await connectAndPair()

    // 预置活跃会话
    await emit('ws_sync_session_created', makeSyncSessionCreated(makeSessionSummary({ id: 'session-1', status: 'running' })))
    await flushAsync()

    // 状态变更（running → waiting）→ 列表状态更新
    await emit('ws_sync_session_status_changed', makeSyncSessionStatusChanged({
      session_id: 'session-1',
      old_status: 'running',
      new_status: 'waiting',
    }))
    await flushAsync()
    expect(conn.activeSessions.value[0].status).toBe('waiting')

    // 重新运行（waiting → running）→ 列表状态更新
    await emit('ws_sync_session_status_changed', makeSyncSessionStatusChanged({
      session_id: 'session-1',
      old_status: 'waiting',
      new_status: 'running',
    }))
    await flushAsync()
    expect(conn.activeSessions.value[0].status).toBe('running')
  })

  it('停止会话：HTTP stop + sync stopped 事件 → 状态 stopped（buffer 联动已迁终端插件，票 15）', async () => {
    await connectAndPair()

    await emit('ws_sync_session_created', makeSyncSessionCreated(makeSessionSummary({ id: 'session-1', status: 'running' })))
    await flushAsync()

    // 插件命令停止（本地立即置 stopped）
    await conn.stopSession('session-1')
    await flushAsync()
    expect(conn.activeSessions.value[0].status).toBe('stopped')
    const stopCall = pluginInvokeCalls('terminal-session.stop-session')[0] as
      | { pluginId: string; args: { sessionId: string } }
      | undefined
    expect(stopCall?.args).toEqual({ sessionId: 'session-1' })

    // 桌面端广播停止事件（另一条通道）→ 保留记录显示灰色
    await emit('ws_sync_session_stopped', makeSyncSessionStopped({ session_id: 'session-1' }))
    await flushAsync()
    expect(conn.activeSessions.value).toHaveLength(1)
    expect(conn.activeSessions.value[0].status).toBe('stopped')
  })

  it('删除会话：HTTP remove + sync removed 事件 → 列表移除（buffer 清理已迁终端插件，票 15）', async () => {
    await connectAndPair()

    await emit('ws_sync_session_created', makeSyncSessionCreated(makeSessionSummary({ id: 'session-1', status: 'running' })))
    await flushAsync()
    expect(conn.activeSessions.value).toHaveLength(1)

    // 插件命令删除（本地立即移除）
    await conn.removeSession('session-1')
    await flushAsync()
    expect(conn.activeSessions.value).toHaveLength(0)
    const removeCall = pluginInvokeCalls('terminal-session.remove-session')[0] as
      | { pluginId: string; args: { sessionId: string } }
      | undefined
    expect(removeCall?.args).toEqual({ sessionId: 'session-1' })

    // 桌面端广播删除事件 → 列表不残留
    await emit('ws_sync_session_created', makeSyncSessionCreated(makeSessionSummary({ id: 'session-1' })))
    await flushAsync()
    await emit('ws_sync_session_removed', makeSyncSessionRemoved({ session_id: 'session-1' }))
    await flushAsync()
    expect(conn.activeSessions.value).toHaveLength(0)
  })

  it('配置同步事件：created / updated / removed → sessionConfigs 增改删', async () => {
    await connectAndPair()

    // created：新配置加入（防重）
    const cfg = makeSessionConfigSummary({ id: 'config-1', name: '默认' })
    await emit('ws_sync_config_created', makeSyncConfigCreated(cfg))
    await flushAsync()
    expect(conn.sessionConfigs.value).toHaveLength(1)
    await emit('ws_sync_config_created', makeSyncConfigCreated(cfg))
    await flushAsync()
    expect(conn.sessionConfigs.value).toHaveLength(1)

    // updated：字段更新（含不存在的配置 → 追加）
    await emit('ws_sync_config_updated', makeSyncConfigUpdated(makeSessionConfigSummary({ id: 'config-1', name: '默认-改' })))
    await flushAsync()
    expect(conn.sessionConfigs.value[0].name).toBe('默认-改')

    // removed：移除
    await emit('ws_sync_config_removed', makeSyncConfigRemoved({ config_id: 'config-1', config_name: '默认-改' }))
    await flushAsync()
    expect(conn.sessionConfigs.value).toHaveLength(0)
  })
})
