/**
 * usePeerDevices 编排测试（ticket 02）
 *
 * mock 最小 PluginContext（commands.execute 记录调用 + events.on 捕获处理器
 * 手动派发），只测编排逻辑不测渲染。场景矩阵：start 幂等、发现快照事件整表
 * 替换、活跃兜底切换（保持当前选择 + 空选时兜底）、连接/断开/切换活跃命令
 * 路由、连接中防重复发起、denied/unreachable 如实上报、WS 控制面与对等连接
 * 态语义隔离。
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { PluginContext } from '@binblink/plugin-sdk-mobile'
import { usePeerDevices } from '../../../../plugins/file-transfer/src/composables/usePeerDevices'

const NODE_A = 'a'.repeat(32)
const NODE_B = 'b'.repeat(32)
const NODE_C = 'c'.repeat(32)

type EventPayload = Record<string, any>
type EventHandler = (payload: EventPayload) => void

/** 最小 mock PluginContext：记录命令调用 + 捕获事件处理器 */
function makeContext() {
  const calls: Array<{ id: string; args: any }> = []
  const handlers = new Map<string, EventHandler>()
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
        handlers.set(event, handler)
        return { dispose: () => handlers.delete(event) }
      },
    },
  } as unknown as PluginContext

  function emit(event: string, payload: EventPayload): void {
    const handler = handlers.get(event)
    if (!handler) throw new Error(`no captured handler for ${event}`)
    handler(payload)
  }

  function onCommand(id: string, respond: (args: any) => unknown): void {
    responders.set(id, respond)
  }

  return { context, calls, emit, onCommand }
}

function makeDevice(nodeId: string, overrides: Record<string, any> = {}) {
  return {
    nodeId,
    deviceName: `设备-${nodeId.slice(0, 2)}`,
    addr: '192.168.1.10:47613',
    fileTransfer: true,
    ...overrides,
  }
}

describe('usePeerDevices orchestration', () => {
  let env: ReturnType<typeof makeContext>

  beforeEach(() => {
    vi.clearAllMocks()
    env = makeContext()
    // 默认命令响应：空发现快照 + 无活跃对端
    env.onCommand('file-transfer.query-peer', () => [])
    env.onCommand('file-transfer.list-peers', () => ({ peers: [], activePeerId: '' }))
    env.onCommand('file-transfer.set-active-peer', () => ({ ok: true }))
  })

  it('start() is idempotent: subscriptions and initial refresh happen once', () => {
    const devices = usePeerDevices(env.context)
    devices.start()
    devices.start()

    expect(
      env.calls.filter((c) => c.id === 'file-transfer.query-peer'),
    ).toHaveLength(1)
    // 二次 start 后事件订阅仍在（未被重复注册覆盖丢失）
    expect(() => env.emit('plugin:file-transfer:devices-changed', [])).not.toThrow()
  })

  it('refresh() pulls discovery snapshot and legacy active peer', async () => {
    env.onCommand('file-transfer.query-peer', () => [makeDevice(NODE_A)])
    env.onCommand('file-transfer.list-peers', () => ({
      peers: [{ deviceId: NODE_A, name: '设备-aa' }],
      activePeerId: NODE_A,
    }))
    const devices = usePeerDevices(env.context)

    await devices.refresh()

    expect(devices.devices.value).toHaveLength(1)
    expect(devices.activePeerId.value).toBe(NODE_A)
  })

  it('devices-changed event replaces the snapshot wholesale (incl. non-capable rows)', () => {
    const devices = usePeerDevices(env.context)
    devices.start()

    env.emit('plugin:file-transfer:devices-changed', [
      makeDevice(NODE_A),
      makeDevice(NODE_B, { fileTransfer: false }),
      { foo: 'malformed' }, // 畸形条目被过滤
    ])

    expect(devices.devices.value.map((d) => d.nodeId)).toEqual([NODE_A, NODE_B])
    const rowB = devices.rows.value.find((r) => r.nodeId === NODE_B)!
    expect(rowB.fileTransfer).toBe(false)
    expect(rowB.status).toBe('idle')
  })

  it('keeps the current active selection when it stays connected (no auto override)', async () => {
    env.onCommand('file-transfer.query-peer', () => [
      makeDevice(NODE_A),
      makeDevice(NODE_B),
    ])
    const devices = usePeerDevices(env.context)
    devices.start()
    await vi.waitFor(() => expect(devices.devices.value).toHaveLength(2))

    // A 已连接并成为活跃；B 随后也连上
    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_A, connected: true })
    expect(devices.activePeerId.value).toBe(NODE_A)

    env.emit('plugin:file-transfer:devices-changed', [
      makeDevice(NODE_A),
      makeDevice(NODE_B),
    ])
    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_B, connected: true })

    // 收敛后的兜底策略：保持当前选择，不被「首个可用设备」覆盖；
    // 仅剩 A 接管活跃时那一次 set-active-peer 调用
    expect(devices.activePeerId.value).toBe(NODE_A)
    expect(
      env.calls.filter((c) => c.id === 'file-transfer.set-active-peer'),
    ).toHaveLength(1)
  })

  it('active peer offline fallback switches to first connected capable node via command', async () => {
    env.onCommand('file-transfer.query-peer', () => [
      makeDevice(NODE_A),
      makeDevice(NODE_B),
    ])
    env.onCommand('file-transfer.list-peers', () => ({ peers: [], activePeerId: NODE_C }))
    const devices = usePeerDevices(env.context)
    devices.start()
    // start 内部异步 refresh：等活跃对端从 list-peers 拉取完成
    await vi.waitFor(() => expect(devices.activePeerId.value).toBe(NODE_C))

    // A/B 已连接且可传输，活跃 C 不在已连接集合 → 兜底切到首个候选
    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_A, connected: true })

    expect(devices.connectedIds.value.has(NODE_A)).toBe(true)
    expect(devices.activePeerId.value).toBe(NODE_A)
    expect(env.calls).toContainEqual({
      id: 'file-transfer.set-active-peer',
      args: { peerId: NODE_A },
    })
  })

  it('connect routes dial-peer command and marks connected on success', async () => {
    env.onCommand('file-transfer.query-peer', () => [makeDevice(NODE_A)])
    env.onCommand('file-transfer.dial-peer', () => ({
      status: 'connected',
      deviceName: '设备-aa',
    }))
    const devices = usePeerDevices(env.context)
    await devices.refresh()

    const status = await devices.connect(NODE_A)

    expect(status).toBe('connected')
    expect(env.calls).toContainEqual({ id: 'file-transfer.dial-peer', args: { nodeId: NODE_A } })
    expect(devices.connectedIds.value.has(NODE_A)).toBe(true)
    expect(devices.connectingIds.value.has(NODE_A)).toBe(false)
    // 活跃缺位 → 新连接自动接管
    expect(devices.activePeerId.value).toBe(NODE_A)
  })

  it('records denied / unreachable terminal states as inline errors without connected badge', async () => {
    env.onCommand('file-transfer.query-peer', () => [
      makeDevice(NODE_A),
      makeDevice(NODE_B),
    ])
    env.onCommand('file-transfer.dial-peer', (args) =>
      args?.nodeId === NODE_A
        ? { status: 'denied', deviceName: '设备-aa' }
        : Promise.reject(new Error('node not started')),
    )
    const devices = usePeerDevices(env.context)
    await devices.refresh()

    expect(await devices.connect(NODE_A)).toBe('denied')
    expect(devices.rows.value.find((r) => r.nodeId === NODE_A)!.dialError).toBe('denied')
    expect(devices.connectedIds.value.has(NODE_A)).toBe(false)

    // 命令面异常按不可达呈现（失败如实上报）
    expect(await devices.connect(NODE_B)).toBe('unreachable')
    expect(devices.rows.value.find((r) => r.nodeId === NODE_B)!.dialError).toBe('unreachable')
  })

  it('refuses to dial undiscovered / incapable / already-connected nodes', async () => {
    env.onCommand('file-transfer.query-peer', () => [
      makeDevice(NODE_A, { fileTransfer: false }),
      makeDevice(NODE_B),
    ])
    const devices = usePeerDevices(env.context)
    devices.start()
    await vi.waitFor(() => expect(devices.devices.value).toHaveLength(2))
    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_B, connected: true })

    expect(await devices.connect(NODE_A)).toBeNull() // 无能力
    expect(await devices.connect(NODE_C)).toBeNull() // 未发现
    expect(await devices.connect(NODE_B)).toBeNull() // 已连接
    expect(env.calls.filter((c) => c.id === 'file-transfer.dial-peer')).toHaveLength(0)
  })

  it('guards against concurrent duplicate dials of the same node', async () => {
    env.onCommand('file-transfer.query-peer', () => [makeDevice(NODE_A)])
    let resolveDial!: (v: unknown) => void
    env.onCommand('file-transfer.dial-peer', () => new Promise((r) => (resolveDial = r)))
    const devices = usePeerDevices(env.context)
    await devices.refresh()

    const pending = devices.connect(NODE_A)
    expect(devices.connectingIds.value.has(NODE_A)).toBe(true)
    // 握手在途：再次发起被拒绝且不触发第二次命令
    expect(await devices.connect(NODE_A)).toBeNull()

    resolveDial({ status: 'connected', deviceName: '设备-aa' })
    expect(await pending).toBe('connected')
    expect(env.calls.filter((c) => c.id === 'file-transfer.dial-peer')).toHaveLength(1)
  })

  it('disconnect routes the command and removes the badge optimistically', async () => {
    env.onCommand('file-transfer.query-peer', () => [makeDevice(NODE_A)])
    env.onCommand('file-transfer.dial-peer', () => ({ status: 'connected', deviceName: null }))
    env.onCommand('file-transfer.disconnect-peer', () => true)
    const devices = usePeerDevices(env.context)
    await devices.refresh()
    await devices.connect(NODE_A)

    await devices.disconnect(NODE_A)

    expect(devices.connectedIds.value.has(NODE_A)).toBe(false)
    expect(devices.activePeerId.value).toBe('')
    expect(env.calls).toContainEqual({
      id: 'file-transfer.disconnect-peer',
      args: { nodeId: NODE_A },
    })
  })

  it('connection-changed event maintains connected set and clears stale dial errors', () => {
    const devices = usePeerDevices(env.context)
    devices.start()

    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_A, connected: true })
    expect(devices.connectedIds.value.has(NODE_A)).toBe(true)

    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_A, connected: false })
    expect(devices.connectedIds.value.has(NODE_A)).toBe(false)
  })

  it('keeps WS control-plane state (connOnline) separate from peer connection state', () => {
    const devices = usePeerDevices(env.context)
    devices.start()

    // WS 控制面在线 ≠ 对等传输连接：互不污染
    env.emit('device-connected', { device_id: NODE_A, device_name: '设备-aa' })
    expect(devices.connOnline.value).toBe(true)
    expect(devices.connectedIds.value.size).toBe(0)

    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_A, connected: true })
    expect(devices.connectedIds.value.size).toBe(1)
    expect(devices.connOnline.value).toBe(true)

    env.emit('device-disconnected', {})
    expect(devices.connOnline.value).toBe(false)
    expect(devices.connectedIds.value.has(NODE_A)).toBe(true)
  })

  it('switchPeer routes set-active-peer command and updates local state', async () => {
    env.onCommand('file-transfer.query-peer', () => [
      makeDevice(NODE_A),
      makeDevice(NODE_B),
    ])
    const devices = usePeerDevices(env.context)
    await devices.refresh()

    const ok = await devices.switchPeer(NODE_B)

    expect(ok).toBe(true)
    expect(devices.activePeerId.value).toBe(NODE_B)
    expect(devices.peer.value).toMatchObject({ id: NODE_B, name: '设备-bb' })
    expect(env.calls).toContainEqual({
      id: 'file-transfer.set-active-peer',
      args: { peerId: NODE_B },
    })
  })
})
