/**
 * useSettings 编排测试（ticket 09 场景矩阵补齐）
 *
 * 承接被删宿主 usePeerReceiving 中接收策略设置场景（setPolicy 经后端校验后
 * 本地生效）与宿主接收设置页的插件化版本：wire 设置归一化（builtin 条目、
 * 策略枚举映射）、共享目录增删结果分类（SAF 选择器取消/不支持/失败）、
 * builtin 条目宿主拒绝移除时如实上报、策略与超时命令路由及钳制。
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { PluginContext } from '@binblink/plugin-sdk-mobile'
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

  it('load normalizes wire settings incl. builtin root kind and policy mapping', async () => {
    env.onCommand('file-transfer.get-settings', () => ({
      policy_mode: 'always_deny',
      ask_timeout_sec: 90,
      download_dir: 'MediaStore/Downloads',
      roots: [
        { id: 'builtin-dl', name: '下载', builtin: true },
        { id: 'r-1', name: '文档', tree_uri: 'content://docs' },
      ],
    }))
    const settings = useSettings(env.context)

    await settings.load()

    expect(settings.settings.value).toMatchObject({
      receivingPolicy: 'reject',
      approvalTimeoutSec: 90,
      downloadDir: 'MediaStore/Downloads',
      concurrency: 1, // 移动端并发固定
    })
    expect(settings.settings.value.roots[0]).toMatchObject({ kind: 'private_downloads', authorized: true })
    expect(settings.settings.value.roots[1]).toMatchObject({
      kind: 'saf',
      documentId: 'content://docs',
    })
    expect(settings.loading.value).toBe(false)
  })

  it('addRoot classifies cancelled / unsupported / failed outcomes without reloading on failure', async () => {
    const settings = useSettings(env.context)
    env.onCommand('file-transfer.get-settings', () => ({ roots: [] }))
    env.onCommand('file-transfer.mount-local', () => Promise.reject(new Error('user cancelled')))

    expect(await settings.addRoot()).toBe('cancelled')

    env.onCommand('file-transfer.mount-local', () =>
      Promise.reject(new Error('SAF picker not supported')),
    )
    expect(await settings.addRoot()).toBe('unsupported')
    // 失败路径不触发 reload
    expect(env.calls.some((c) => c.id === 'file-transfer.get-settings')).toBe(false)

    env.onCommand('file-transfer.mount-local', () => Promise.reject(new Error('boom')))
    expect(await settings.addRoot()).toBe('failed')

    env.onCommand('file-transfer.mount-local', () => true)
    expect(await settings.addRoot()).toBe('ok')
    expect(env.calls.some((c) => c.id === 'file-transfer.get-settings')).toBe(true)
  })

  it('removeRoot reports false when the host refuses (builtin) and prunes locally on success', async () => {
    const settings = useSettings(env.context)
    settings.settings.value = {
      ...settings.settings.value,
      roots: [
        { id: 'builtin-dl', kind: 'private_downloads', name: '下载', documentId: '', authorized: true },
        { id: 'r-1', kind: 'saf', name: '文档', documentId: 'content://docs', authorized: true },
      ],
    }
    env.onCommand('file-transfer.update-roots', (_args: any) =>
      _args?.remove === 'builtin-dl' ? { removed: false } : { removed: true },
    )

    expect(await settings.removeRoot('builtin-dl')).toBe(false)
    expect(settings.settings.value.roots).toHaveLength(2) // 宿主拒绝 → 本地不摘

    expect(await settings.removeRoot('r-1')).toBe(true)
    expect(env.calls).toContainEqual({ id: 'file-transfer.update-roots', args: { remove: 'r-1' } })
    expect(settings.settings.value.roots.map((r) => r.id)).toEqual(['builtin-dl'])
  })

  it('removeRoot returns false when the command rejects', async () => {
    env.onCommand('file-transfer.update-roots', () => Promise.reject(new Error('boom')))
    const settings = useSettings(env.context)

    expect(await settings.removeRoot('r-1')).toBe(false)
  })

  it('setReceivingPolicy routes set-settings and syncs local state; failure keeps old value', async () => {
    const settings = useSettings(env.context)
    env.onCommand('file-transfer.set-settings', () => true)

    expect(await settings.setReceivingPolicy('accept')).toBe(true)
    expect(env.calls.at(-1)!.args).toEqual({ receivingPolicy: 'accept', approvalTimeoutSec: 60 })
    expect(settings.settings.value.receivingPolicy).toBe('accept')

    env.onCommand('file-transfer.set-settings', () => Promise.reject(new Error('boom')))
    expect(await settings.setReceivingPolicy('reject')).toBe(false)
    expect(settings.settings.value.receivingPolicy).toBe('accept') // 失败不乐观更新
  })

  it('setApprovalTimeout clamps into the 10-600 window and rounds fractional input', async () => {
    const settings = useSettings(env.context)
    env.onCommand('file-transfer.set-settings', () => true)

    await settings.setApprovalTimeout(5)
    expect(env.calls.at(-1)!.args).toEqual({ receivingPolicy: 'ask', approvalTimeoutSec: 10 })

    await settings.setApprovalTimeout(1000.6)
    expect(env.calls.at(-1)!.args).toEqual({ receivingPolicy: 'ask', approvalTimeoutSec: 600 })
    expect(settings.settings.value.approvalTimeoutSec).toBe(600)

    await settings.setApprovalTimeout(45.4)
    expect(settings.settings.value.approvalTimeoutSec).toBe(45)
  })
})
