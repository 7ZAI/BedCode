/**
 * useSettings 编排测试（ticket 09 场景矩阵补齐）
 *
 * 承接被删宿主 usePeerReceiving 中接收策略设置场景（setPolicy 经后端校验后
 * 本地生效）与宿主接收设置页的插件化版本：wire 设置归一化、策略/超时命令
 * 路由与本地同步、超时钳制（10–600）、共享目录增删与下载目录选择。
 * mock 最小 PluginContext，只测编排逻辑不测渲染；addRoot 经 mount-local 命令
 * （系统多目录选择器在插件 WASM 侧打开）可 mock 编排，含多选添加与取消。
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import { useSettings } from '../../../../plugins/file-transfer/src/composables/useSettings'

function makeContext() {
  const calls: Array<{ id: string; args: any }> = []
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
      on() {
        return { dispose: () => {} }
      },
    },
  } as unknown as PluginContext

  function onCommand(id: string, respond: (args: any) => unknown): void {
    responders.set(id, respond)
  }

  return { context, calls, onCommand }
}

describe('useSettings orchestration', () => {
  let env: ReturnType<typeof makeContext>

  beforeEach(() => {
    vi.clearAllMocks()
    env = makeContext()
  })

  it('load normalizes wire settings (policy_mode / roots DTO / ask_timeout_sec)', async () => {
    env.onCommand('file-transfer.get-settings', () => ({
      policy_mode: 'always_accept',
      ask_timeout_sec: 45,
      download_dir: 'C:/Downloads',
      roots: [
        { id: 'r-1', name: '下载' },
        { id: '', path: 'D:/docs' }, // 无名条目回退 path
      ],
    }))
    const settings = useSettings(env.context)

    await settings.load()

    expect(env.calls).toContainEqual({ id: 'file-transfer.get-settings', args: {} })
    expect(settings.settings.value).toMatchObject({
      receivingPolicy: 'accept',
      approvalTimeoutSec: 45,
      downloadDir: 'C:/Downloads',
    })
    expect(settings.rootItems.value).toEqual([
      { id: 'r-1', name: '下载', path: '下载' }, // 无 path 条目：完整路径退化用 name
      { id: '', name: 'D:/docs', path: 'D:/docs' }, // 无名条目回退 path
    ])
    expect(settings.loading.value).toBe(false)
  })

  it('addRoot routes mount-local and reloads; multi-pick returns added paths', async () => {
    env.onCommand('file-transfer.get-settings', () => ({
      roots: [
        { id: 'r-a', name: 'a', path: '/tmp/a' },
        { id: 'r-b', name: 'b', path: '/media/b' },
      ],
    }))
    env.onCommand('file-transfer.mount-local', () => ({
      added: [
        { id: 'r-a', name: 'a', path: '/tmp/a' },
        { id: 'r-b', name: 'b', path: '/media/b' },
      ],
    }))
    const settings = useSettings(env.context)

    await expect(settings.addRoot()).resolves.toEqual(['/tmp/a', '/media/b'])
    expect(env.calls).toContainEqual({ id: 'file-transfer.mount-local', args: {} })
    // 结果经 load 刷新（含完整路径），不依赖返回值手工拼装
    expect(settings.rootItems.value.map((r) => r.path)).toEqual(['/tmp/a', '/media/b'])
  })

  it('addRoot treats user cancellation as empty result without error', async () => {
    env.onCommand('file-transfer.mount-local', () => Promise.reject(new Error('cancelled')))
    const settings = useSettings(env.context)

    await expect(settings.addRoot()).resolves.toEqual([])
  })

  it('load failure keeps defaults and clears loading without throwing', async () => {
    env.onCommand('file-transfer.get-settings', () => Promise.reject(new Error('boom')))
    const settings = useSettings(env.context)

    await expect(settings.load()).resolves.toBeUndefined()
    expect(settings.settings.value.receivingPolicy).toBe('ask')
    expect(settings.loading.value).toBe(false)
  })

  it('setReceivingPolicy routes set-settings and syncs local state on success', async () => {
    env.onCommand('file-transfer.set-settings', () => true)
    const settings = useSettings(env.context)

    await settings.setReceivingPolicy('reject')

    expect(env.calls).toContainEqual({
      id: 'file-transfer.set-settings',
      args: { receivingPolicy: 'reject', approvalTimeoutSec: 60 },
    })
    expect(settings.settings.value.receivingPolicy).toBe('reject')
  })

  it('setApprovalTimeoutSec clamps into the 10-600 window', async () => {
    env.onCommand('file-transfer.set-settings', () => true)
    const settings = useSettings(env.context)

    await settings.setApprovalTimeoutSec(5)
    expect(env.calls.at(-1)!.args).toEqual({ receivingPolicy: 'ask', approvalTimeoutSec: 10 })

    await settings.setApprovalTimeoutSec(1000.6)
    expect(env.calls.at(-1)!.args).toEqual({ receivingPolicy: 'ask', approvalTimeoutSec: 600 })
    expect(settings.settings.value.approvalTimeoutSec).toBe(600)
  })

  it('setEncryption routes set-settings and syncs local state; load normalizes missing flag to false', async () => {
    env.onCommand('file-transfer.get-settings', () => ({ policy_mode: 'ask' }))
    env.onCommand('file-transfer.set-settings', () => true)
    const settings = useSettings(env.context)

    // 缺省/旧宿主响应无 encryption 字段 → 归一化为 false
    await settings.load()
    expect(settings.settings.value.encryption).toBe(false)

    await settings.setEncryption(true)

    expect(env.calls).toContainEqual({
      id: 'file-transfer.set-settings',
      args: { encryption: true },
    })
    expect(settings.settings.value.encryption).toBe(true)
  })

  it('removeRoot routes update-roots with the entry id and prunes locally', async () => {
    env.onCommand('file-transfer.get-settings', () => ({
      roots: [
        { id: 'r-1', name: 'A' },
        { id: 'r-2', name: 'B' },
      ],
    }))
    env.onCommand('file-transfer.update-roots', () => ({ removed: true }))
    const settings = useSettings(env.context)
    await settings.load()

    await settings.removeRoot('r-1')

    expect(env.calls).toContainEqual({ id: 'file-transfer.update-roots', args: { remove: 'r-1' } })
    expect(settings.rootItems.value.map((r) => r.id)).toEqual(['r-2'])
  })

  it('pickDownloadDir applies the picked path; cancelled and failure keep state', async () => {
    env.onCommand('file-transfer.pick-download-dir', () => ({ path: 'E:/media' }))
    const settings = useSettings(env.context)

    const picked = await settings.pickDownloadDir()
    expect(picked).toBe('E:/media')
    expect(settings.settings.value.downloadDir).toBe('E:/media')

    env.onCommand('file-transfer.pick-download-dir', () => ({ cancelled: true }))
    expect(await settings.pickDownloadDir()).toBeNull()

    env.onCommand('file-transfer.pick-download-dir', () => Promise.reject(new Error('boom')))
    expect(await settings.pickDownloadDir()).toBeNull()
    // 失败不污染既有状态
    expect(settings.settings.value.downloadDir).toBe('E:/media')
  })
})
