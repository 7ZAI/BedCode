/**
 * useTrustedPeers 编排测试（ticket 05）
 *
 * mock 最小 PluginContext（commands.execute 记录调用），只测列表加载与撤销
 * 命令路由编排不测渲染。场景矩阵对齐 ticket 验收：列表加载成功归一化（畸形
 * 条目剔除）、加载失败如实置错误态而非空白、失败后重试恢复、撤销命令路由 +
 * 成功本地摘除、撤销失败保留条目并上报、纯函数（时间格式化）直测。
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { PluginContext } from '@binblink/plugin-sdk-desktop'
import {
  formatTrustedDate,
  normalizeTrustedPeers,
  useTrustedPeers,
} from '../../../../plugins/file-transfer/src/composables/useTrustedPeers'

const NODE_A = 'a'.repeat(32)
const NODE_B = 'b'.repeat(32)

type EventPayload = Record<string, any>
type EventHandler = (payload: EventPayload) => void

/** 最小 mock PluginContext：记录命令调用 + 受控命令响应 */
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
      on(_event: string, _handler: EventHandler) {
        return { dispose: () => {} }
      },
    },
  } as unknown as PluginContext

  function onCommand(id: string, respond: (args: any) => unknown): void {
    responders.set(id, respond)
  }

  return { context, calls, onCommand }
}

function makePeer(nodeId: string, overrides: Record<string, any> = {}) {
  return {
    nodeId,
    displayName: `设备-${nodeId.slice(0, 2)}`,
    fingerprintShort: nodeId.slice(0, 8),
    addedAt: '2026-08-01T10:30:00+08:00',
    ...overrides,
  }
}

describe('useTrustedPeers orchestration', () => {
  let env: ReturnType<typeof makeContext>

  beforeEach(() => {
    vi.clearAllMocks()
    env = makeContext()
  })

  it('refresh routes list-trusted and populates normalized peers', async () => {
    env.onCommand('file-transfer.list-trusted', () => [
      makePeer(NODE_A),
      { foo: 'malformed' }, // 畸形条目被剔除
      makePeer(NODE_B, { displayName: null }),
    ])
    const trusted = useTrustedPeers(env.context)

    await trusted.refresh()

    expect(env.calls).toContainEqual({
      id: 'file-transfer.list-trusted',
      args: {},
    })
    expect(trusted.loadState.value).toBe('ready')
    expect(trusted.errorKey.value).toBe('')
    expect(trusted.peers.value.map((p) => p.nodeId)).toEqual([NODE_A, NODE_B])
    // 无名条目短指纹兑底
    expect(trusted.peers.value[1]!.fingerprintShort).toBe(NODE_B.slice(0, 8))
  })

  it('load failure surfaces an error state (i18n key) instead of a blank list', async () => {
    env.onCommand('file-transfer.list-trusted', () =>
      Promise.reject(new Error('node not started')),
    )
    const trusted = useTrustedPeers(env.context)

    await trusted.refresh()

    expect(trusted.loadState.value).toBe('error')
    expect(trusted.peers.value).toEqual([])
    expect(trusted.errorKey.value).toBe('transfer.trusted.loadFailed')
  })

  it('recovers to ready state after a successful retry following a failure', async () => {
    let fail = true
    env.onCommand('file-transfer.list-trusted', () => {
      if (fail) return Promise.reject(new Error('boom'))
      return [makePeer(NODE_A)]
    })
    const trusted = useTrustedPeers(env.context)
    await trusted.refresh()
    expect(trusted.loadState.value).toBe('error')

    fail = false
    await trusted.refresh()

    expect(trusted.loadState.value).toBe('ready')
    expect(trusted.errorKey.value).toBe('')
    expect(trusted.peers.value).toHaveLength(1)
  })

  it('revoke routes the command and removes the entry locally on success', async () => {
    env.onCommand('file-transfer.list-trusted', () => [
      makePeer(NODE_A),
      makePeer(NODE_B),
    ])
    env.onCommand('file-transfer.revoke-trusted', () => true)
    const trusted = useTrustedPeers(env.context)
    await trusted.refresh()

    const ok = await trusted.revoke(NODE_A)

    expect(ok).toBe(true)
    expect(env.calls).toContainEqual({
      id: 'file-transfer.revoke-trusted',
      args: { nodeId: NODE_A },
    })
    expect(trusted.peers.value.map((p) => p.nodeId)).toEqual([NODE_B])
    expect(trusted.revokingIds.value.has(NODE_A)).toBe(false)
  })

  it('keeps the entry and reports an error key when revoke fails', async () => {
    env.onCommand('file-transfer.list-trusted', () => [makePeer(NODE_A)])
    env.onCommand('file-transfer.revoke-trusted', () =>
      Promise.reject(new Error('boom')),
    )
    const trusted = useTrustedPeers(env.context)
    await trusted.refresh()

    const ok = await trusted.revoke(NODE_A)

    expect(ok).toBe(false)
    expect(trusted.peers.value.map((p) => p.nodeId)).toEqual([NODE_A])
    expect(trusted.errorKey.value).toBe('transfer.trusted.revokeFailed')
    expect(trusted.revokingIds.value.has(NODE_A)).toBe(false)
  })
})

describe('normalizeTrustedPeers pure function', () => {
  it('returns an empty list for non-array input', () => {
    expect(normalizeTrustedPeers(null)).toEqual([])
    expect(normalizeTrustedPeers({})).toEqual([])
  })

  it('falls back fingerprintShort to nodeId prefix for malformed entries', () => {
    const peers = normalizeTrustedPeers([
      { nodeId: NODE_A, fingerprintShort: '', addedAt: null },
    ])
    expect(peers[0]!.fingerprintShort).toBe(NODE_A.slice(0, 8))
    expect(peers[0]!.addedAt).toBe('')
  })
})

describe('formatTrustedDate pure function', () => {
  it('formats a valid RFC3339 date for zh-CN locale', () => {
    const out = formatTrustedDate('2026-08-01T10:30:00+08:00', 'zh-CN')
    expect(out).toContain('2026')
    expect(out).not.toBe('2026-08-01T10:30:00+08:00')
  })

  it('falls back to the raw string for invalid dates and empty input', () => {
    expect(formatTrustedDate('not-a-date', 'zh-CN')).toBe('not-a-date')
    expect(formatTrustedDate('', 'en')).toBe('')
  })
})
