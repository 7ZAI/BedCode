import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createPluginContext } from '@/plugin/context'
import {
  ensureHostCredential,
  pluginAuthSetStrategy,
  pluginChannelToken,
  pluginFsAuthRespond,
} from '@/plugin/commands'
import type { PluginInfo } from '@/plugin/types'

/**
 * 前端插件通道身份契约（审计票 06 / P0-5）
 *
 * 被测行为：前端插件面的命令**不再自报身份**，而由宿主签发的通道令牌绑定：
 * - C1 宿主面凭证（loader 会话密钥）只从 `plugin_frontend_loader_session` 取一次（幂等）
 * - C2 插件令牌必须用宿主面凭证换取（`plugin_channel_token` 带 loaderSession），插件无法自造
 * - C3 `PluginContext` 的每个插件面调用都携带**同一枚令牌**（storage / terminal / plugin_invoke），
 *      且不带任何「自选身份」参数
 * - C4 fs 授权应答带宿主面凭证（插件不得替用户同意自己的文件访问请求）
 *
 * 不测内部实现：断言的是「发给宿主的命令与参数」这一可观测边界。
 */

const invokeMock = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }))

const LOADER_SESSION = 'loader-session-test'
const PLUGIN_ID = 'com.test.channel'

function makePluginInfo(): PluginInfo {
  return {
    id: PLUGIN_ID,
    name: 'Channel Test',
    version: '1.0.0',
    description: '',
    author: 'tester',
    main: 'index.js',
    pluginType: 'ts-only',
    permissions: ['storage', 'terminal:input'],
    state: { state: 'activated' },
    extensionPath: '/tmp/com.test.channel',
    contributes: {},
    source: 'builtin',
    sizeBytes: 0,
  } as unknown as PluginInfo
}

/** 取某命令的调用参数（去掉命令名） */
function argsOf(cmd: string): Record<string, unknown> {
  const call = invokeMock.mock.calls.find(([c]) => c === cmd)
  expect(call, `expected invoke('${cmd}') to have been called`).toBeTruthy()
  return call![1] as Record<string, unknown>
}

beforeEach(async () => {
  invokeMock.mockReset()
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === 'plugin_frontend_loader_session') return LOADER_SESSION
    if (cmd === 'plugin_channel_token') return `token-${String(args?.pluginId)}`
    return undefined
  })
  // 预热宿主面凭证缓存：后续断言只看用例自身的调用
  await ensureHostCredential()
  invokeMock.mockClear()
})

describe('前端插件通道身份', () => {
  it('C1 宿主面凭证幂等：多次取用只打一次宿主命令', async () => {
    const first = await ensureHostCredential()
    const second = await ensureHostCredential()

    expect(first).toBe(LOADER_SESSION)
    expect(second).toBe(LOADER_SESSION)
    expect(invokeMock.mock.calls.filter(([c]) => c === 'plugin_frontend_loader_session')).toHaveLength(0)
  })

  it('C2 插件令牌必须用宿主面凭证换取（不能自报 plugin_id 直接换）', async () => {
    const token = await pluginChannelToken(PLUGIN_ID)

    expect(token).toBe(`token-${PLUGIN_ID}`)
    expect(argsOf('plugin_channel_token')).toEqual({
      pluginId: PLUGIN_ID,
      loaderSession: LOADER_SESSION,
    })
  })

  it('C3 PluginContext 的插件面调用全部携带同一枚令牌', async () => {
    const ctx = await createPluginContext(makePluginInfo())
    const token = `token-${PLUGIN_ID}`
    invokeMock.mockClear()

    await ctx.storage.set('k', 1)
    await ctx.storage.get('k')
    await ctx.storage.delete('k')
    await ctx.commands.execute('demo.cmd', { a: 1 })

    // 每一类插件面命令都带令牌（credential 是身份，pluginId 只是目标）
    expect(argsOf('plugin_storage_set').credential).toBe(token)
    expect(argsOf('plugin_storage_get').credential).toBe(token)
    expect(argsOf('plugin_storage_delete').credential).toBe(token)
    expect(argsOf('plugin_invoke').credential).toBe(token)
    // 目标仍是本插件自己，不是自选身份
    expect(argsOf('plugin_invoke').pluginId).toBe(PLUGIN_ID)
    // 插件面无凭证的调用一律不存在
    // 票 08：`plugin_terminal_send_input` 已注销（输入改走插件命令通道 `plugin_invoke`），
    // 故这里只剩存储三条 + 互调一条
    const pluginFaceCalls = invokeMock.mock.calls.filter(([c]) =>
      ['plugin_storage_get', 'plugin_storage_set', 'plugin_storage_delete', 'plugin_invoke'].includes(
        c,
      ),
    )
    expect(pluginFaceCalls).toHaveLength(4)
    for (const [, args] of pluginFaceCalls) {
      expect(typeof (args as Record<string, unknown>).credential).toBe('string')
    }
  })

  it('C4 fs 授权应答带宿主面凭证（插件不得替用户同意）', async () => {
    await pluginFsAuthRespond('req-1', 'deny')

    expect(argsOf('plugin_fs_auth_respond')).toEqual({
      requestId: 'req-1',
      decision: 'deny',
      credential: LOADER_SESSION,
    })
  })

  it('C4 策略档位设置同样带宿主面凭证（插件不得改自己的档位，票 03）', async () => {
    await pluginAuthSetStrategy('com.bedcode.test', 'fs', 'always_ask')

    expect(argsOf('plugin_auth_set_strategy')).toEqual({
      pluginId: 'com.bedcode.test',
      resource: 'fs',
      strategy: 'always_ask',
      credential: LOADER_SESSION,
    })
  })

  it('C5 冷启动并发取凭证只打一次宿主命令（在途请求共享）', async () => {
    // 冷缓存路径要新模块实例（本文件 beforeEach 已预热缓存；vi.resetModules 不碰生产代码）
    vi.resetModules()
    const fresh = await import('@/plugin/commands')
    let releaseInvoke: ((credential: string) => void) | null = null
    invokeMock.mockImplementationOnce(
      () =>
        new Promise<string>((resolve) => {
          releaseInvoke = resolve
        }),
    )

    // 两个调用者同时进入（宿主前端实测：`loadAll()` 与授权弹窗挂载补拉）
    const both = Promise.all([fresh.ensureHostCredential(), fresh.ensureHostCredential()])
    releaseInvoke?.(LOADER_SESSION)
    const [first, second] = await both

    expect(first).toBe(LOADER_SESSION)
    expect(second).toBe(LOADER_SESSION)
    // 变异对照：去掉在途 Promise 共享 → 两个调用者各自 invoke，后到者会撞上宿主侧
    // 「首个调用者生效」的第二道闸（session already issued）→ 本断言转红
    expect(
      invokeMock.mock.calls.filter(([c]) => c === 'plugin_frontend_loader_session'),
      '并发调用必须共享同一次宿主命令',
    ).toHaveLength(1)
    // 落地后仍走缓存（不得每次调用都重新签发）
    expect(await fresh.ensureHostCredential()).toBe(LOADER_SESSION)
    expect(invokeMock.mock.calls.filter(([c]) => c === 'plugin_frontend_loader_session')).toHaveLength(1)
  })

  it('C5 取凭证失败不缓存：下一次调用重试（一次瞬时失败不得锁死整页）', async () => {
    vi.resetModules()
    const fresh = await import('@/plugin/commands')
    invokeMock.mockRejectedValueOnce(new Error('frontend loader session already issued for this page load'))

    await expect(fresh.ensureHostCredential()).rejects.toThrow('already issued')
    // 重试路径：宿主侧重置后（或首次撞闸后）仍能拿到凭证
    expect(await fresh.ensureHostCredential()).toBe(LOADER_SESSION)
    expect(invokeMock.mock.calls.filter(([c]) => c === 'plugin_frontend_loader_session')).toHaveLength(2)
  })
})
