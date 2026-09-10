/**
 * usePeerDevices 编排测试（issue 13 Phase 3 自建设备缓存版）
 *
 * 场景矩阵：快照恢复首屏（restored → recent 标注）、mdns-found upsert
 * （TXT/cap 位解读 + last-seen 盖章）、畸形载荷丢弃、mdns-lost 按短指纹移除、
 * connect 显式 endpoint 拨号与 denied/unreachable 行内错误、并发拨号防护、
 * 断开乐观摘除、TTL 惰性清扫（refresh 触发）、生命周期幂等。
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import { usePeerDevices } from '../../../../plugins/file-transfer/src/composables/usePeerDevices'
import { DEVICE_TTL_MS } from '../../../../plugins/file-transfer/src/composables/deviceState'

type EventHandler = (payload: any) => void

const NODE_A = 'a'.repeat(64)
const NODE_B = 'b'.repeat(64)

function makeContext() {
  const calls: Array<{ id: string; args: any }> = []
  const handlers = new Map<string, EventHandler[]>()
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
      on(event: string, handler: EventHandler) {
        const list = handlers.get(event) ?? []
        list.push(handler)
        handlers.set(event, list)
        return {
          dispose: () =>
            handlers.set(
              event,
              (handlers.get(event) ?? []).filter((h) => h !== handler),
            ),
        }
      },
    },
  } as unknown as PluginContext

  function emit(event: string, payload: unknown): void {
    const list = handlers.get(event)
    if (!list?.length) throw new Error(`no captured handler for ${event}`)
    for (const handler of list) handler(payload)
  }

  function onCommand(id: string, respond: (args: any) => unknown): void {
    responders.set(id, respond)
  }

  function listenerCount(event: string): number {
    return handlers.get(event)?.length ?? 0
  }

  const flush = () => new Promise((r) => setTimeout(r, 0))

  return { context, calls, emit, onCommand, listenerCount, flush }
}

/** mdns:found 载荷形状（引擎透传 wire） */
function makeFound(nodeId: string, name: string, overrides: Record<string, any> = {}) {
  return {
    instanceName: `bedcode-peer-${nodeId.slice(0, 8)}._bedcode-peer._tcp.local.`,
    addresses: ['192.168.1.10'],
    port: 47821,
    txtRecords: { id: nodeId, name, ver: '1', cap: '1' },
    ...overrides,
  }
}

describe('usePeerDevices orchestration (self-built cache)', () => {
  let env: ReturnType<typeof makeContext>

  beforeEach(() => {
    vi.clearAllMocks()
    env = makeContext()
    env.onCommand('file-transfer.get-device-snapshot', () => ({ devices: [] }))
    env.onCommand('file-transfer.save-device-snapshot', () => ({ ok: true }))
    env.onCommand('file-transfer.set-active-peer', () => ({ ok: true }))
  })

  it('start is idempotent and restores snapshot entries annotated recent', async () => {
    env.onCommand('file-transfer.get-device-snapshot', () => ({
      devices: [
        {
          nodeId: NODE_A,
          deviceName: '设备-a',
          addr: '192.168.1.10',
          port: 47821,
          capabilitiesHex: '1',
          lastSeenMs: Date.now() - 60_000,
        },
      ],
    }))
    const dev = usePeerDevices(env.context)

    dev.start()
    dev.start()
    await new Promise((r) => setTimeout(r, 5))
    await dev.refresh()

    expect(env.listenerCount('plugin:file-transfer:mdns-found')).toBe(1)
    // 快照恢复条目：未连接 → recent 标注（「最近可见」），lastSeen 新鲜故未被清扫
    expect(dev.rows.value).toHaveLength(1)
    expect(dev.rows.value[0]).toMatchObject({
      nodeId: NODE_A,
      deviceName: '设备-a',
      status: 'idle',
      recent: true,
      fileTransfer: true,
    })
  })

  it('mdns-found upserts with cap-bit parsing and stamps last-seen', async () => {
    const dev = usePeerDevices(env.context)
    dev.start()
    await env.flush()

    env.emit('plugin:file-transfer:mdns-found', makeFound(NODE_A, '设备-a'))
    env.emit(
      'plugin:file-transfer:mdns-found',
      makeFound(NODE_B, '无能力-b', { txtRecords: { id: NODE_B, cap: '0' } }),
    )
    // 畸形载荷：id 非 64 位 hex → 丢弃
    env.emit('plugin:file-transfer:mdns-found', makeFound('zz', '坏数据'))
    await dev.refresh()

    expect(dev.devices.value).toHaveLength(2)
    const [a, b] = dev.rows.value
    expect(a).toMatchObject({ nodeId: NODE_A, fileTransfer: true, status: 'idle' })
    expect(b?.fileTransfer).toBe(false)
    // found 盖章后 restored 清除，recent 不再标注
    expect(b?.recent).toBe(false)
    // 快照落盘被 debounce 调度（含两台设备）
    await new Promise((r) => setTimeout(r, 2100))
    expect(env.calls.some((c) => c.id === 'file-transfer.save-device-snapshot')).toBe(true)
  })

  it('mdns-lost removes by instance short fingerprint', async () => {
    const dev = usePeerDevices(env.context)
    dev.start()
    await env.flush()
    env.emit('plugin:file-transfer:mdns-found', makeFound(NODE_A, '设备-a'))

    env.emit('plugin:file-transfer:mdns-lost', {
      instanceName: `bedcode-peer-${NODE_A.slice(0, 8)}._bedcode-peer._tcp.local.`,
    })

    expect(dev.devices.value).toHaveLength(0)
  })

  it('connect routes dial-peer with explicit endpoint and marks connected', async () => {
    env.onCommand('file-transfer.dial-peer', () => ({ status: 'connected' }))
    const dev = usePeerDevices(env.context)
    dev.start()
    await env.flush()
    env.emit('plugin:file-transfer:mdns-found', makeFound(NODE_A, '设备-a'))

    const status = await dev.connect(NODE_A)

    expect(status).toBe('connected')
    expect(env.calls).toContainEqual({
      id: 'file-transfer.dial-peer',
      args: { endpoint: { nodeId: NODE_A, addr: '192.168.1.10', port: 47821 } },
    })
    expect(dev.rows.value[0]?.status).toBe('connected')
  })

  it('denied / unreachable errors land inline without connected badge', async () => {
    env.onCommand('file-transfer.dial-peer', (args) =>
      String(args?.endpoint?.nodeId) === NODE_A
        ? Promise.reject(new Error('dial endpoint failed: peer denied'))
        : Promise.reject(new Error('dial endpoint failed: peer unreachable')),
    )
    const dev = usePeerDevices(env.context)
    dev.start()
    await env.flush()
    env.emit('plugin:file-transfer:mdns-found', makeFound(NODE_A, 'a'))
    env.emit('plugin:file-transfer:mdns-found', makeFound(NODE_B, 'b'))

    expect(await dev.connect(NODE_A)).toBe('denied')
    expect(await dev.connect(NODE_B)).toBe('unreachable')

    const rows = Object.fromEntries(dev.rows.value.map((r) => [r.nodeId, r]))
    expect(rows[NODE_A]?.dialError).toBe('denied')
    expect(rows[NODE_B]?.dialError).toBe('unreachable')
    expect(rows[NODE_A]?.status).toBe('idle')
  })

  it('refuses to dial undiscovered / incapable nodes without routing commands', async () => {
    const dev = usePeerDevices(env.context)
    dev.start()
    await env.flush()
    env.emit(
      'plugin:file-transfer:mdns-found',
      makeFound(NODE_B, '无能力', { txtRecords: { id: NODE_B, cap: '0' } }),
    )

    expect(await dev.connect('f'.repeat(64))).toBeNull() // 未发现
    expect(await dev.connect(NODE_B)).toBeNull() // 无能力（面板置灰不可点，防御拦截）
    expect(env.calls.some((c) => c.id === 'file-transfer.dial-peer')).toBe(false)
  })

  it('guards against concurrent duplicate dials of the same node', async () => {
    let resolving: (() => void) | null = null
    env.onCommand('file-transfer.dial-peer', () => {
      return new Promise((resolve) => {
        resolving = () => resolve({ status: 'connected' })
      })
    })
    const dev = usePeerDevices(env.context)
    dev.start()
    await env.flush()
    env.emit('plugin:file-transfer:mdns-found', makeFound(NODE_A, 'a'))

    const first = dev.connect(NODE_A)
    const second = await dev.connect(NODE_A)
    expect(second).toBeNull()
    resolving?.()
    expect(await first).toBe('connected')
  })

  it('disconnect removes optimistically then routes the command', async () => {
    env.onCommand('file-transfer.disconnect-peer', () => ({ existed: true }))
    env.onCommand('file-transfer.dial-peer', () => ({ status: 'connected' }))
    const dev = usePeerDevices(env.context)
    dev.start()
    await env.flush()
    env.emit('plugin:file-transfer:mdns-found', makeFound(NODE_A, 'a'))
    await dev.connect(NODE_A)
    expect(dev.rows.value[0]?.status).toBe('connected')

    await dev.disconnect(NODE_A)

    expect(dev.rows.value[0]?.status).toBe('idle')
    expect(env.calls).toContainEqual({ id: 'file-transfer.disconnect-peer', args: { nodeId: NODE_A } })
  })

  it('connection-changed maintains connected set and active fallback', async () => {
    env.onCommand('file-transfer.dial-peer', () => ({ status: 'connected' }))
    const dev = usePeerDevices(env.context)
    dev.start()
    await env.flush()
    env.emit('plugin:file-transfer:mdns-found', makeFound(NODE_A, 'a'))
    env.emit('plugin:file-transfer:mdns-found', makeFound(NODE_B, 'b'))
    await dev.connect(NODE_A)
    await dev.switchPeer(NODE_A)
    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_B, connected: true })
    expect(dev.peer.value.online).toBe(true)

    // 活跃对端断连：有其他候选时乐观切换
    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_A, connected: false })
    expect(dev.activePeerId.value).toBe(NODE_B)

    // 唯一候选也断连：活跃清空
    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_B, connected: false })
    expect(dev.activePeerId.value).toBe('')
  })

  it('connection-changed without explicit connected flag is treated as disconnect (host must always send the field)', () => {
    const dev = usePeerDevices(env.context)
    dev.start()
    // 契约回归锁：缺 connected 字段按断开处理——宿主拨号/入站/刷新重发/断开
    // 四路事件都必须携带该字段，漏发会把已连接状态回滚成未连接
    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_A, connected: true })
    expect(dev.connectedIds.value.has(NODE_A)).toBe(true)
    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_A })
    expect(dev.connectedIds.value.has(NODE_A)).toBe(false)
  })

  it('inbound connected event registers the peer endpoint for data-plane redial', () => {
    const dev = usePeerDevices(env.context)
    dev.start()
    env.emit('plugin:file-transfer:mdns-found', makeFound(NODE_A, 'a'))
    // 被连侧没有拨号 memo：连接建立时用自建缓存的 addr/port 回填，
    // 否则浏览/收发数据面命令报 no endpoint known for peer
    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_A, connected: true })
    expect(
      env.calls.some((c) => c.id === 'file-transfer.remember-peer-endpoint'),
    ).toBe(true)
    expect(
      env.calls.find((c) => c.id === 'file-transfer.remember-peer-endpoint')!.args,
    ).toEqual({
      endpoint: { nodeId: NODE_A, addr: '192.168.1.10', port: 47821 },
    })
  })

  it('refresh sweeps TTL-expired idle entries but keeps connected ones', async () => {
    const dev = usePeerDevices(env.context)
    dev.start()
    await env.flush()
    const expired = Date.now() - DEVICE_TTL_MS - 1000
    env.emit('plugin:file-transfer:mdns-found', makeFound(NODE_A, '过期'))
    // 手工把 lastSeen 拨回过期时刻并标记已恢复（模拟陈旧快照条目）
    dev.devices.value = dev.devices.value.map((d) => ({
      ...d,
      lastSeenMs: expired,
      restored: true,
    }))
    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_A, connected: true })

    await dev.refresh()

    // 已连接条目不被清扫；断开后同一条目因 lastSeen 过期被清算
    expect(dev.devices.value).toHaveLength(1)
    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_A, connected: false })
    await dev.refresh()
    expect(dev.devices.value).toHaveLength(0)
  })
})
