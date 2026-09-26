/**
 * Plugin Context 会话 API 单测（票 04：控制面迁 HTTP）
 *
 * 验证 `terminal.sendInput` / `session.list` 从 WS `Message` 信封迁到桌面 HTTP
 * 面后：
 * 1. 权限判定不变（缺权限快速失败，抛「lacks permission」）
 * 2. 有权限时经 `useHttpApi`（httpSendSessionInput / httpListSessions）发请求，
 *    不再经 useMobileCommands 的 WS 信封命令
 * 3. 失败语义：HTTP `code!=0` → 抛 Error(message)（前端归一化口径）
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPluginContext } from '@/plugin/context'
import type { PluginInfo } from '@/plugin/types'

// Mock HTTP 通道：断言调用参数而不真正发请求
const mockHttpSendInput = vi.fn(async () => ({ code: 0, message: 'ok' }))
const mockHttpListSessions = vi.fn(async () => ({
  code: 0,
  message: 'ok',
  data: { sessions: [{ id: 's1', status: 'running' }] },
}))
vi.mock('@/composables/useHttpApi', () => ({
  httpSendSessionInput: (...args: any[]) => mockHttpSendInput(...args),
  httpListSessions: (...args: any[]) => mockHttpListSessions(...args),
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

describe('plugin context session APIs (HTTP, 票 04)', () => {
  beforeEach(() => {
    mockHttpSendInput.mockClear()
    mockHttpListSessions.mockClear()
  })

  it('terminal.sendInput 缺权限快速失败（权限判定不变）', async () => {
    const ctx = createPluginContext(makeInfo([]))
    await expect(ctx.terminal.sendInput('s1', 'ls')).rejects.toThrow('lacks permission for terminal.sendInput')
    expect(mockHttpSendInput).not.toHaveBeenCalled()
  })

  it('terminal.sendInput 有权限时走 HTTP（httpSendSessionInput），不再经 WS 信封', async () => {
    const ctx = createPluginContext(makeInfo(['terminal:input']))
    await ctx.terminal.sendInput('s1', 'ls -la')
    expect(mockHttpSendInput).toHaveBeenCalledTimes(1)
    expect(mockHttpSendInput.mock.calls[0]).toEqual(['s1', 'ls -la'])
  })

  it('terminal.sendInput HTTP 业务失败（code!=0）→ 抛 Error(message)', async () => {
    mockHttpSendInput.mockResolvedValueOnce({ code: 1002, message: 'session not found' })
    const ctx = createPluginContext(makeInfo(['terminal:input']))
    await expect(ctx.terminal.sendInput('s1', 'x')).rejects.toThrow('session not found')
  })

  it('session.list 缺权限快速失败（权限判定不变）', async () => {
    const ctx = createPluginContext(makeInfo([]))
    await expect(ctx.session.list()).rejects.toThrow('lacks permission for session.list')
    expect(mockHttpListSessions).not.toHaveBeenCalled()
  })

  it('session.list 有权限时走 HTTP（httpListSessions），返回 data.sessions', async () => {
    const ctx = createPluginContext(makeInfo(['session:read']))
    const sessions = await ctx.session.list()
    expect(mockHttpListSessions).toHaveBeenCalledTimes(1)
    expect(sessions).toEqual([{ id: 's1', status: 'running' }])
  })

  it('session.list HTTP 业务失败（code!=0）→ 抛 Error(message)', async () => {
    mockHttpListSessions.mockResolvedValueOnce({ code: 1001, message: 'invalid token' })
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
