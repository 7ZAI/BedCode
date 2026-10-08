/**
 * Plugin Context 会话 API 单测（票 13：会话控制迁插件；票 15 阶段 B：TerminalAPI 退役）
 *
 * 验证 `session.list` 自宿主 HTTP 代理换插件命令面
 * （`@/plugin/sessionCommands` → plugin_invoke `com.bedcode.terminal-session`）后：
 * 1. 权限判定不变（缺权限快速失败，抛「lacks permission」）
 * 2. 有权限时经 `sessionCommands`（listSessions）发请求，
 *    不再经 useMobileCommands 的 WS 信封命令
 * 3. 失败语义：`code!=0` → 抛 Error(message)（前端归一化口径不变）
 * 4. `context.terminal`（TerminalAPI）已随票 15 阶段 B 整面退役：
 *    PluginContext 不再有 terminal 字段（外部插件输入改走
 *    `terminal-session.send-http-input` HTTP 通道 / 订阅帧直写）
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPluginContext } from '@/plugin/context'
import type { PluginInfo } from '@/plugin/types'

// Mock HTTP 通道：断言调用参数而不真正发请求
const mockListSessions = vi.fn(async () => ({
  code: 0,
  message: 'ok',
  data: { sessions: [{ id: 's1', status: 'running' }] },
}))
vi.mock('@/plugin/sessionCommands', () => ({
  listSessions: (...args: any[]) => mockListSessions(...args),
}))

function makeInfo(permissions: string[]): PluginInfo {
  return {
    id: 'test-plugin',
    name: 'Test',
    version: '1.0.0',
    description: '',
    author: '',
    main: '',
    pluginType: 'wasm' as any,
    permissions,
    state: 'activated' as any,
    contributes: {} as any,
    source: '',
    assetDir: '',
  } as PluginInfo
}

describe('plugin context session APIs (plugin commands, 票 13)', () => {
  beforeEach(() => {
    mockListSessions.mockClear()
  })

  it('context.terminal 已退役（TerminalAPI 整面删除，票 15 阶段 B）', () => {
    const ctx = createPluginContext(makeInfo([]))
    // PluginContext 契约不再含 terminal 字段——悬挂 API 不允许以 undefined 形态残存
    expect((ctx as Record<string, unknown>).terminal).toBeUndefined()
  })

  it('session.list 缺权限快速失败（权限判定不变）', async () => {
    const ctx = createPluginContext(makeInfo([]))
    await expect(ctx.session.list()).rejects.toThrow('lacks permission for session.list')
    expect(mockListSessions).not.toHaveBeenCalled()
  })

  it('session.list 有权限时走插件命令面（listSessions），返回 data.sessions', async () => {
    const ctx = createPluginContext(makeInfo(['session:read']))
    const sessions = await ctx.session.list()
    expect(mockListSessions).toHaveBeenCalledTimes(1)
    expect(sessions).toEqual([{ id: 's1', status: 'running' }])
  })

  it('session.list HTTP 业务失败（code!=0）→ 抛 Error(message)', async () => {
    mockListSessions.mockResolvedValueOnce({ code: 1001, message: 'invalid token' })
    const ctx = createPluginContext(makeInfo(['session:read']))
    await expect(ctx.session.list()).rejects.toThrow('invalid token')
  })

  it('session.get 复用 session.list（权限要求 session:read）', async () => {
    const ctx = createPluginContext(makeInfo(['session:read']))
    const session = await ctx.session.get('s1')
    expect(session).toEqual({ id: 's1', status: 'running' })
    // 缺权限时 session.get 同样快速失败
    const noPerm = createPluginContext(makeInfo([]))
    await expect(noPerm.session.get('s1')).rejects.toThrow('lacks permission for session.get')
  })
})
