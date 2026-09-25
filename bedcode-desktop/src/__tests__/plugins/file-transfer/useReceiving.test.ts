/**
 * useReceiving 编排测试（ticket 09 场景矩阵补齐）
 *
 * 承接被删宿主 usePeerReceiving 编排测试的场景矩阵：三个接收侧快照事件整表
 * 替换（snake_case → camelCase 契约翻译）、refresh 三列表并发拉取、应答/
 * 取消/清历史命令路由、接收中 toast（batch 立即弹 / per-file 3s 合并窗口，
 * spec §14.4）与 stop 对称清理。mock 最小 PluginContext，只测编排不测渲染。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import { useReceiving } from '../../../../wasm-apps/file-transfer/src/composables/useReceiving'
import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification'

// 系统通知模块整体 mock：编排测试只验证是否/何时触发，不触碰真实 Tauri 桥
vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue(true),
  sendNotification: vi.fn(),
}))

type EventHandler = (payload: any) => void

function makeContext() {
  const calls: Array<{ id: string; args: any }> = []
  const handlers = new Map<string, EventHandler[]>()
  const responders = new Map<string, (args: any) => unknown>()
  /** 已弹出的宿主全局弹窗（清空历史二次确认用例驱动用） */
  const dialogs: Array<{ options: any; closed: boolean }> = []

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
    // maybeNotifyPendingBatch 转发到系统通知的 title/body
    i18n: {
      t: (key: string) => key,
    },
    // 宿主全局弹窗：记录 options 供用例点击动作（清空历史的二次确认）
    ui: {
      showDialog(options: any) {
        const entry = { options, closed: false }
        dialogs.push(entry)
        return {
          close: () => {
            entry.closed = true
          },
          update: (next: any) => {
            entry.options = { ...entry.options, ...next }
          },
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

  return { context, calls, emit, onCommand, listenerCount, dialogs }
}

/**
 * 驱动清空历史确认弹窗：点击指定动作（默认「清空」）。
 * i18n 桩返回 key，故按 key 定位动作按钮。
 */
function clickClearDialogAction(
  env: ReturnType<typeof makeContext>,
  actionKey = 'transfer.history.clear',
): void {
  const dialog = env.dialogs.at(-1)
  expect(dialog, 'clear-history must open a confirm dialog').toBeTruthy()
  const action = dialog!.options.actions.find((a: any) => a.label === actionKey)
  expect(action, `confirm action ${actionKey} must exist`).toBeTruthy()
  action.onClick()
}

/** 自持存储条目 wire 形状工厂（引擎 PeerTransferDto camelCase） */
function makeEntry(overrides: Record<string, any> = {}) {
  return {
    batchId: 'b-1',
    nodeId: 'node-a',
    peerName: '设备-a',
    direction: 'receive',
    status: 'running',
    files: [{ path: 'docs/a.pdf', size: 1024 }],
    totalBytes: 1024,
    transferredBytes: 512,
    rateBps: 0,
    createdAtMs: 1,
    updatedAtMs: 2,
    ...overrides,
  }
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
      {
        batchId: 'pb-1', nodeId: 'node-a', peerName: '设备-a',
        direction: 'receive', status: 'pending',
        files: [{ path: 'docs/a.pdf', size: 10 }], totalBytes: 10,
        transferredBytes: 0, rateBps: 0, createdAtMs: 1, updatedAtMs: 2,
      },
    ])
    env.onCommand('file-transfer.list-receiving', () => [
      makeEntry({ batchId: 'r-1', status: 'running' }),
    ])
    env.onCommand('file-transfer.list-history', () => [
      makeEntry({ batchId: 'h-1', status: 'completed' }),
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
    // wire 契约翻译：files[].path → relativePath；status running → transferring
    expect(rec.batches.value[0]).toMatchObject({ batchId: 'pb-1', peerName: '设备-a' })
    expect(rec.batches.value[0]!.files[0]!.relativePath).toBe('docs/a.pdf')
    expect(rec.receiving.value[0]).toMatchObject({ sessionId: 'r-1', state: 'transferring' })
    expect(rec.history.value[0]).toMatchObject({ id: 'h-1', fileName: 'a.pdf' })
  })

  it('receiving snapshot carries rate and paused status for the queue card', () => {
    // 接收（下载）卡需要速率与暂停态：rateBps 不映射则卡上无速度可显示，
    // paused 不直传则暂停任务卡无法呈现「继续」
    const rec = useReceiving(env.context)
    rec.start()

    env.emit('plugin:file-transfer:receiving-changed', [
      makeEntry({ batchId: 'r-2', status: 'paused', rateBps: 4096, transferredBytes: 256 }),
    ])

    expect(rec.receiving.value[0]).toMatchObject({
      sessionId: 'r-2',
      state: 'paused',
      rateBps: 4096,
      offset: 256,
    })
  })

  it('snapshot events replace the three lists wholesale', () => {
    const rec = useReceiving(env.context)
    rec.start()

    env.emit('plugin:file-transfer:batches-changed', [
      makeEntry({ batchId: 'pb-1', peerName: '设备-a', status: 'pending' }),
      makeEntry({ batchId: 'pb-2', peerName: '设备-b', status: 'pending', files: [] }),
    ])
    env.emit('plugin:file-transfer:receiving-changed', [])
    env.emit('plugin:file-transfer:history-changed', [
      makeEntry({ batchId: 'h-1', direction: 'send', status: 'completed' }),
    ])

    expect(rec.batches.value.map((b) => b.batchId)).toEqual(['pb-1', 'pb-2'])
    expect(rec.receiving.value).toEqual([])
    expect(rec.history.value.map((h) => h.id)).toEqual(['h-1'])
  })

  it('refresh renders the lists that succeeded even when one command fails (Bug 9)', async () => {
    env.onCommand('file-transfer.list-batches', () => [
      {
        batchId: 'pb-1', nodeId: 'node-a', peerName: '设备-a',
        direction: 'receive', status: 'pending',
        files: [{ path: 'docs/a.pdf', size: 10 }], totalBytes: 10,
        transferredBytes: 0, rateBps: 0, createdAtMs: 1, updatedAtMs: 2,
      },
    ])
    env.onCommand('file-transfer.list-receiving', () => Promise.reject(new Error('wasm dead')))
    env.onCommand('file-transfer.list-history', () => [
      makeEntry({ batchId: 'h-1', status: 'completed' }),
    ])
    const rec = useReceiving(env.context)

    await rec.refresh()

    // 部分成功也渲染：失败命令只影响其对应列表，不拖垮其余
    expect(rec.batches.value).toHaveLength(1)
    expect(rec.receiving.value).toEqual([])
    expect(rec.history.value).toHaveLength(1)
  })

  it('approve/reject report false when the command rejects (Bug 8)', async () => {
    env.onCommand('file-transfer.approve-batch', () => Promise.reject(new Error('boom')))
    env.onCommand('file-transfer.reject-batch', () => Promise.reject(new Error('boom')))
    const rec = useReceiving(env.context)

    await expect(rec.approveBatch('pb-1')).resolves.toBe(false)
    await expect(rec.rejectBatch('pb-1')).resolves.toBe(false)
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
    const clearing = rec.clearHistory()
    clickClearDialogAction(env)
    await clearing

    expect(env.calls).toContainEqual({ id: 'file-transfer.approve-batch', args: { batchId: 'pb-1' } })
    expect(env.calls).toContainEqual({ id: 'file-transfer.reject-batch', args: { batchId: 'pb-1' } })
    expect(env.calls).toContainEqual({ id: 'file-transfer.cancel-receiving', args: { sessionId: 'r-1' } })
    expect(env.calls).toContainEqual({ id: 'file-transfer.clear-history', args: {} })
  })

  it('clear-history re-pulls lists after the command so the panel cannot stay stale', async () => {
    env.onCommand('file-transfer.clear-history', () => ({ cleared: 3 }))
    env.onCommand('file-transfer.list-history', () => [])
    const rec = useReceiving(env.context)
    await rec.refresh()
    const before = env.calls.length

    const clearing = rec.clearHistory()
    clickClearDialogAction(env)
    await clearing

    const after = env.calls.slice(before).map((c) => c.id)
    // 清空命令之后必须重拉三列表（历史为空 → UI 立刻反映插件存储真源）
    expect(after[0]).toBe('file-transfer.clear-history')
    expect(after).toEqual(
      expect.arrayContaining([
        'file-transfer.list-batches',
        'file-transfer.list-receiving',
        'file-transfer.list-history',
      ]),
    )
    expect(rec.history.value).toEqual([])
  })

  it('clear-history rejects and keeps local history when the command fails', async () => {
    env.onCommand('file-transfer.list-history', () => [
      makeEntry({ batchId: 'h-1', status: 'completed' }),
    ])
    const rec = useReceiving(env.context)
    await rec.refresh()
    expect(rec.history.value).toHaveLength(1)

    env.onCommand('file-transfer.clear-history', () => {
      throw new Error('Plugin com.bedcode.file-transfer is not activated')
    })
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {})

    const clearing = rec.clearHistory()
    clickClearDialogAction(env)
    await expect(clearing).rejects.toThrow('is not activated')

    // 失败不得伪装成成功：本地历史保持不变，且原因经 console.error 留痕
    expect(spy).toHaveBeenCalled()
    expect(rec.history.value).toHaveLength(1)
    spy.mockRestore()
  })

  it('clear-history does nothing when the confirm dialog is cancelled', async () => {
    env.onCommand('file-transfer.list-history', () => [
      makeEntry({ batchId: 'h-1', status: 'completed' }),
    ])
    const rec = useReceiving(env.context)
    await rec.refresh()

    const clearing = rec.clearHistory()
    clickClearDialogAction(env, 'transfer.task.cancel')
    await clearing

    // 反例：未确认不得下发破坏性命令，历史保持不变
    expect(env.calls.some((c) => c.id === 'file-transfer.clear-history')).toBe(false)
    expect(rec.history.value).toHaveLength(1)
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

describe('useReceiving pending-batch 系统通知（窗口不可见时；前台由宿主全局弹窗承载）', () => {
  let env: ReturnType<typeof makeContext>
  /** document.hidden 的可变桩值（happy-dom 下 hidden 为可重定义普通属性） */
  let docHidden = false
  /** pending 批 wire 形状（ask 策略下宿主推送） */
  const pendingWire = () => [
    {
      batchId: 'pb-1', nodeId: 'node-a', peerName: '设备-a',
      direction: 'receive', status: 'pending',
      files: [{ path: 'docs/a.pdf', size: 10 }], totalBytes: 10,
      transferredBytes: 0, rateBps: 0, createdAtMs: 1, updatedAtMs: 2,
    },
  ]

  beforeEach(() => {
    vi.clearAllMocks()
    docHidden = false
    Object.defineProperty(document, 'hidden', {
      configurable: true,
      get: () => docHidden,
    })
    env = makeContext()
    env.onCommand('file-transfer.list-batches', () => [])
    env.onCommand('file-transfer.list-receiving', () => [])
    env.onCommand('file-transfer.list-history', () => [])
  })

  afterEach(() => {
    vi.useRealTimers()
    Object.defineProperty(document, 'hidden', {
      configurable: true,
      get: () => false,
    })
  })

  it('窗口不可见时发系统通知，同批去重不重复发', async () => {
    const rec = useReceiving(env.context)
    rec.start()
    // 先落定初始 refresh：其异步空快照会在批事件后到达时把刚记账的批 ID 从
    // notifiedBatches 清掉（去重记账与启动刷新存在竞态——生产上极小概率重复
    // 通知；测试需确定性，故先收敛再触发事件）
    for (let i = 0; i < 5; i++) await Promise.resolve()

    docHidden = true
    env.emit('plugin:file-transfer:batches-changed', pendingWire())
    // maybeNotifyPendingBatch 的权限检查 await 挂起后在下一条微任务完成（sendNotification + 记账同在同步块）
    await Promise.resolve()
    await Promise.resolve()
    expect(sendNotification).toHaveBeenCalledTimes(1)
    expect(isPermissionGranted).toHaveBeenCalled()
    expect(requestPermission).not.toHaveBeenCalled()
    // 通知载荷带批信息（title/body 转发 i18n key）
    expect(sendNotification).toHaveBeenCalledWith(
      expect.objectContaining({ title: 'transfer.request.title' }),
    )

    // 同批再次到达（事件重复）不重复通知
    env.emit('plugin:file-transfer:batches-changed', pendingWire())
    await Promise.resolve()
    expect(sendNotification).toHaveBeenCalledTimes(1)
  })

  it('窗口可见（前台）时不发系统通知——待确认弹窗由宿主全局弹窗承载', async () => {
    const rec = useReceiving(env.context)
    rec.start()
    for (let i = 0; i < 5; i++) await Promise.resolve()

    env.emit('plugin:file-transfer:batches-changed', pendingWire())
    await Promise.resolve()
    expect(sendNotification).not.toHaveBeenCalled()
  })
})
