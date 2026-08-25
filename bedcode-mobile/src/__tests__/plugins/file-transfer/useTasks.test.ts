/**
 * useTasks 编排测试（ticket 09 场景矩阵补齐）
 *
 * 承接被删宿主 usePeerTransfers + usePeerReceiving 编排测试的场景矩阵（移动
 * 端批级模型合并版）：四类快照事件整表替换与 wire 契约翻译、历史按 id 去重
 * 合并、refreshV2 三列表初始拉取、发送空选拒绝 / 失败 toast、应答/取消/
 * 重试/清历史命令路由、队列终态系统通知每批一次、设备变化只维护可发送标记
 * （活跃对端收敛到 usePeerDevices，不越权 set-active-peer）。
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { PluginContext } from '@binblink/plugin-sdk-mobile'
import { useTasks } from '../../../../plugins/file-transfer/src/composables/useTasks'

type EventHandler = (payload: any) => void

function makeContext() {
  const calls: Array<{ id: string; args: any }> = []
  const handlers = new Map<string, EventHandler[]>()
  const responders = new Map<string, (args: any) => unknown>()
  const notifications: Array<{ title: string; body: string }> = []

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
    i18n: {
      t(key: string, params?: Record<string, any>) {
        return params ? `${key}:${JSON.stringify(params)}` : key
      },
    },
    dialogs: {
      showToast(message: string, type?: string) {
        return { message, type }
      },
    },
    notifications: {
      async notify(title: string, body: string) {
        notifications.push({ title, body })
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

  return { context, calls, emit, onCommand, listenerCount, flush, notifications }
}

function makeWireTask(overrides: Record<string, any> = {}) {
  return {
    id: 'batch-1',
    direction: 'upload',
    peer: { device_id: 'node-a', name: '设备-a' },
    remote_path: 'report.pdf',
    size: 1024,
    offset: 512,
    rate_bps: 256,
    state: 'transferring',
    reason: null,
    initiator: 'me',
    batch_id: 'batch-1',
    created_at: 1,
    updated_at: 2,
    ...overrides,
  }
}

describe('useTasks orchestration', () => {
  let env: ReturnType<typeof makeContext>

  beforeEach(() => {
    vi.clearAllMocks()
    env = makeContext()
    env.onCommand('file-transfer.list-tasks', () => [])
    env.onCommand('file-transfer.list-batches', () => [])
    env.onCommand('file-transfer.list-receiving', () => [])
    env.onCommand('file-transfer.list-history', () => [])
    env.onCommand('file-transfer.query-peer', () => [])
  })

  it('start pulls the initial v2 snapshot via the three list commands', async () => {
    env.onCommand('file-transfer.list-batches', () => [
      { batch_id: 'pb-1', peer_name: '设备-a', files: [{ path: 'a.pdf', size: 1 }], total_size: 1 },
    ])
    env.onCommand('file-transfer.list-receiving', () => [
      { session_id: 'r-1', remote_path: 'a.pdf', state: 'running' },
    ])
    env.onCommand('file-transfer.list-history', () => [
      { id: 'h-1', direction: 'download', file_name: 'a.pdf', state: 'completed' },
    ])
    const tasks = useTasks(env.context)
    tasks.start()
    await env.flush()

    expect(env.calls.map((c) => c.id)).toEqual(
      expect.arrayContaining([
        'file-transfer.list-batches',
        'file-transfer.list-receiving',
        'file-transfer.list-history',
      ]),
    )
    expect(tasks.batches.value[0]!.batchId).toBe('pb-1')
    expect(tasks.batches.value[0]!.files[0]!.relativePath).toBe('a.pdf')
    expect(tasks.receivingTasks.value[0]).toMatchObject({ sessionId: 'r-1', state: 'running' })
    expect(tasks.history.value[0]!.id).toBe('h-1')
  })

  it('tasks-changed replaces the list and derives totalSpeed from transferring batches', async () => {
    const tasks = useTasks(env.context)
    tasks.start()
    await env.flush()

    env.emit('plugin:file-transfer:tasks-changed', [
      makeWireTask({ rate_bps: 300 }),
      makeWireTask({ id: 'b-2', batch_id: 'b-2', state: 'failed', rate_bps: 999 }),
    ])

    expect(tasks.tasks.value.map((t) => t.state)).toEqual(['transferring', 'failed'])
    expect(tasks.totalSpeed.value).toBe(300)
    expect(tasks.peerName.value).toBe('设备-a')
  })

  it('history-changed merges by id (dedupe) and keeps newest first', async () => {
    const tasks = useTasks(env.context)
    tasks.start()
    await env.flush()

    env.emit('plugin:file-transfer:history-changed', [
      { id: 'h-1', direction: 'upload', file_name: 'a.pdf', state: 'completed', updated_at: 10 },
    ])
    env.emit('plugin:file-transfer:history-changed', [
      // 同 id 二次到达 → 覆盖而非重复；新条目按 updatedAt 排前
      { id: 'h-1', direction: 'upload', file_name: 'a.pdf', state: 'failed', updated_at: 20 },
      { id: 'h-2', direction: 'download', file_name: 'b.txt', state: 'completed', updated_at: 5 },
    ])

    expect(tasks.history.value.map((h) => h.id)).toEqual(['h-1', 'h-2'])
    expect(tasks.history.value[0]).toMatchObject({ state: 'failed', updatedAt: 20 })
  })

  it('sendFiles refuses an empty selection; routes enqueue on success; toasts on failure', async () => {
    const tasks = useTasks(env.context)

    expect(await tasks.sendFiles([])).toBe(0)
    expect(env.calls.some((c) => c.id === 'file-transfer.enqueue')).toBe(false)

    env.onCommand('file-transfer.enqueue', () => true)
    expect(await tasks.sendFiles(['/a.pdf', '/b.txt'])).toBe(2)

    env.onCommand('file-transfer.enqueue', () => Promise.reject(new Error('offline')))
    expect(await tasks.sendFiles(['/c.pdf'])).toBe(0)
  })

  it('cancel / retry / approve / reject / cancel-receiving route commands', async () => {
    env.onCommand('file-transfer.cancel', () => true)
    env.onCommand('file-transfer.retry', () => true)
    env.onCommand('file-transfer.approve-batch', () => true)
    env.onCommand('file-transfer.reject-batch', () => true)
    env.onCommand('file-transfer.cancel-receiving', () => true)
    const tasks = useTasks(env.context)

    await tasks.cancel('batch-9')
    await tasks.retry('batch-9')
    await tasks.approveBatch('pb-1')
    await tasks.rejectBatch('pb-1')
    await tasks.cancelReceiving('r-1')

    expect(env.calls).toContainEqual({ id: 'file-transfer.cancel', args: { taskId: 'batch-9' } })
    expect(env.calls).toContainEqual({ id: 'file-transfer.retry', args: { taskId: 'batch-9' } })
    expect(env.calls).toContainEqual({ id: 'file-transfer.approve-batch', args: { batchId: 'pb-1' } })
    expect(env.calls).toContainEqual({ id: 'file-transfer.reject-batch', args: { batchId: 'pb-1' } })
    expect(env.calls).toContainEqual({
      id: 'file-transfer.cancel-receiving',
      args: { sessionId: 'r-1' },
    })
  })

  it('clearHistory routes the command and empties local history', async () => {
    env.onCommand('file-transfer.clear-history', () => true)
    const tasks = useTasks(env.context)
    tasks.start()
    await env.flush()
    env.emit('plugin:file-transfer:history-changed', [
      { id: 'h-1', direction: 'upload', file_name: 'a.pdf', state: 'completed', updated_at: 1 },
    ])

    await env.flush()
    await tasks.clearHistory()

    expect(env.calls).toContainEqual({ id: 'file-transfer.clear-history', args: {} })
    expect(tasks.history.value).toEqual([])
  })

  it('notifies once when the queue fully settles; resets after the queue empties', async () => {
    const tasks = useTasks(env.context)
    tasks.start()
    await env.flush()

    env.emit('plugin:file-transfer:tasks-changed', [makeWireTask()])
    await env.flush()
    expect(env.notifications).toHaveLength(0) // 未全部终态不通知

    env.emit('plugin:file-transfer:tasks-changed', [makeWireTask({ state: 'completed' })])
    await env.flush()
    expect(env.notifications).toHaveLength(1)
    expect(env.notifications[0]!.title).toContain('doneTitle')

    // 全部终态期间重复快照不重复通知
    env.emit('plugin:file-transfer:tasks-changed', [makeWireTask({ state: 'completed' })])
    await env.flush()
    expect(env.notifications).toHaveLength(1)

    // 队列清空后重置，新批次完成可再次通知
    env.emit('plugin:file-transfer:tasks-changed', [])
    env.emit('plugin:file-transfer:tasks-changed', [makeWireTask({ state: 'completed' })])
    await env.flush()
    expect(env.notifications).toHaveLength(2)
  })

  it('failed-only settlement notifies with the failed template; cancelled-only stays silent', async () => {
    const tasks = useTasks(env.context)
    tasks.start()
    await env.flush()

    env.emit('plugin:file-transfer:tasks-changed', [
      makeWireTask({ state: 'failed' }),
      makeWireTask({ id: 'b-2', batch_id: 'b-2', state: 'rejected' }),
    ])
    await env.flush()
    expect(env.notifications).toHaveLength(1)
    expect(env.notifications[0]!.title).toContain('failedTitle')

    env.emit('plugin:file-transfer:tasks-changed', [])
    env.emit(
      'plugin:file-transfer:tasks-changed',
      [makeWireTask({ state: 'cancelled' })],
    )
    await env.flush()
    expect(env.notifications).toHaveLength(1) // 全取消不打扰用户
  })

  it('devices-changed only maintains the sendable flag without overriding active peer', async () => {
    const tasks = useTasks(env.context)
    tasks.start()
    await env.flush()

    env.emit('plugin:file-transfer:devices-changed', [
      { nodeId: 'node-x', deviceName: '不可传', fileTransfer: false },
      { nodeId: 'node-a', deviceName: '设备-a' },
    ])
    expect(tasks.peerOnline.value).toBe(true)

    env.emit('plugin:file-transfer:devices-changed', [{ nodeId: 'node-x', fileTransfer: false }])
    expect(tasks.peerOnline.value).toBe(false)
    await env.flush()

    // 活跃对端选择归 usePeerDevices，此处不得越权下发 set-active-peer
    expect(env.calls.some((c) => c.id === 'file-transfer.set-active-peer')).toBe(false)
  })

  it('ws_* control-plane events drive connOnline independently of discovery', async () => {
    const tasks = useTasks(env.context)
    tasks.start()
    await env.flush()
    env.emit('ws_connected', {})
    expect(tasks.connOnline.value).toBe(true)
    env.emit('ws_disconnected', {})
    expect(tasks.connOnline.value).toBe(false)
  })

  it('start is idempotent and stop detaches all listeners', async () => {
    const tasks = useTasks(env.context)
    tasks.start()
    tasks.start()
    expect(env.listenerCount('plugin:file-transfer:tasks-changed')).toBe(1)
    expect(env.listenerCount('plugin:file-transfer:batches-changed')).toBe(1)

    tasks.stop()
    expect(tasks.totalSpeed.value).toBe(0)
    expect(() => env.emit('plugin:file-transfer:tasks-changed', [])).toThrow()
  })
})
