/**
 * 任务核心逻辑 (Mobile) — 自有存储 wire 版（issue 13 Phase 3）
 *
 * 权威数据源为插件 WASM 自持存储的派生事件（引擎 PeerTransferDto camelCase
 * 形状）：tasks/batches/receiving/history 四路快照整表替换；本层只做一层
 * wire → 前端内部模型映射，组件消费模型不变。可发送性标记改由 usePeerDevices
 * 的自建设备缓存驱动（mdns-found/lost），本文件不再消费 devices-changed。
 */
import { ref, computed } from 'vue'
import type { Disposable, PluginContext } from '@binblink/plugin-sdk-mobile'
import type {
  Task,
  TaskStateName,
  PendingBatch,
  ReceivingTask,
  HistoryEntry,
} from '../types'
import { isTerminalState } from '../types'

/** 展示名：首文件名（多文件追加 +N） */
function displayName(files: unknown): string {
  const arr = Array.isArray(files) ? files : []
  const first = arr[0]?.path ?? arr[0]?.relativePath ?? 'file'
  const name = String(first).split('/').pop() ?? String(first)
  const extra = Math.max(0, arr.length - 1)
  return extra > 0 ? `${name} +${extra}` : name
}

/** 将自有存储条目（camelCase TransferEntry）映射为前端内部模型 */
function mapWireTask(raw: any): Task {
  const status = String(raw.status ?? '')
  return {
    id: raw.batchId ?? '',
    direction: raw.direction === 'receive' ? 'download' : 'upload',
    peer: {
      deviceId: raw.nodeId ?? '',
      name: raw.peerName ?? '',
    },
    remotePath: displayName(raw.files),
    size: raw.totalBytes ?? 0,
    offset: raw.transferredBytes ?? 0,
    rateBps: raw.rateBps ?? 0,
    state: (status === 'running' ? 'transferring' : status) as TaskStateName,
    reason: raw.detail ?? raw.rejectReason ?? null,
    initiator: 'me',
    batchId: raw.batchId ?? null,
    createdAt: raw.createdAtMs ?? 0,
    updatedAt: raw.updatedAtMs ?? 0,
  }
}

/** 接收任务快照项映射（自持存储 camelCase wire） */
function mapWireReceiving(raw: any): ReceivingTask {
  const status = String(raw.status ?? 'running')
  return {
    sessionId: raw.batchId ?? '',
    batchId: raw.batchId ?? null,
    remotePath: displayName(raw.files),
    size: raw.totalBytes ?? 0,
    offset: raw.transferredBytes ?? 0,
    state: status === 'running' ? 'transferring' : status,
    reason: raw.detail ?? null,
    peerId: raw.nodeId ?? '',
    peerName: raw.peerName ?? '',
    createdAt: raw.createdAtMs ?? 0,
    updatedAt: raw.updatedAtMs ?? 0,
  }
}

/** 历史条目映射（增量到达时按 id 去重合并） */
function mergeHistory(current: HistoryEntry[], incoming: HistoryEntry[]): HistoryEntry[] {
  const byId = new Map(current.map((e) => [e.id, e]))
  for (const e of incoming) byId.set(e.id, e)
  return [...byId.values()].sort((a, b) => b.updatedAt - a.updatedAt)
}

function mapWireHistory(raw: any): HistoryEntry {
  return {
    id: raw.batchId ?? '',
    direction: raw.direction === 'receive' ? 'download' : 'upload',
    initiator: raw.direction === 'receive' ? 'peer' : 'me',
    fileName: displayName(raw.files),
    size: raw.totalBytes ?? 0,
    state: raw.status ?? 'failed',
    reason: raw.detail ?? raw.rejectReason ?? null,
    peerName: raw.peerName ?? '',
    localPath: raw.localPath ?? null,
    createdAt: raw.createdAtMs ?? 0,
    updatedAt: raw.updatedAtMs ?? 0,
  }
}

/** pending 批映射（自持存储 camelCase wire：files[].path → relativePath） */
function mapWireBatch(raw: any): PendingBatch {
  const files = Array.isArray(raw.files) ? raw.files : []
  return {
    batchId: raw.batchId ?? '',
    peerName: raw.peerName ?? '',
    files: files.map((f: any) => ({
      relativePath: f.path ?? f.relativePath ?? '',
      size: f.size ?? 0,
    })),
    totalSize: raw.totalBytes ?? 0,
    createdAt: raw.createdAtMs ?? 0,
  }
}

export function useTasks(context: PluginContext) {
  /** 发送方向任务列表（批级） */
  const tasks = ref<Task[]>([])
  /** 「正在接收」tab 数据源（非 pending 接收批） */
  const receivingTasks = ref<ReceivingTask[]>([])
  /** 「历史」tab 数据源 */
  const history = ref<HistoryEntry[]>([])
  /** 接收应答卡数据源（pending 批） */
  const batches = ref<PendingBatch[]>([])
  /** 活跃传输总速率（B/s，取各 running 批 rateBps 求和） */
  const totalSpeed = ref(0)

  /** 存在具备文件传输能力的发现设备（= 可发送） */
  const peerOnline = ref(false)
  const peerId = ref('')
  const peerName = ref('')

  let dispTasks: Disposable | null = null
  let dispBatches: Disposable | null = null
  let dispReceiving: Disposable | null = null
  let dispHistory: Disposable | null = null

  /** 任务/速率快照整表替换 */
  function applySnapshot(list: any[]): void {
    const next = list.map(mapWireTask)
    tasks.value = next
    let speed = 0
    for (const t of next) {
      if (t.state === 'transferring') speed += t.rateBps ?? 0
    }
    totalSpeed.value = speed

    const firstNamed = next.find((t) => t.peer?.name)
    if (firstNamed?.peer?.name) peerName.value = firstNamed.peer.name
    checkSettledNotification()
  }

  /** 队列全部完成/失败 → 系统通知（每批仅一次） */
  let settledNotified = false
  function checkSettledNotification(): void {
    const list = tasks.value
    if (list.length === 0) {
      settledNotified = false
      return
    }
    if (list.some((t) => !isTerminalState(t.state))) {
      settledNotified = false
      return
    }
    if (settledNotified) return
    settledNotified = true
    const completed = list.filter((t) => t.state === 'completed').length
    const failed = list.filter((t) => t.state === 'failed' || t.state === 'rejected').length
    const cancelled = list.filter((t) => t.state === 'cancelled').length
    if (cancelled === list.length) return
    void (async () => {
      try {
        if (failed > 0) {
          await context.notifications.notify(
            context.i18n.t('transfer.notify.failedTitle'),
            context.i18n.t('transfer.notify.failedBody', { count: failed }),
          )
        } else if (completed > 0) {
          await context.notifications.notify(
            context.i18n.t('transfer.notify.doneTitle'),
            context.i18n.t('transfer.notify.doneBody', { count: completed }),
          )
        }
      } catch (e) {
        console.warn('[File Transfer] notification failed:', e)
      }
    })()
  }

  // ==================== 命令封装 ====================

  /** 拉取发送方向任务（初始同步） */
  async function refresh(): Promise<void> {
    try {
      const data = await context.commands.execute('file-transfer.list-tasks', {})
      if (Array.isArray(data)) applySnapshot(data)
    } catch (e) {
      console.error('[File Transfer] list-tasks failed:', e)
    }
  }

  /**
   * 发送本地文件到活跃对端（paths 来自系统选择器 pickFiles）
   * 返回成功入队的批数（0 = 全部失败）
   */
  async function sendFiles(paths: string[]): Promise<number> {
    if (paths.length === 0) return 0
    try {
      await context.commands.execute('file-transfer.enqueue', { paths })
      return paths.length
    } catch (e) {
      console.error('[File Transfer] send failed:', e)
      context.dialogs.showToast(String(e), 'error')
      return 0
    }
  }

  /** 兼容保留：可发送性已由 usePeerDevices 的自建缓存驱动，此处仅回当前值 */
  async function queryPeer(): Promise<boolean> {
    return peerOnline.value
  }

  async function cancel(id: string): Promise<void> {
    await context.commands.execute('file-transfer.cancel', { taskId: id })
  }
  async function retry(id: string): Promise<void> {
    await context.commands.execute('file-transfer.retry', { taskId: id })
  }
  async function approveBatch(batchId: string): Promise<void> {
    try {
      await context.commands.execute('file-transfer.approve-batch', { batchId })
    } catch (e) {
      console.error(`[File Transfer] approve-batch failed for "${batchId}":`, e)
    }
  }
  async function rejectBatch(batchId: string): Promise<void> {
    try {
      await context.commands.execute('file-transfer.reject-batch', { batchId })
    } catch (e) {
      console.error(`[File Transfer] reject-batch failed for "${batchId}":`, e)
    }
  }
  async function cancelReceiving(sessionId: string): Promise<void> {
    try {
      await context.commands.execute('file-transfer.cancel-receiving', { sessionId })
    } catch (e) {
      console.error(`[File Transfer] cancel-receiving failed for "${sessionId}":`, e)
    }
  }
  async function clearHistory(): Promise<void> {
    // 破坏性操作：先经 SDK confirm 二次确认（先例：useConsent 首连 / TrustedPeersSection 撤销）
    let confirmed = false
    try {
      confirmed =
        (await context.dialogs.showConfirm({
          title: context.i18n.t('transfer.history.clearConfirmTitle'),
          message: context.i18n.t('transfer.history.clearConfirmMessage'),
          variant: 'warning',
          confirmText: context.i18n.t('transfer.history.clearConfirmAction'),
          cancelText: context.i18n.t('transfer.dialog.cancel'),
          dismissible: true,
        })) === true
    } catch (e) {
      console.error('[File Transfer] clear-history confirm failed:', e)
    }
    if (!confirmed) return
    try {
      await context.commands.execute('file-transfer.clear-history', {})
      history.value = []
    } catch (e) {
      console.error('[File Transfer] clear-history failed:', e)
    }
  }

  // ==================== 派生状态 ====================

  /** 是否有未完成任务 */
  const hasRunning = computed(() => tasks.value.some((t) => !isTerminalState(t.state)))

  /** 当前主任务（迷你传输条展示：优先传输中） */
  const primaryTask = computed<Task | null>(
    () => tasks.value.find((t) => t.state === 'transferring') ?? null,
  )

  /** 对端展示名 */
  const displayPeerName = computed(() => {
    if (peerName.value) return peerName.value
    if (peerId.value) return peerId.value
    return context.i18n.t('transfer.peer.unpaired')
  })

  // ==================== 生命周期 ====================

  /** 注册事件监听（组件挂载调用；随 context._disposables 兜底清理） */
  function start(): void {
    stop()
    dispTasks = context.events.on('plugin:file-transfer:tasks-changed', (payload: unknown) => {
      if (Array.isArray(payload)) applySnapshot(payload)
    })
    dispBatches = context.events.on('plugin:file-transfer:batches-changed', (payload: unknown) => {
      if (Array.isArray(payload)) batches.value = payload.map(mapWireBatch)
    })
    dispReceiving = context.events.on(
      'plugin:file-transfer:receiving-changed',
      (payload: unknown) => {
        if (Array.isArray(payload)) {
          // 诊断插桩：接收快照到达（排查下载进度不更新：确认事件到达前端）
          const first = payload[0]
          console.debug(
            '[File Transfer] receiving snapshot',
            payload.length,
            first
              ? `${first.batchId ?? '?'} off=${first.transferredBytes ?? 0}/${first.totalBytes ?? 0} st=${first.status ?? '?'}`
              : '(empty)',
          )
          receivingTasks.value = payload.map(mapWireReceiving)
        }
      },
    )
    dispHistory = context.events.on('plugin:file-transfer:history-changed', (payload: unknown) => {
      if (Array.isArray(payload)) history.value = mergeHistory(history.value, payload.map(mapWireHistory))
    })
    void refresh()
    void refreshReceiving()
    void queryPeer()
  }

  /** 拉取接收侧初始快照（pending batches / 接收中 / 历史） */
  async function refreshReceiving(): Promise<void> {
    try {
      const [b, r, h] = await Promise.all([
        context.commands.execute('file-transfer.list-batches', {}),
        context.commands.execute('file-transfer.list-receiving', {}),
        context.commands.execute('file-transfer.list-history', {}),
      ])
      batches.value = Array.isArray(b) ? b.map(mapWireBatch) : []
      receivingTasks.value = Array.isArray(r) ? r.map(mapWireReceiving) : []
      history.value = Array.isArray(h) ? h.map(mapWireHistory) : []
    } catch (e) {
      console.error('[File Transfer] initial receiving snapshot failed:', e)
    }
  }

  /** 摘除事件监听 */
  function stop(): void {
    dispTasks?.dispose()
    dispTasks = null
    dispBatches?.dispose()
    dispBatches = null
    dispReceiving?.dispose()
    dispReceiving = null
    dispHistory?.dispose()
    dispHistory = null
    totalSpeed.value = 0
  }

  return {
    tasks,
    receivingTasks,
    history,
    batches,
    totalSpeed,
    hasRunning,
    primaryTask,
    peerOnline,
    peerId,
    peerName,
    displayPeerName,
    refresh,
    refreshReceiving,
    sendFiles,
    queryPeer,
    cancel,
    retry,
    approveBatch,
    rejectBatch,
    cancelReceiving,
    clearHistory,
    start,
    stop,
  }
}
