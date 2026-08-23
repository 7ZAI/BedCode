/**
 * 任务核心逻辑 (Mobile) — host-peer 契约版
 *
 * 权威数据源为插件 WASM 转发的宿主对等事件（批级模型）：
 * - tasks-changed：发送方向 PeerTransferDto[] → Task（一批一条记录）
 * - receiving-changed / batches-changed / history-changed 同构
 * - devices-changed：DiscoveredPeerDto[]，前端挑选具备文件传输能力的设备为活跃对端
 *
 * 命令调用经 context.commands.execute('file-transfer.*')，由插件 Rust 侧
 * 薄代理转发 host.peer_* 并翻译 wire 形状；本层只做事件消费与命令编排。
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

/** 将代理层快照项（snake_case）映射为前端 camelCase 内部模型 */
function mapWireTask(raw: any): Task {
  return {
    id: raw.id ?? raw.batch_id ?? '',
    direction: raw.direction === 'upload' ? 'upload' : 'download',
    peer: {
      deviceId: raw.peer?.device_id ?? raw.peer?.deviceId ?? '',
      name: raw.peer?.name ?? '',
    },
    remotePath: raw.remote_path ?? raw.remotePath ?? '',
    localPath: null,
    size: raw.size ?? 0,
    offset: raw.offset ?? 0,
    rateBps: raw.rate_bps ?? raw.rateBps ?? 0,
    state: raw.state as TaskStateName,
    reason: raw.reason ?? null,
    initiator: raw.initiator === 'peer' ? 'peer' : 'me',
    batchId: raw.batch_id ?? raw.batchId ?? null,
    createdAt: raw.created_at ?? raw.createdAt ?? 0,
    updatedAt: raw.updated_at ?? raw.updatedAt ?? 0,
  }
}

/** 接收任务快照项映射 */
function mapWireReceiving(raw: any): ReceivingTask {
  return {
    sessionId: raw.session_id ?? raw.sessionId ?? '',
    batchId: raw.batch_id ?? raw.batchId ?? null,
    remotePath: raw.remote_path ?? raw.remotePath ?? '',
    size: raw.size ?? 0,
    offset: raw.offset ?? 0,
    state: raw.state ?? 'running',
    reason: raw.reason ?? null,
    peerId: raw.peer_id ?? raw.peerId ?? '',
    peerName: raw.peer_name ?? raw.peerName ?? '',
    createdAt: raw.created_at ?? raw.createdAt ?? 0,
    updatedAt: raw.updated_at ?? raw.updatedAt ?? 0,
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
    id: raw.id ?? '',
    direction: raw.direction === 'upload' ? 'upload' : 'download',
    initiator: raw.initiator === 'peer' ? 'peer' : 'me',
    fileName: raw.file_name ?? raw.fileName ?? '',
    size: raw.size ?? 0,
    state: raw.state ?? 'failed',
    reason: raw.reason ?? null,
    peerName: raw.peer_name ?? raw.peerName ?? '',
    localPath: null,
    createdAt: raw.created_at ?? raw.createdAt ?? 0,
    updatedAt: raw.updated_at ?? raw.updatedAt ?? 0,
  }
}

/** pending 批映射（宿主 DTO 即 camelCase 契约形状） */
function mapWireBatch(raw: any): PendingBatch {
  const files = Array.isArray(raw.files) ? raw.files : []
  return {
    batchId: raw.batch_id ?? raw.batchId ?? '',
    peerName: raw.peer_name ?? raw.peerName ?? '',
    files: files.map((f: any) => ({
      relativePath: f.path ?? f.relativePath ?? f.relative_path ?? '',
      size: f.size ?? 0,
    })),
    totalSize: raw.total_size ?? raw.totalSize ?? 0,
    createdAt: raw.created_at ?? raw.createdAt ?? 0,
  }
}

/** 发现设备（devices-changed 事件项） */
interface DeviceInfo {
  nodeId: string
  deviceName: string
  fileTransfer?: boolean
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

  /** WS 控制面连接状态（宿主 ws_* 事件驱动） */
  const connOnline = ref(false)
  /** 存在具备文件传输能力的发现设备（= 可发送） */
  const peerOnline = ref(false)
  const peerId = ref('')
  const peerName = ref('')

  let dispTasks: Disposable | null = null
  let dispBatches: Disposable | null = null
  let dispReceiving: Disposable | null = null
  let dispHistory: Disposable | null = null
  let dispDevices: Disposable | null = null
  let dispConn: Disposable[] = []

  // 初始连接状态：读取宿主共享连接状态（视图挂载可能晚于 ws_paired 事件）
  try {
    connOnline.value =
      (globalThis as any).__BEDCODE_SHARED__?.connection?.isConnected?.value === true
  } catch {
    connOnline.value = false
  }

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

  /** 设备列表变化：挑选首个具备文件传输能力的节点为活跃对端 */
  function onDevicesChanged(payload: unknown): void {
    if (!Array.isArray(payload)) return
    const capable = payload.find(
      (d: DeviceInfo) => d.fileTransfer !== false && !!d.nodeId,
    ) as DeviceInfo | undefined
    if (capable) {
      peerOnline.value = true
      peerId.value = capable.nodeId
      if (capable.deviceName) peerName.value = capable.deviceName
      connOnline.value = true
      void context.commands
        .execute('file-transfer.set-active-peer', { peerId: capable.nodeId })
        .catch(() => {})
    } else {
      peerOnline.value = false
    }
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

  /** 刷新设备列表（探测回复后由 devices-changed 事件驱动状态更新） */
  async function queryPeer(): Promise<boolean> {
    try {
      const devices = await context.commands.execute('file-transfer.query-peer', {})
      onDevicesChanged(devices)
      return true
    } catch (e) {
      console.error('[File Transfer] query-peer failed:', e)
      return false
    }
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
    if (connOnline.value) return context.i18n.t('transfer.peer.unknown')
    return context.i18n.t('transfer.peer.unpaired')
  })

  // ==================== 生命周期 ====================

  function onConnChanged(online: boolean): void {
    connOnline.value = online
  }

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
        if (Array.isArray(payload)) receivingTasks.value = payload.map(mapWireReceiving)
      },
    )
    dispHistory = context.events.on('plugin:file-transfer:history-changed', (payload: unknown) => {
      if (Array.isArray(payload)) history.value = mergeHistory(history.value, payload.map(mapWireHistory))
    })
    dispDevices = context.events.on('plugin:file-transfer:devices-changed', onDevicesChanged)
    dispConn = [
      context.events.on('ws_connected', () => onConnChanged(true)),
      context.events.on('ws_paired', () => onConnChanged(true)),
      context.events.on('ws_reconnected', () => onConnChanged(true)),
      context.events.on('ws_disconnected', () => onConnChanged(false)),
      context.events.on('ws_unexpected_disconnect', () => onConnChanged(false)),
      context.events.on('ws_reconnecting', () => onConnChanged(false)),
      context.events.on('ws_reconnect_failed', () => onConnChanged(false)),
      context.events.on('ws_error', () => onConnChanged(false)),
      context.events.on('ws_auth_failed', () => onConnChanged(false)),
    ]
    void refresh()
    void refreshV2()
    void queryPeer()
  }

  /** 拉取接收侧初始快照 */
  async function refreshV2(): Promise<void> {
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
      console.error('[File Transfer] initial v2 snapshot failed:', e)
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
    dispDevices?.dispose()
    dispDevices = null
    dispConn.forEach((d) => d.dispose())
    dispConn = []
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
    connOnline,
    peerOnline,
    peerId,
    peerName,
    displayPeerName,
    refresh,
    refreshV2,
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
