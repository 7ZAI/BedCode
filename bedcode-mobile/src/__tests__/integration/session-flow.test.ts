/**
 * 会话流组合集成测试（L2 场景 4）
 *
 * 协作实体：真实 useMobileConnection（配置加载 / 会话启停 / sync 事件状态
 * 联动） + useHttpApi（HTTP REST：/api/configs、/api/sessions/*） +
 * terminalBuffer store（状态变更时的 buffer 联动：markSessionRunning /
 * markSessionStopped / clearBuffer）。
 *
 * 测试 seam：mock invoke + plugin-http.fetch（HTTP 按 URL 分发）+ 脚本化
 * ws_sync_* 事件驱动状态联动。
 *
 * 契约注意：HTTP API 响应为 camelCase（wslDistro / workingDir，来自桌面端
 * HTTP 层），ws_sync_* 事件内嵌 DTO 为 snake_case（Rust serde）——两套形状
 * 并存，fixtures 分别对齐（makeSessionConfigSummary 用于事件、HTTP 响应
 * 手写 camelCase 对象）。
 *
 * 覆盖：配置加载（HTTP → sessionConfigs 映射）；启动会话 + sync created
 * 联动；状态变更（markSessionRunning）；停止（HTTP 本地状态 + sync stopped
 * 双通道）；删除（本地 + sync removed + buffer 清理）；配置同步增改删。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import type { RemoteDevice } from '@/composables/model'
import { flushAsync, loadFreshModule, resetLocalStorage, clearEventHandlers, mockHttpResponse } from './helpers'
import { useTerminalBufferStore } from '@/stores/terminalBuffer'
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

/** fetch 按 URL 分发：/api/health 探测 + HTTP API 响应 */
function installFetchMock(): void {
  mockFetch.mockImplementation((url: string, options: { method?: string; body?: string }) => {
    const method = options?.method || 'GET'
    if (url.endsWith('/api/health')) {
      return Promise.resolve(mockHttpResponse({ status: 'ok', port: 8765, uptime_secs: 120 }))
    }
    // 会话列表 / 配置列表
    if (url.endsWith('/api/configs') && method === 'GET') {
      return Promise.resolve(mockHttpResponse({ code: 0, message: 'ok', data: { configs: [httpConfig(makeSessionConfigSummary())] } }))
    }
    if (url.endsWith('/api/sessions') && method === 'GET') {
      return Promise.resolve(mockHttpResponse({ code: 0, message: 'ok', data: { sessions: [] } }))
    }
    if (url.endsWith('/api/sessions/start') && method === 'POST') {
      return Promise.resolve(mockHttpResponse({ code: 0, message: 'ok', data: { sessionId: 'session-1', status: 'running' } }))
    }
    if (url.includes('/stop') && method === 'POST') {
      return Promise.resolve(mockHttpResponse({ code: 0, message: 'ok' }))
    }
    if (url.includes('/remove') && method === 'DELETE') {
      return Promise.resolve(mockHttpResponse({ code: 0, message: 'ok' }))
    }
    return Promise.resolve(mockHttpResponse({ code: 0, message: 'ok' }))
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
  installFetchMock()
  mockInvoke.mockResolvedValue(undefined)
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

    // 启动会话（HTTP 返回 sessionId）
    const result = await conn.startSession('config-1')
    await flushAsync()
    expect(result.sessionId).toBe('session-1')
    expect(mockFetch).toHaveBeenCalledWith(
      'http://192.168.1.100:8765/api/sessions/start',
      expect.objectContaining({ method: 'POST', body: JSON.stringify({ configId: 'config-1' }) }),
    )

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

  it('状态变更联动：ws_sync_session_status_changed → activeSessions 状态更新 + buffer markSessionRunning', async () => {
    await connectAndPair()
    const bufferStore = useTerminalBufferStore()
    bufferStore.ensureBuffer('session-1')
    bufferStore.markSessionStopped('session-1')
    expect(bufferStore.getBuffer('session-1')!.sessionStopped).toBe(true)

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

    // 重新运行（waiting → running）→ buffer 的 sessionStopped 复位（停止→重启同 id）
    await emit('ws_sync_session_status_changed', makeSyncSessionStatusChanged({
      session_id: 'session-1',
      old_status: 'waiting',
      new_status: 'running',
    }))
    await flushAsync()
    expect(conn.activeSessions.value[0].status).toBe('running')
    expect(bufferStore.getBuffer('session-1')!.sessionStopped).toBe(false)
  })

  it('停止会话：HTTP stop + sync stopped 事件 → 状态 stopped + buffer markSessionStopped', async () => {
    await connectAndPair()
    const bufferStore = useTerminalBufferStore()
    bufferStore.ensureBuffer('session-1')

    await emit('ws_sync_session_created', makeSyncSessionCreated(makeSessionSummary({ id: 'session-1', status: 'running' })))
    await flushAsync()

    // HTTP 停止（本地立即置 stopped）
    await conn.stopSession('session-1')
    await flushAsync()
    expect(conn.activeSessions.value[0].status).toBe('stopped')
    expect(mockFetch).toHaveBeenCalledWith(
      'http://192.168.1.100:8765/api/sessions/session-1/stop',
      expect.objectContaining({ method: 'POST' }),
    )

    // 桌面端广播停止事件（另一条通道）→ 保留记录显示灰色 + buffer 停止标记
    await emit('ws_sync_session_stopped', makeSyncSessionStopped({ session_id: 'session-1' }))
    await flushAsync()
    expect(conn.activeSessions.value).toHaveLength(1)
    expect(conn.activeSessions.value[0].status).toBe('stopped')
    expect(bufferStore.getBuffer('session-1')!.sessionStopped).toBe(true)
  })

  it('删除会话：HTTP remove + sync removed 事件 → 列表移除 + buffer 清理', async () => {
    await connectAndPair()
    const bufferStore = useTerminalBufferStore()
    bufferStore.ensureBuffer('session-1')

    await emit('ws_sync_session_created', makeSyncSessionCreated(makeSessionSummary({ id: 'session-1', status: 'running' })))
    await flushAsync()
    expect(conn.activeSessions.value).toHaveLength(1)

    // HTTP 删除（本地立即移除）
    await conn.removeSession('session-1')
    await flushAsync()
    expect(conn.activeSessions.value).toHaveLength(0)
    expect(mockFetch).toHaveBeenCalledWith(
      'http://192.168.1.100:8765/api/sessions/session-1/remove',
      expect.objectContaining({ method: 'DELETE' }),
    )

    // 桌面端广播删除事件 → buffer 清理（会话记录不残留）
    await emit('ws_sync_session_created', makeSyncSessionCreated(makeSessionSummary({ id: 'session-1' })))
    await flushAsync()
    await emit('ws_sync_session_removed', makeSyncSessionRemoved({ session_id: 'session-1' }))
    await flushAsync()
    expect(conn.activeSessions.value).toHaveLength(0)
    expect(bufferStore.getBuffer('session-1')).toBeUndefined()
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
