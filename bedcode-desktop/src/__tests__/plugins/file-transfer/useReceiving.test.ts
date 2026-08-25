/**
 * useReceiving 编排测试（ticket 09 场景矩阵补齐）
 *
 * 承接被删宿主 usePeerReceiving 编排测试的场景矩阵：三个接收侧快照事件整表
 * 替换（snake_case → camelCase 契约翻译）、refresh 三列表并发拉取、应答/
 * 取消/清历史命令路由、接收中 toast（batch 立即弹 / per-file 3s 合并窗口，
 * spec §14.4）与 stop 对称清理。mock 最小 PluginContext，只测编排不测渲染。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import type { PluginContext } from '@binblink/plugin-sdk-desktop'
import { useReceiving } from '../../../../plugins/file-transfer/src/composables/useReceiving'

type EventHandler = (payload: any) => void

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

  return { context, calls, emit, onCommand, listenerCount }
}

describe('useReceiving orchestration', () => {
  let env: ReturnType<typeof makeContext>

  beforeEach(() => {
    vi.clearAllMocks()
    env = makeContext()
    env.onCommand('file-transfer.list-batches', () => [])
    env.onCommand('file-transfer.list-receiving', () => [])
    env.onCommand('file-transfer.list-history', () => [])
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('refresh pulls all three receiving-side lists via commands', async () => {
    env.onCommand('file-transfer.list-batches', () => [
      { batch_id: 'pb-1', peer_name: '设备-a', files: [{ path: 'docs/a.pdf', size: 10 }], total_size: 10 },
    ])
    env.onCommand('file-transfer.list-receiving', () => [
      { session_id: 'r-1', remote_path: 'a.pdf', state: 'running', peer_id: 'node-a' },
    ])
    env.onCommand('file-transfer.list-history', () => [
      { id: 'h-1', direction: 'download', file_name: 'a.pdf', state: 'completed' },
    ])
    const rec = useReceiving(env.context)

    await rec.refresh()

    expect(env.calls.map((c) => c.id)).toEqual(
      expect.arrayContaining([
        'file-transfer.list-batches',
        'file-transfer.list-receiving',
        'file-transfer.list-history',
      ]),
    )
    // wire 契约翻译：files[].path → relativePath；state running → transferring
    expect(rec.batches.value[0]).toMatchObject({ batchId: 'pb-1', peerName: '设备-a' })
    expect(rec.batches.value[0]!.files[0]!.relativePath).toBe('docs/a.pdf')
    expect(rec.receiving.value[0]).toMatchObject({ sessionId: 'r-1', state: 'transferring' })
    expect(rec.history.value[0]).toMatchObject({ id: 'h-1' })
  })

  it('snapshot events replace the three lists wholesale', () => {
    const rec = useReceiving(env.context)
    rec.start()

    env.emit('plugin:file-transfer:batches-changed', [
      { batch_id: 'pb-1', peer_name: '设备-a', files: [{ path: 'a.pdf', size: 1 }], total_size: 1 },
      { batch_id: 'pb-2', peer_name: '设备-b', files: [], total_size: 0 },
    ])
    env.emit('plugin:file-transfer:receiving-changed', [])
    env.emit('plugin:file-transfer:history-changed', [
      { id: 'h-1', direction: 'upload', file_name: 'x.txt', state: 'completed' },
    ])

    expect(rec.batches.value.map((b) => b.batchId)).toEqual(['pb-1', 'pb-2'])
    expect(rec.receiving.value).toEqual([])
    expect(rec.history.value.map((h) => h.id)).toEqual(['h-1'])
  })

  it('approve / reject / cancel-receiving / clear-history route commands', async () => {
    env.onCommand('file-transfer.approve-batch', () => true)
    env.onCommand('file-transfer.reject-batch', () => true)
    env.onCommand('file-transfer.cancel-receiving', () => true)
    env.onCommand('file-transfer.clear-history', () => ({ removed: 2 }))
    const rec = useReceiving(env.context)

    await rec.approveBatch('pb-1')
    await rec.rejectBatch('pb-1')
    await rec.cancelReceiving('r-1')
    await rec.clearHistory()

    expect(env.calls).toContainEqual({ id: 'file-transfer.approve-batch', args: { batchId: 'pb-1' } })
    expect(env.calls).toContainEqual({ id: 'file-transfer.reject-batch', args: { batchId: 'pb-1' } })
    expect(env.calls).toContainEqual({ id: 'file-transfer.cancel-receiving', args: { sessionId: 'r-1' } })
    expect(env.calls).toContainEqual({ id: 'file-transfer.clear-history', args: {} })
  })

  it('batch-mode toast appears immediately and auto-dismisses after 5s', () => {
    vi.useFakeTimers()
    const rec = useReceiving(env.context)
    rec.start()

    env.emit('plugin:file-transfer:toast', { mode: 'batch', name: '设备-a', count: 3, totalSize: 2048 })
    expect(rec.toasts.value).toHaveLength(1)
    expect(rec.toasts.value[0]).toMatchObject({ mode: 'batch', count: 3 })

    vi.advanceTimersByTime(5000)
    expect(rec.toasts.value).toHaveLength(0)
  })

  it('per-file toasts merge inside the 3s window instead of stacking', () => {
    vi.useFakeTimers()
    const rec = useReceiving(env.context)
    rec.start()

    env.emit('plugin:file-transfer:toast', { mode: 'per-file', name: '设备-a', count: 1 })
    env.emit('plugin:file-transfer:toast', { mode: 'per-file', name: '设备-a', count: 2 })
    // 同窗口只更新计数不重复弹
    expect(rec.toasts.value).toHaveLength(1)
    expect(rec.toasts.value[0]!.count).toBe(3)

    // 窗口过期后再到 → 新 toast
    vi.advanceTimersByTime(3000)
    expect(rec.toasts.value).toHaveLength(0)
    env.emit('plugin:file-transfer:toast', { mode: 'per-file', name: '设备-a', count: 1 })
    expect(rec.toasts.value).toHaveLength(1)
  })

  it('dismissToast removes the entry manually before auto-dismiss', () => {
    const rec = useReceiving(env.context)
    rec.start()

    env.emit('plugin:file-transfer:toast', { mode: 'batch', name: '设备-a', count: 1 })
    const id = rec.toasts.value[0]!.id
    rec.dismissToast(id)

    expect(rec.toasts.value).toHaveLength(0)
  })

  it('stop disposes subscriptions and clears transient state', () => {
    const rec = useReceiving(env.context)
    rec.start()
    rec.start()
    expect(env.listenerCount('plugin:file-transfer:batches-changed')).toBe(1)

    env.emit('plugin:file-transfer:toast', { mode: 'batch', name: '设备-a', count: 1 })
    rec.stop()
    expect(rec.toasts.value).toHaveLength(0)
    expect(() => env.emit('plugin:file-transfer:batches-changed', [])).toThrow()
  })
})
