/**
 * usePeerDevices 测试（issue 08）
 *
 * 覆盖编排逻辑（不测渲染）：发现快照拉取、事件驱动自动刷新、连接发起
 * （成功/拒绝/不可达）、无能力节点与重复操作的防御拦截、断开与事件摘除。
 * listen 经 mock 捕获处理器后手动派发事件载荷驱动。
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import {
  usePeerDevices,
  _resetPeerDevicesForTest,
  type DiscoveredPeer,
  type DialPeerResult,
} from '@/composables/usePeerDevices'

// Mock Tauri invoke / event
const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}))

type PayloadHandler = (event: { payload: any }) => void
const handlers: Record<string, PayloadHandler> = {}

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, handler: PayloadHandler) => {
    handlers[event] = handler
    return () => {}
  }),
}))

const NODE_A = 'aa'.repeat(32)
const NODE_B = 'bb'.repeat(32)

function makePeer(overrides: Partial<DiscoveredPeer> = {}): DiscoveredPeer {
  return {
    nodeId: NODE_A,
    deviceName: '张三的手机',
    addr: '192.168.1.10:47613',
    protocolVersion: 1,
    capabilities: 1,
    fileTransfer: true,
    ...overrides,
  }
}

/** 经捕获的 listen 处理器派发一条事件载荷 */
async function emit(event: string, payload: unknown) {
  const handler = handlers[event]
  if (!handler) throw new Error(`no captured handler for ${event}`)
  await handler({ payload })
}

describe('usePeerDevices flow', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    handlers['peer-devices-changed'] = undefined as unknown as PayloadHandler
    handlers['peer-connected'] = undefined as unknown as PayloadHandler
    handlers['peer-disconnected'] = undefined as unknown as PayloadHandler
    _resetPeerDevicesForTest()
  })

  it('start() registers listeners and pulls initial discovery snapshot', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_discovered_peers') return Promise.resolve([makePeer()])
      return Promise.resolve(undefined)
    })
    const devices = usePeerDevices()
    await devices.start()

    expect(mockInvoke).toHaveBeenCalledWith('list_discovered_peers')
    expect(devices.peers.value).toHaveLength(1)
    expect(devices.peers.value[0]!.deviceName).toBe('张三的手机')
  })

  it('peers-changed event replaces the list reactively', async () => {
    mockInvoke.mockResolvedValue([])
    const devices = usePeerDevices()
    await devices.start()

    await emit('peer-devices-changed', [
      makePeer(),
      makePeer({ nodeId: NODE_B, deviceName: '李四的电脑' }),
    ])

    expect(devices.peers.value).toHaveLength(2)
    expect(devices.peers.value[1]!.nodeId).toBe(NODE_B)
  })

  it('connect succeeds for capable node and marks connected state', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_discovered_peers') return Promise.resolve([makePeer()])
      if (cmd === 'dial_peer')
        return Promise.resolve<DialPeerResult>({ status: 'connected', deviceName: '张三的手机' })
      return Promise.resolve(undefined)
    })
    const devices = usePeerDevices()
    await devices.start()

    const status = await devices.connect(NODE_A)

    expect(status).toBe('connected')
    expect(mockInvoke).toHaveBeenCalledWith('dial_peer', { nodeId: NODE_A })
    expect(devices.connectedIds.value.has(NODE_A)).toBe(true)
    expect(devices.connectingIds.value.has(NODE_A)).toBe(false)
  })

  it('connect records denied/unreachable terminal states without connected badge', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_discovered_peers') return Promise.resolve([makePeer(), makePeer({ nodeId: NODE_B })])
      if (cmd === 'dial_peer')
        return Promise.resolve<DialPeerResult>({ status: 'denied', deviceName: '张三的手机' })
      return Promise.resolve(undefined)
    })
    const devices = usePeerDevices()
    await devices.start()

    const denied = await devices.connect(NODE_A)
    expect(denied).toBe('denied')
    expect(devices.dialErrors.value[NODE_A]).toBe('denied')
    expect(devices.connectedIds.value.has(NODE_A)).toBe(false)

    // 第二台节点：命令面异常按不可达呈现
    mockInvoke.mockRejectedValueOnce(new Error('node not started'))
    const unreachable = await devices.connect(NODE_B)
    expect(unreachable).toBe('unreachable')
    expect(devices.dialErrors.value[NODE_B]).toBe('unreachable')
  })

  it('refuses to dial incapable or undiscovered nodes', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_discovered_peers')
        return Promise.resolve([makePeer({ capabilities: 0, fileTransfer: false })])
      return Promise.resolve(undefined)
    })
    const devices = usePeerDevices()
    await devices.start()

    expect(await devices.connect(NODE_A)).toBeNull()
    expect(await devices.connect(NODE_B)).toBeNull()
    expect(mockInvoke).not.toHaveBeenCalledWith('dial_peer', { nodeId: NODE_A })
    expect(mockInvoke).not.toHaveBeenCalledWith('dial_peer', { nodeId: NODE_B })
  })

  it('guards against concurrent duplicate dials of the same node', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_discovered_peers') return Promise.resolve([makePeer()])
      if (cmd === 'dial_peer')
        return new Promise<DialPeerResult>((resolve) => {
          setTimeout(
            () => resolve({ status: 'connected', deviceName: '张三的手机' }),
            50,
          )
        })
      return Promise.resolve(undefined)
    })
    const devices = usePeerDevices()
    await devices.start()

    const first = devices.connect(NODE_A)
    // 拨号在途：同一节点的再次发起被拒绝且不触发第二次命令
    expect(devices.connectingIds.value.has(NODE_A)).toBe(true)
    expect(await devices.connect(NODE_A)).toBeNull()

    await vi.waitFor(() => expect(devices.connectedIds.value.has(NODE_A)).toBe(true))
    await first

    expect(mockInvoke).toHaveBeenCalledTimes(2)
    expect(mockInvoke).not.toHaveBeenCalledWith('dial_peer', { nodeId: NODE_B })
    expect(mockInvoke).toHaveBeenCalledWith('dial_peer', { nodeId: NODE_A })
  })

  it('peer-disconnected event removes the connected badge', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_discovered_peers') return Promise.resolve([makePeer()])
      if (cmd === 'dial_peer')
        return Promise.resolve<DialPeerResult>({ status: 'connected', deviceName: '张三的手机' })
      return Promise.resolve(undefined)
    })
    const devices = usePeerDevices()
    await devices.start()
    await devices.connect(NODE_A)
    expect(devices.connectedIds.value.has(NODE_A)).toBe(true)

    await emit('peer-disconnected', { nodeId: NODE_A })

    expect(devices.connectedIds.value.has(NODE_A)).toBe(false)
  })

  it('disconnect clears badge optimistically and calls the command', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_discovered_peers') return Promise.resolve([makePeer()])
      if (cmd === 'dial_peer')
        return Promise.resolve<DialPeerResult>({ status: 'connected', deviceName: '张三的手机' })
      if (cmd === 'disconnect_peer') return Promise.resolve(true)
      return Promise.resolve(undefined)
    })
    const devices = usePeerDevices()
    await devices.start()
    await devices.connect(NODE_A)

    await devices.disconnect(NODE_A)

    expect(mockInvoke).toHaveBeenCalledWith('disconnect_peer', { nodeId: NODE_A })
    expect(devices.connectedIds.value.has(NODE_A)).toBe(false)
  })

  it('successful connection clears previous dial error feedback', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_discovered_peers') return Promise.resolve([makePeer()])
      if (cmd === 'dial_peer') {
        return Promise.resolve<DialPeerResult>({ status: 'denied', deviceName: null })
      }
      return Promise.resolve(undefined)
    })
    const devices = usePeerDevices()
    await devices.start()

    await devices.connect(NODE_A)
    expect(devices.dialErrors.value[NODE_A]).toBe('denied')

    // 对端改为主意接受：重连成功后错误反馈清除
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'dial_peer')
        return Promise.resolve<DialPeerResult>({ status: 'connected', deviceName: '张三的手机' })
      return Promise.resolve(undefined)
    })
    await devices.connect(NODE_A)
    expect(devices.dialErrors.value[NODE_A]).toBeUndefined()
    expect(devices.connectedIds.value.has(NODE_A)).toBe(true)
  })
})
