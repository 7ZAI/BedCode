/**
 * usePeerReceiving 测试（issue 10）
 *
 * 覆盖编排逻辑（不测渲染）：接收快照拉取与事件替换、询问批选取（最早优先）、
 * 倒计时口径、应答/取消命令回流、设置读写。listen 经 mock 捕获处理器后手动
 * 派发事件载荷驱动。
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
    peerName: '张三的手机',
    direction: 'receive',
    status: 'pending',
    files: [{ path: 'photos/a.png', size: 10 }],
    totalBytes: 10,
    transferredBytes: 0,
    rateBps: 0,
    detail: null,
    rejectReason: null,
    createdAtMs: 1000,
    updatedAtMs: 1000,
    ...overrides,
  }
}

/** 经捕获的 listen 处理器派发一条事件载荷 */
async function emit(event: string, payload: unknown) {
  const handler = handlers[event]
  if (!handler) throw new Error(`no captured handler for ${event}`)
  await handler({ payload })
}

describe('usePeerReceiving pure helpers', () => {
  it('offerDeadline adds the ask window in seconds', () => {
    expect(offerDeadline(1000, 60)).toBe(61_000)
    expect(offerDeadline(1000, 10)).toBe(11_000)
  })

  it('remainingSeconds is ceil-based and clamps at zero', () => {
    expect(remainingSeconds(61_000, 60_000)).toBe(1)
    expect(remainingSeconds(60_500, 60_000)).toBe(1)
    expect(remainingSeconds(60_499, 60_000)).toBe(1)
    expect(remainingSeconds(59_999, 60_000)).toBe(0)
    expect(remainingSeconds(0, 60_000)).toBe(0)
  })

  it('pickCurrentOffer returns the earliest pending batch only', () => {
    const later = makeTask({ batchId: 'b-later', createdAtMs: 2000 })
    const earlier = makeTask({ batchId: 'b-earlier', createdAtMs: 1000 })
    const running = makeTask({ batchId: 'b-running', status: 'running', createdAtMs: 500 })

    expect(pickCurrentOffer([])).toBeNull()
    // 非 pending 批不参与
    expect(pickCurrentOffer([running])).toBeNull()
    // 多个 pending 取最早
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
        return Promise.resolve({ policyMode: 'ask', askTimeoutSecs: 30 })
      return Promise.resolve(undefined)
    })

    const receiving = usePeerReceiving()
    await receiving.start()

    expect(mockInvoke).toHaveBeenCalledWith('list_peer_receiving')
    expect(mockInvoke).toHaveBeenCalledWith('get_peer_receive_settings')
    expect(receiving.receivingTasks.value).toHaveLength(1)
    expect(receiving.settings.value.askTimeoutSecs).toBe(30)
  })

  it('receive-changed event replaces the list reactively', async () => {
    mockInvoke.mockResolvedValue([])
    const receiving = usePeerReceiving()
    await receiving.start()

    await emit('peer-receive-changed', [makeTask(), makeTask({ batchId: 'b-2' })])
    expect(receiving.receivingTasks.value).toHaveLength(2)
    // currentOffer 派生自列表内最早 pending
    expect(receiving.currentOffer.value?.batchId).toBe('b-1')

    await emit('peer-receive-changed', [])
    expect(receiving.currentOffer.value).toBeNull()
  })

  it('respond forwards accepted flag and reports delivery', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_peer_receiving') return Promise.resolve([])
      if (cmd === 'get_peer_receive_settings') return Promise.resolve({ policyMode: 'ask', askTimeoutSecs: 60 })
      if (cmd === 'respond_peer_transfer') return Promise.resolve(true)
      return Promise.resolve(undefined)
    })

    const receiving = usePeerReceiving()
    await receiving.start()
    const delivered = await receiving.respond('b-1', true)

    expect(mockInvoke).toHaveBeenCalledWith('respond_peer_transfer', { batchId: 'b-1', accepted: true })
    expect(delivered).toBe(true)
  })

  it('cancel routes to cancel_peer_receiving', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_peer_receiving') return Promise.resolve([])
      if (cmd === 'get_peer_receive_settings') return Promise.resolve({ policyMode: 'always_accept', askTimeoutSecs: 60 })
      return Promise.resolve(undefined)
    })

    const receiving = usePeerReceiving()
    await receiving.start()
    await receiving.cancel('b-9')

    expect(mockInvoke).toHaveBeenCalledWith('cancel_peer_receiving', { batchId: 'b-9' })
  })

  it('setPolicy validates via backend then reloads settings', async () => {
    // 可变设置状态：set 写入、get 读回，模拟宿主持久化 + 回读闭环
    const settingsState = { policyMode: 'ask', askTimeoutSecs: 60 }
    mockInvoke.mockImplementation((cmd: string, args?: any) => {
      if (cmd === 'list_peer_receiving') return Promise.resolve([])
      if (cmd === 'get_peer_receive_settings') return Promise.resolve({ ...settingsState })
      if (cmd === 'set_peer_receive_policy') {
        if (args?.timeoutSecs < 10) throw new Error('invalid')
        settingsState.policyMode = args.mode
        settingsState.askTimeoutSecs = args.timeoutSecs
        return Promise.resolve(undefined)
      }
      return Promise.resolve(undefined)
    })

    const receiving = usePeerReceiving()
    await receiving.start()

    const ok = await receiving.setPolicy('always_deny', 120)
    expect(ok).toBe(true)
    expect(mockInvoke).toHaveBeenCalledWith('set_peer_receive_policy', { mode: 'always_deny', timeoutSecs: 120 })
    expect(receiving.settings.value.policyMode).toBe('always_deny')
    expect(receiving.settings.value.askTimeoutSecs).toBe(120)
  })

  it('setDownloadDir forwards path and reloads settings (desktop)', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_peer_receiving') return Promise.resolve([])
      if (cmd === 'get_peer_receive_settings')
        return Promise.resolve({ policyMode: 'ask', askTimeoutSecs: 60, downloadDir: 'D:/dl' })
      return Promise.resolve(undefined)
    })

    const receiving = usePeerReceiving()
    await receiving.start()
    const ok = await receiving.setDownloadDir('D:/downloads/bedcode')

    expect(ok).toBe(true)
    expect(mockInvoke).toHaveBeenCalledWith('set_peer_download_dir', { path: 'D:/downloads/bedcode' })
    expect(receiving.settings.value.downloadDir).toBe('D:/dl')
  })

  it('clearHistory returns removed count from backend', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_peer_receiving') return Promise.resolve([])
      if (cmd === 'get_peer_receive_settings') return Promise.resolve({ policyMode: 'ask', askTimeoutSecs: 60 })
      if (cmd === 'clear_peer_receiving_history') return Promise.resolve(7)
      return Promise.resolve(undefined)
    })

    const receiving = usePeerReceiving()
    await receiving.start()
    expect(await receiving.clearHistory()).toBe(7)
  })
})
