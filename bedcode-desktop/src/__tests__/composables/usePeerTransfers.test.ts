/**
 * usePeerTransfers 测试（issue 09）
 *
 * 覆盖编排逻辑（不测渲染）：全量列表拉取与事件替换、扇出发送的相互独立
 * （单台失败不影响其余）、取消/重试/清空命令接线、活跃与历史分区、
 * 空入参防御拦截。listen 经 mock 捕获处理器后手动派发事件载荷驱动。
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import {
  usePeerTransfers,
  _resetPeerTransfersForTest,
  type PeerTransferTask,
} from '@/composables/usePeerTransfers'

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
const BATCH_A = 'batch-a'
const BATCH_B = 'batch-b'

function makeTask(overrides: Partial<PeerTransferTask> = {}): PeerTransferTask {
  return {
    batchId: BATCH_A,
    nodeId: NODE_A,
    peerName: '张三的手机',
    direction: 'send',
    status: 'running',
    files: [{ path: 'photos/a.png', size: 10 }],
    totalBytes: 100,
    transferredBytes: 40,
    rateBps: 2048,
    detail: null,
    rejectReason: null,
    createdAtMs: 1_000,
    updatedAtMs: 2_000,
    ...overrides,
  }
}

/** 经捕获的 listen 处理器派发一条事件载荷 */
async function emit(event: string, payload: unknown) {
  const handler = handlers[event]
  if (!handler) throw new Error(`no captured handler for ${event}`)
  await handler({ payload })
}

describe('usePeerTransfers flow', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    handlers['peer-transfer-changed'] = undefined as unknown as PayloadHandler
    _resetPeerTransfersForTest()
  })

  it('start() registers listener and pulls initial task snapshot', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_peer_transfers') return Promise.resolve([makeTask()])
      return Promise.resolve(undefined)
    })
    const api = usePeerTransfers()
    expect(api.transfers.value).toHaveLength(0)
    await api.start()

    expect(mockInvoke).toHaveBeenCalledWith('list_peer_transfers')
    expect(api.transfers.value).toHaveLength(1)
  })

  it('start() is idempotent and does not double-register listeners', async () => {
    mockInvoke.mockResolvedValue([])
    const api = usePeerTransfers()
    await api.start()
    await api.start()
    // listen 只注册一次（peer-transfer-changed 单事件）
    expect(Object.keys(handlers).filter((k) => k === 'peer-transfer-changed')).toHaveLength(1)
  })

  it('transfer-changed event replaces the full list (progress + terminal states)', async () => {
    mockInvoke.mockResolvedValue([])
    const api = usePeerTransfers()
    await api.start()

    await emit('peer-transfer-changed', [
      makeTask({ batchId: BATCH_B, nodeId: NODE_B, status: 'completed', transferredBytes: 100 }),
      makeTask({ transferredBytes: 90, rateBps: 4096 }),
    ])

    expect(api.transfers.value).toHaveLength(2)
    expect(api.activeTransfers.value.map((t) => t.batchId)).toEqual([BATCH_A])
    expect(api.historyTransfers.value.map((t) => t.batchId)).toEqual([BATCH_B])
  })

  it('sendToPeers fans out one command per node and stays independent on failure', async () => {
    mockInvoke.mockImplementation((cmd: string, args?: any) => {
      if (cmd === 'send_files_to_peer') {
        if (args.nodeId === NODE_A) return Promise.resolve(makeTask({ batchId: BATCH_A }))
        // B 拒绝/离线：命令面失败，但不得影响 A 的发起
        return Promise.reject(new Error('dial unreachable'))
      }
      return Promise.resolve(undefined)
    })
    const paths = ['C:/docs/report.pdf']
    const outcomes = await usePeerTransfers().sendToPeers(paths, [NODE_A, NODE_B])

    // 每台设备各一条独立批：两次独立调用互不阻断
    expect(mockInvoke).toHaveBeenCalledWith('send_files_to_peer', { nodeId: NODE_A, paths })
    expect(mockInvoke).toHaveBeenCalledWith('send_files_to_peer', { nodeId: NODE_B, paths })
    expect(outcomes).toEqual([
      { nodeId: NODE_A, ok: true, batchId: BATCH_A },
      { nodeId: NODE_B, ok: false },
    ])
  })

  it('sendToPeers refuses empty selections without invoking commands', async () => {
    mockInvoke.mockResolvedValue([])
    const api = usePeerTransfers()

    expect(await api.sendToPeers([], [NODE_A])).toEqual([])
    expect(await api.sendToPeers(['C:/a.bin'], [])).toEqual([])
    expect(mockInvoke).not.toHaveBeenCalledWith(
      'send_files_to_peer',
      expect.anything(),
    )
  })

  it('cancel invokes the cancel command with batch id', async () => {
    mockInvoke.mockResolvedValue(true)
    await usePeerTransfers().cancel(BATCH_A)
    expect(mockInvoke).toHaveBeenCalledWith('cancel_peer_transfer', { batchId: BATCH_A })
  })

  it('retry reports success and surfaces failures honestly', async () => {
    const api = usePeerTransfers()
    mockInvoke.mockImplementation((cmd: string, args?: any) => {
      if (cmd === 'retry_peer_transfer') {
        if (args.batchId === BATCH_A) return Promise.resolve(makeTask())
        return Promise.reject(new Error('sources unavailable after restart'))
      }
      return Promise.resolve(undefined)
    })
    expect(await api.retry(BATCH_A)).toBe(true)
    expect(await api.retry(BATCH_B)).toBe(false)
  })

  it('clearHistory returns removed count even when command fails', async () => {
    mockInvoke.mockResolvedValueOnce(3)
    expect(await usePeerTransfers().clearHistory()).toBe(3)

    mockInvoke.mockRejectedValueOnce(new Error('node stopped'))
    expect(await usePeerTransfers().clearHistory()).toBe(0)
  })
})
