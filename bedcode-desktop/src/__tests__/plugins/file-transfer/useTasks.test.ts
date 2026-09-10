/**
 * useTasks 编排测试（issue 13 Phase 3 自持存储版）
 *
 * 场景矩阵：start 注册监听并拉初始快照、事件整表替换（进度 + 终态 +
 * interrupted 映射）、cancel/retry 命令路由、发送空选拒绝、派生状态
 * （hasRunning/primaryTask/totalSpeed）与生命周期幂等。
 * mock 最小 PluginContext，只测编排逻辑不测渲染。
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import { useTasks } from '../../../../plugins/file-transfer/src/composables/useTasks'

type EventHandler = (payload: any) => void

/** 最小 mock PluginContext：记录命令调用 + 捕获事件处理器（含重复注册计数） */
function makeContext() {
  const calls: Array<{ id: string; args: any }> = []
  /** 同名事件可能叠加多个处理器（用于 start 幂等断言），全部派发 */
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

/** wire 形状快照项（引擎 PeerTransferDto camelCase，自持存储条目同构） */
function makeWireTask(overrides: Record<string, any> = {}) {
  return {
    batchId: 'batch-1',
    nodeId: 'node-a',
    peerName: '设备-a',
    direction: 'send',
    status: 'running',
    files: [{ path: 'docs/report.pdf', size: 1024 }],
    totalBytes: 1024,
    transferredBytes: 512,
    rateBps: 256,
    createdAtMs: 1,
    updatedAtMs: 2,
    ...overrides,
  }
}

describe('useTasks orchestration', () => {
  let env: ReturnType<typeof makeContext>

  beforeEach(() => {
    vi.clearAllMocks()
    env = makeContext()
    env.onCommand('file-transfer.list-tasks', () => [])
  })

  it('refresh routes list-tasks and populates the snapshot', async () => {
    env.onCommand('file-transfer.list-tasks', () => [makeWireTask()])
    const tasks = useTasks(env.context)

    await tasks.refresh()

    expect(env.calls).toContainEqual({ id: 'file-transfer.list-tasks', args: {} })
    expect(tasks.tasks.value).toHaveLength(1)
    expect(tasks.tasks.value[0]).toMatchObject({
      id: 'batch-1',
      state: 'transferring',
      peer: { deviceId: 'node-a', name: '设备-a' },
      remotePath: 'report.pdf',
    })
  })

  it('tasks-changed event replaces the full list and derives speed + peer name', async () => {
    const tasks = useTasks(env.context)
    tasks.start()
    await env.flush()

    env.emit('plugin:file-transfer:tasks-changed', [
      makeWireTask({ rateBps: 300 }),
      makeWireTask({ batchId: 'b-2', status: 'completed', rateBps: 999 }),
      makeWireTask({ batchId: 'b-3', nodeId: 'node-b', peerName: '', status: 'failed' }),
    ])

    // 整表替换（进度 + 终态并存），速率只累加传输中的批次
    expect(tasks.tasks.value.map((t) => t.state)).toEqual(['transferring', 'completed', 'failed'])
    expect(tasks.totalSpeed.value).toBe(300)
    // 展示对端名取首个有名字的任务
    expect(tasks.peerName.value).toBe('设备-a')
    expect(tasks.hasRunning.value).toBe(true)
    expect(tasks.primaryTask.value?.id).toBe('batch-1')
  })

  it('interrupted status maps to a retryable terminal state', async () => {
    const tasks = useTasks(env.context)
    tasks.start()
    await env.flush()

    env.emit('plugin:file-transfer:tasks-changed', [
      makeWireTask({ batchId: 'b-x', status: 'interrupted' }),
    ])

    expect(tasks.tasks.value[0]?.state).toBe('interrupted')
    expect(tasks.hasRunning.value).toBe(false)
    expect(tasks.primaryTask.value).toBeNull()
  })

  it('multi-file display name appends +N to the first file name', async () => {
    env.onCommand('file-transfer.list-tasks', () => [
      makeWireTask({
        files: [{ path: 'a.png', size: 1 }, { path: 'b.png', size: 2 }, { path: 'c.png', size: 3 }],
      }),
    ])
    const tasks = useTasks(env.context)

    await tasks.refresh()

    expect(tasks.tasks.value[0]?.remotePath).toBe('a.png +2')
  })

  it('all-terminal snapshot settles derived flags (no primary, no running)', async () => {
    const tasks = useTasks(env.context)
    tasks.start()
    await env.flush()

    env.emit('plugin:file-transfer:tasks-changed', [
      makeWireTask({ status: 'completed' }),
      makeWireTask({ batchId: 'b-2', status: 'cancelled' }),
    ])

    expect(tasks.hasRunning.value).toBe(false)
    expect(tasks.primaryTask.value).toBeNull()
    expect(tasks.totalSpeed.value).toBe(0)
  })

  it('cancel and retry route commands with the task id', async () => {
    env.onCommand('file-transfer.cancel', () => ({ ok: true }))
    env.onCommand('file-transfer.retry', () => ({ batchId: 'new' }))
    const tasks = useTasks(env.context)

    await tasks.cancel('batch-9')
    await tasks.retry('batch-9')

    expect(env.calls).toContainEqual({ id: 'file-transfer.cancel', args: { taskId: 'batch-9' } })
    expect(env.calls).toContainEqual({ id: 'file-transfer.retry', args: { taskId: 'batch-9' } })
  })

  it('sendPickedFiles refuses an empty picker result without enqueueing', async () => {
    env.onCommand('file-transfer.pick-files', () => [])
    env.onCommand('file-transfer.enqueue', () => ({}))
    const tasks = useTasks(env.context)

    const sent = await tasks.sendPickedFiles()

    expect(sent).toBe(0)
    expect(env.calls.some((c) => c.id === 'file-transfer.enqueue')).toBe(false)
  })

  it('sendPickedFiles routes enqueue with picked paths and reports the count', async () => {
    env.onCommand('file-transfer.pick-files', () => ['C:/a.pdf', 'C:/b.txt'])
    env.onCommand('file-transfer.enqueue', () => ({ batchId: 'n' }))
    const tasks = useTasks(env.context)

    const sent = await tasks.sendPickedFiles()

    expect(sent).toBe(2)
    expect(env.calls).toContainEqual({
      id: 'file-transfer.enqueue',
      args: { paths: ['C:/a.pdf', 'C:/b.txt'] },
    })
  })

  it('start is idempotent (no stacked listeners) and stop detaches + resets', () => {
    const tasks = useTasks(env.context)
    tasks.start()
    tasks.start()
    expect(env.listenerCount('plugin:file-transfer:tasks-changed')).toBe(1)

    tasks.totalSpeed.value = 42
    tasks.stop()
    expect(tasks.totalSpeed.value).toBe(0)
    // 停止后事件不再送达（订阅已摘除）
    expect(() => env.emit('plugin:file-transfer:tasks-changed', [])).toThrow()
  })
})
