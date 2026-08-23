/**
 * usePeerReceiving 测试（issue 10，移动端镜像）
 *
 * 覆盖编排逻辑（不测渲染）：接收快照拉取与事件替换、询问批选取（最早优先）、
 * 倒计时口径、应答/取消/策略命令回流。listen 经 mock 捕获处理器后手动派发。
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import {
  usePeerReceiving,
  _resetPeerReceivingForTest,
  offerDeadline,
  remainingSeconds,
  pickCurrentOffer,
  type PeerReceiveTask,
} from '@/composables/usePeerReceiving'

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

function makeTask(overrides: Partial<PeerReceiveTask> = {}): PeerReceiveTask {
  return {
    batchId: 'b-1',
    nodeId: NODE_A,
    peerName: '李四的桌面',
    direction: 'receive',
    status: 'pending',
    files: [{ path: 'docs/report.pdf', size: 20 }],
    totalBytes: 20,
    transferredBytes: 0,
    rateBps: 0,
    detail: null,
    rejectReason: null,
    createdAtMs: 1000,
    updatedAtMs: 1000,
    ...overrides,
  }
}

async function emit(event: string, payload: unknown) {
  const handler = handlers[event]
  if (!handler) throw new Error(`no captured handler for ${event}`)
  await handler({ payload })
}

describe('usePeerReceiving pure helpers', () => {
  it('offerDeadline adds the ask window in seconds', () => {
    expect(offerDeadline(1000, 60)).toBe(61_000)
  })

  it('remainingSeconds is ceil-based and clamps at zero', () => {
    expect(remainingSeconds(61_000, 60_000)).toBe(1)
    expect(remainingSeconds(59_999, 60_000)).toBe(0)
    expect(remainingSeconds(0, 60_000)).toBe(0)
  })

  it('pickCurrentOffer returns the earliest pending batch only', () => {
    const later = makeTask({ batchId: 'b-later', createdAtMs: 2000 })
    const earlier = makeTask({ batchId: 'b-earlier', createdAtMs: 1000 })
    const running = makeTask({ batchId: 'b-running', status: 'running', createdAtMs: 500 })

    expect(pickCurrentOffer([])).toBeNull()
    expect(pickCurrentOffer([running])).toBeNull()
    expect(pickCurrentOffer([later, earlier])?.batchId).toBe('b-earlier')
  })
})

describe('usePeerReceiving flow', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    handlers['peer-receive-changed'] = undefined as unknown as PayloadHandler
    _resetPeerReceivingForTest()
  })

  it('start() registers listener and pulls initial snapshot + settings', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_peer_receiving') return Promise.resolve([makeTask()])
      if (cmd === 'get_peer_receive_settings')
        return Promise.resolve({ policyMode: 'always_accept', askTimeoutSecs: 120 })
      return Promise.resolve(undefined)
    })

    const receiving = usePeerReceiving()
    await receiving.start()

    expect(mockInvoke).toHaveBeenCalledWith('list_peer_receiving')
    expect(receiving.receivingTasks.value).toHaveLength(1)
    expect(receiving.settings.value.policyMode).toBe('always_accept')
  })

  it('receive-changed event replaces list and derives current offer', async () => {
    mockInvoke.mockResolvedValue([])
    const receiving = usePeerReceiving()
    await receiving.start()

    await emit('peer-receive-changed', [makeTask()])
    expect(receiving.currentOffer.value?.batchId).toBe('b-1')

    // 终态化后弹窗消失（宿主 resolved 快照驱动，与发送侧同款范式）
    await emit('peer-receive-changed', [makeTask({ status: 'rejected', rejectReason: 'timeout' })])
    expect(receiving.currentOffer.value).toBeNull()
  })

  it('respond and cancel route to host commands', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_peer_receiving') return Promise.resolve([])
      if (cmd === 'get_peer_receive_settings') return Promise.resolve({ policyMode: 'ask', askTimeoutSecs: 60 })
      return Promise.resolve(true)
    })

    const receiving = usePeerReceiving()
    await receiving.start()

    await receiving.respond('b-1', false)
    expect(mockInvoke).toHaveBeenCalledWith('respond_peer_transfer', { batchId: 'b-1', accepted: false })

    await receiving.cancel('b-2')
    expect(mockInvoke).toHaveBeenCalledWith('cancel_peer_receiving', { batchId: 'b-2' })
  })

  it('setPolicy validates via backend then reloads settings', async () => {
    // 可变设置状态：set 写入、get 读回，模拟宿主持久化 + 回读闭环
    const settingsState = { policyMode: 'ask', askTimeoutSecs: 60 }
    mockInvoke.mockImplementation((cmd: string, args?: any) => {
      if (cmd === 'list_peer_receiving') return Promise.resolve([])
      if (cmd === 'get_peer_receive_settings') return Promise.resolve({ ...settingsState })
      if (cmd === 'set_peer_receive_policy') {
        settingsState.policyMode = args.mode
        settingsState.askTimeoutSecs = args.timeoutSecs
        return Promise.resolve(undefined)
      }
      return Promise.resolve(undefined)
    })

    const receiving = usePeerReceiving()
    await receiving.start()

    expect(await receiving.setPolicy('always_deny', 300)).toBe(true)
    expect(receiving.settings.value.policyMode).toBe('always_deny')
    expect(receiving.settings.value.askTimeoutSecs).toBe(300)
  })
})
