/**
 * Session Store 行为契约测试
 *
 * 被测：`src/stores/session.ts`（2026-09-21 命令面收敛后的精简 store）
 *
 * 两条通道的契约：
 * 1. 引擎事实/渲染管道 —— 宿主命令面直连（`list_sessions` / `write_to_session` /
 *    `send_special_key` / `resize_session`）
 * 2. 业务动作 —— 经 `com.bedcode.terminal-session` 插件命令面（`plugin_invoke` 转发）：
 *    `session.close`（停止）、`session.config.list`（读配置真源）
 *
 * | 契约ID | 来源 | 行为/规则 | 前置 | 输入 | 预期 | 副作用 | 错误 |
 * |---|---|---|---|---|---|---|---|
 * | C1 | loadSessions 分支 | 列表反射到 store.sessions | 无 | 无 | sessions = 命令返回值 | 覆盖上次列表 | 上抛且不改列表 |
 * | C2 | loadSessionConfig 分支 | 按 id 命中插件配置真源 | 无 | configId | 命中→配置；未命中/空→null | 无（只读） | 插件报错上抛 |
 * | C3 | stopSession 分支 | 先关会话再刷新列表 | 无 | sessionId | plugin_invoke(session.close) → list_sessions | 覆盖 sessions | 关会话失败→上抛且不刷新 |
 * | C4 | 输入/尺寸 action | 参数透传引擎命令 | 无 | 数据 / 尺寸 | 参数逐字段一致 | invoke 调用 | 上抛 |
 *
 * 测试 seam：只 mock IPC 边界（`@tauri-apps/api/core` 的 invoke）——pluginInvoke
 * 属真实执行（真实校验它转发给 `plugin_invoke` 的参数形状）；Pinia 真实。
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useSessionStore } from '@/stores/session'
import { ensureHostCredential } from '@/plugin/commands'
import { makeSessionInfo } from '@/__tests__/fixtures/index'

/** IPC 边界 mock（invoke 是唯一跨进程 seam） */
const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}))

/** 会话中心插件 ID（断言 plugin_invoke 转发的目标） */
const SESSION_PLUGIN_ID = 'com.bedcode.terminal-session'

/** 某命令的全部调用参数（去掉命令名本身） */
function argsOf(cmd: string): unknown[] {
  const call = mockInvoke.mock.calls.find(([c]) => c === cmd)
  return call ? call.slice(1) : []
}

beforeEach(async () => {
  mockInvoke.mockReset()
  // 前端通道身份（审计票 06）：宿主凭证/令牌获取在用例断言范围之外——统一给固定值，
  // 并在每个用例前预热凭证缓存（否则首个用例的 invoke 调用列表会多出一次取凭证）
  mockInvoke.mockImplementation(async (cmd: string) =>
    cmd === 'plugin_frontend_loader_session' ? 'loader-session' : undefined,
  )
  await ensureHostCredential()
  setActivePinia(createPinia())
})

describe('Session Store', () => {
  describe('C1 loadSessions：引擎事实面', () => {
    it('正例：命令返回的会话列表逐字段反射到 store', async () => {
      const running = makeSessionInfo({ id: 'session-1', status: 'running' })
      const stopped = makeSessionInfo({ id: 'session-2', status: 'stopped' })
      mockInvoke.mockResolvedValueOnce([running, stopped])

      const store = useSessionStore()
      await store.loadSessions()

      expect(mockInvoke).toHaveBeenCalledWith('list_sessions')
      expect(store.sessions).toEqual([running, stopped])
    })

    it('空列表：命令返回空数组时 store 清空（不是残留旧值）', async () => {
      mockInvoke.mockResolvedValueOnce([makeSessionInfo({ id: 'old' })])

      const store = useSessionStore()
      await store.loadSessions()
      expect(store.sessions).toHaveLength(1)

      mockInvoke.mockResolvedValueOnce([])
      await store.loadSessions()
      expect(store.sessions).toEqual([])
    })

    it('反例：命令失败时上抛，且不覆盖既有列表', async () => {
      mockInvoke.mockResolvedValueOnce([makeSessionInfo({ id: 'kept' })])

      const store = useSessionStore()
      await store.loadSessions()

      mockInvoke.mockRejectedValueOnce(new Error('list_sessions failed'))
      await expect(store.loadSessions()).rejects.toThrow('list_sessions failed')
      expect(store.sessions.map((s) => s.id)).toEqual(['kept'])
    })
  })

  describe('C2 loadSessionConfig：插件配置真源', () => {
    it('正例：命中 id 时返回该配置，且经 plugin_invoke 调 session.config.list', async () => {
      const configs = [
        { id: 'config-a', name: 'A', working_dir: '/a', command: 'bash' },
        { id: 'config-b', name: 'B', working_dir: '/b', command: 'zsh' },
      ]
      mockInvoke.mockResolvedValueOnce(configs)

      const store = useSessionStore()
      const found = await store.loadSessionConfig('config-b')

      expect(found).toEqual(configs[1])
      expect(mockInvoke).toHaveBeenCalledWith('plugin_invoke', {
        pluginId: SESSION_PLUGIN_ID,
        command: 'session.config.list',
        args: null,
        credential: 'loader-session',
      })
    })

    it('反例：id 未命中返回 null（不抛错）', async () => {
      mockInvoke.mockResolvedValueOnce([{ id: 'config-a' }])

      const store = useSessionStore()
      expect(await store.loadSessionConfig('config-missing')).toBeNull()
    })

    it('边界：插件返回空列表时返回 null', async () => {
      mockInvoke.mockResolvedValueOnce([])

      const store = useSessionStore()
      expect(await store.loadSessionConfig('config-a')).toBeNull()
    })

    it('异常：插件命令失败（未激活 / WASM trap）时上抛，交由调用方提示', async () => {
      mockInvoke.mockRejectedValueOnce('plugin is not activated')

      const store = useSessionStore()
      await expect(store.loadSessionConfig('config-a')).rejects.toBe('plugin is not activated')
    })
  })

  describe('C3 stopSession：停止编排归插件', () => {
    it('正例：先经插件 session.close 停止，再刷新列表（DB 已为空）', async () => {
      const store = useSessionStore()
      mockInvoke.mockResolvedValueOnce(undefined) // plugin_invoke: session.close
      mockInvoke.mockResolvedValueOnce([]) // list_sessions

      await store.stopSession('session-1')

      expect(mockInvoke.mock.calls.map(([c]) => c)).toEqual(['plugin_invoke', 'list_sessions'])
      expect(argsOf('plugin_invoke')).toEqual([
        {
          pluginId: SESSION_PLUGIN_ID,
          command: 'session.close',
          args: { sessionId: 'session-1' },
          credential: 'loader-session',
        },
      ])
      expect(store.sessions).toEqual([])
    })

    it('正例：列表刷新为插件停止后的引擎事实（记录保留、状态 stopped）', async () => {
      const store = useSessionStore()
      mockInvoke.mockResolvedValueOnce(undefined)
      mockInvoke.mockResolvedValueOnce([makeSessionInfo({ id: 'session-1', status: 'stopped' })])

      await store.stopSession('session-1')

      expect(store.sessions.map((s) => `${s.id}:${s.status}`)).toEqual(['session-1:stopped'])
    })

    it('反例：插件停止失败时上抛，且不再拉取列表（旧列表保持）', async () => {
      const store = useSessionStore()
      mockInvoke.mockResolvedValueOnce([makeSessionInfo({ id: 'session-1', status: 'running' })])
      await store.loadSessions()

      mockInvoke.mockRejectedValueOnce('plugin is not activated')
      await expect(store.stopSession('session-1')).rejects.toBe('plugin is not activated')

      expect(mockInvoke.mock.calls.map(([c]) => c)).toEqual(['list_sessions', 'plugin_invoke'])
      expect(store.sessions.map((s) => s.status)).toEqual(['running'])
    })
  })

  describe('C4 引擎命令透传：输入 / 特殊键 / 尺寸', () => {
    it('正例：writeToSession 逐字段透传 write_to_session', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const store = useSessionStore()
      await store.writeToSession('session-1', 'ls -la\r')

      expect(argsOf('write_to_session')).toEqual([{ sessionId: 'session-1', data: 'ls -la\r' }])
    })

    it('正例：sendSpecialKey 透传 send_special_key', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const store = useSessionStore()
      await store.sendSpecialKey('session-1', 'ctrl-c')

      expect(argsOf('send_special_key')).toEqual([{ sessionId: 'session-1', key: 'ctrl-c' }])
    })

    it('正例：resizeSession 默认 force=false 并把裁决结果原样返回', async () => {
      const outcome = { status: 'needsConfirmation', currentCanonical: { kind: 'mobile', deviceName: 'Phone' } }
      mockInvoke.mockResolvedValueOnce(outcome)

      const store = useSessionStore()
      const result = await store.resizeSession('session-1', 120, 40)

      expect(argsOf('resize_session')).toEqual([
        { sessionId: 'session-1', cols: 120, rows: 40, force: false },
      ])
      expect(result).toEqual(outcome)
    })

    it('边界：force=true（用户确认覆盖）透传到命令参数', async () => {
      mockInvoke.mockResolvedValueOnce({ status: 'applied', canonical: { kind: 'desktop' } })

      const store = useSessionStore()
      await store.resizeSession('session-1', 80, 24, true)

      expect(argsOf('resize_session')).toEqual([
        { sessionId: 'session-1', cols: 80, rows: 24, force: true },
      ])
    })

    it('异常：引擎命令失败时上抛（不吞错）', async () => {
      mockInvoke.mockRejectedValueOnce(new Error('session not found'))

      const store = useSessionStore()
      await expect(store.writeToSession('ghost', 'x')).rejects.toThrow('session not found')
    })
  })
})
