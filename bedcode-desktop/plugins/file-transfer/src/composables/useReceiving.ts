/**
 * v2 接收侧状态与传输历史
 *
 * 数据源：WASM 全量快照事件（batches-changed / receiving-changed /
 * history-changed）+ 初始拉取命令（list-batches / list-receiving /
 * list-history），与 useTasks 同模式：整表替换避免状态漂移。
 *
 * toast（接收中提醒）：宿主无现成 toast API，插件内自绘轻量 toast 队列；
 * mode=batch 立即弹一条；mode=per-file 进入 3 秒合并窗口——窗口内新文件
 * 只更新计数不重复弹（spec §14.4）。
 */
import { ref, type Ref } from 'vue'
import type { Disposable, PluginContext } from '@binblink/plugin-sdk-desktop'
import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification'
import type { HistoryEntry, PendingBatch, ReceivingTask } from '../types'
import { formatBytes } from '../utils/format'

/** per-file toast 合并窗口（毫秒） */
const PER_FILE_TOAST_WINDOW_MS = 3000

/** 接收中 toast 条目 */
export interface TransferToast {
  id: number
  /** 对端名（空串时前端用占位文案） */
  name: string
  /** 文件数（per-file 窗口内累加） */
  count: number
  /** 批级总大小（仅 batch 模式） */
  totalSize?: number
  mode: 'batch' | 'per-file'
}

function mapPendingBatch(raw: any): PendingBatch {
  return {
    batchId: raw.batch_id ?? raw.batchId ?? '',
    peerId: raw.peer_id ?? raw.peerId ?? '',
    peerName: raw.peer_name ?? raw.peerName ?? '',
    files: Array.isArray(raw.files) ? raw.files : [],
    totalSize: raw.total_size ?? raw.totalSize ?? 0,
    createdAt: raw.created_at ?? raw.createdAt ?? 0,
  }
}

function mapReceivingTask(raw: any): ReceivingTask {
  return {
    sessionId: raw.session_id ?? raw.sessionId ?? '',
    batchId: raw.batch_id ?? raw.batchId ?? null,
    remotePath: raw.remote_path ?? raw.remotePath ?? '',
    size: raw.size ?? 0,
    state: raw.state ?? 'transferring',
    reason: raw.reason ?? null,
    peerId: raw.peer_id ?? raw.peerId ?? '',
    createdAt: raw.created_at ?? raw.createdAt ?? 0,
    updatedAt: raw.updated_at ?? raw.updatedAt ?? 0,
  }
}

function mapHistoryEntry(raw: any): HistoryEntry {
  return {
    id: raw.id ?? '',
    direction: raw.direction === 'upload' ? 'upload' : 'download',
    initiator: raw.initiator === 'peer' ? 'peer' : 'me',
    fileName: raw.file_name ?? raw.fileName ?? '',
    size: raw.size ?? 0,
    state: raw.state ?? 'failed',
    reason: raw.reason ?? null,
    peerName: raw.peer_name ?? raw.peerName ?? '',
    localPath: raw.local_path ?? raw.localPath ?? null,
    createdAt: raw.created_at ?? raw.createdAt ?? 0,
    updatedAt: raw.updated_at ?? raw.updatedAt ?? 0,
  }
}

export function useReceiving(context: PluginContext) {
  /** pending 批（应答卡数据源） */
  const batches = ref<PendingBatch[]>([]) as Ref<PendingBatch[]>
  /** 接收中任务 */
  const receiving = ref<ReceivingTask[]>([]) as Ref<ReceivingTask[]>
  /** 传输历史（封顶 200，最新在前） */
  const history = ref<HistoryEntry[]>([]) as Ref<HistoryEntry[]>
  /** toast 队列（接收中提醒） */
  const toasts = ref<TransferToast[]>([]) as Ref<TransferToast[]>

  let dispBatches: Disposable | null = null
  let dispReceiving: Disposable | null = null
  let dispHistory: Disposable | null = null
  let dispToast: Disposable | null = null
  /** per-file 合并窗口内的 toast id（窗口内只更新计数） */
  let perFileToastId: number | null = null
  let toastSeq = 0
  let dismissTimers = new Map<number, ReturnType<typeof setTimeout>>()

  function applyBatches(list: any[]): void {
    batches.value = (Array.isArray(list) ? list : []).map(mapPendingBatch)
    // 最小化/后台时新 pending 批 → 系统通知「打开应用」（spec §12.3：桌面最小化
    // 不提供通知内应答，仅提示；点击通知聚焦窗口）。前台横幅已足够，不发通知。
    // 同批只通知一次；批消失后从集合移除，允许重新请求时再次提示
    const ids = new Set(batches.value.map(b => b.batchId))
    for (const id of [...notifiedBatches]) {
      if (!ids.has(id)) notifiedBatches.delete(id)
    }
    for (const batch of batches.value) void maybeNotifyPendingBatch(batch)
  }
  function applyReceiving(list: any[]): void {
    receiving.value = (Array.isArray(list) ? list : []).map(mapReceivingTask)
  }
  function applyHistory(list: any[]): void {
    history.value = (Array.isArray(list) ? list : []).map(mapHistoryEntry)
  }

  /** 移除 toast（自动消失或手动关闭） */
  function dismissToast(id: number): void {
    toasts.value = toasts.value.filter(t => t.id !== id)
    dismissTimers.delete(id)
  }

  // ==================== 系统通知（最小化/后台） ====================

  /** 已发系统通知的批 ID（去重，见 applyBatches） */
  const notifiedBatches = new Set<string>()
  /** 通知权限是否已检查过（避免每次批到达都请求） */
  let notifyPermissionChecked = false

  /** 窗口不可见且批未通知过时发系统通知（best-effort，失败仅记日志） */
  async function maybeNotifyPendingBatch(batch: PendingBatch): Promise<void> {
    if (!document.hidden || notifiedBatches.has(batch.batchId)) return
    try {
      if (!notifyPermissionChecked) {
        notifyPermissionChecked = true
        if (!(await isPermissionGranted())) await requestPermission()
      }
      sendNotification({
        title: context.i18n.t('transfer.request.title'),
        body: context.i18n.t('transfer.request.body', {
          name: batch.peerName || context.i18n.t('transfer.peer.unknown'),
          count: batch.files?.length ?? 1,
          size: formatBytes(batch.totalSize),
        }),
      })
      notifiedBatches.add(batch.batchId)
    } catch (e) {
      console.error('[File Transfer] notify pending batch failed:', e)
    }
  }

  /** 入队 toast：batch 立即弹；per-file 3s 窗口合并去重 */
  function pushToast(payload: any): void {
    const mode: 'batch' | 'per-file' = payload?.mode === 'batch' ? 'batch' : 'per-file'
    const name: string = payload?.name ?? ''
    const count = Math.max(1, Number(payload?.count ?? 1))
    const totalSize = Number(payload?.totalSize ?? 0)

    if (mode === 'per-file' && perFileToastId !== null) {
      // 窗口内：只更新计数，不重复弹（spec §14.4 3s 窗口合并）
      const existing = toasts.value.find(t => t.id === perFileToastId)
      if (existing) {
        existing.count += count
        return
      }
      perFileToastId = null
    }

    const id = ++toastSeq
    const toast: TransferToast = { id, name, count, totalSize, mode }
    toasts.value = [...toasts.value, toast]
    if (mode === 'per-file') perFileToastId = id
    // 自动消失：5s（batch）/ 3s（per-file）
    const timer = setTimeout(() => {
      dismissToast(id)
      if (perFileToastId === id) perFileToastId = null
    }, mode === 'batch' ? 5000 : 3000)
    dismissTimers.set(id, timer)
  }

  // ==================== 命令封装 ====================

  async function refresh(): Promise<void> {
    try {
      const [b, r, h] = await Promise.all([
        context.commands.execute('file-transfer.list-batches', {}),
        context.commands.execute('file-transfer.list-receiving', {}),
        context.commands.execute('file-transfer.list-history', {}),
      ])
      applyBatches(Array.isArray(b) ? b : [])
      applyReceiving(Array.isArray(r) ? r : [])
      applyHistory(Array.isArray(h) ? h : [])
    } catch (e) {
      console.error('[File Transfer] receiving/history refresh failed:', e)
    }
  }

  async function approveBatch(batchId: string): Promise<void> {
    await context.commands.execute('file-transfer.approve-batch', { batchId })
  }
  async function rejectBatch(batchId: string): Promise<void> {
    await context.commands.execute('file-transfer.reject-batch', { batchId })
  }
  async function cancelReceiving(sessionId: string): Promise<void> {
    await context.commands.execute('file-transfer.cancel-receiving', { sessionId })
  }
  async function clearHistory(): Promise<void> {
    await context.commands.execute('file-transfer.clear-history', {})
  }

  // ==================== 生命周期 ====================

  function start(): void {
    stop()
    dispBatches = context.events.on('plugin:file-transfer:batches-changed', (p: any) =>
      applyBatches(Array.isArray(p) ? p : []),
    )
    dispReceiving = context.events.on('plugin:file-transfer:receiving-changed', (p: any) =>
      applyReceiving(Array.isArray(p) ? p : []),
    )
    dispHistory = context.events.on('plugin:file-transfer:history-changed', (p: any) =>
      applyHistory(Array.isArray(p) ? p : []),
    )
    dispToast = context.events.on('plugin:file-transfer:toast', pushToast)
    void refresh()
  }

  function stop(): void {
    dispBatches?.dispose()
    dispBatches = null
    dispReceiving?.dispose()
    dispReceiving = null
    dispHistory?.dispose()
    dispHistory = null
    dispToast?.dispose()
    dispToast = null
    perFileToastId = null
    for (const t of dismissTimers.values()) clearTimeout(t)
    dismissTimers.clear()
    toasts.value = []
  }

  return {
    batches,
    receiving,
    history,
    toasts,
    refresh,
    approveBatch,
    rejectBatch,
    cancelReceiving,
    clearHistory,
    dismissToast,
    start,
    stop,
  }
}
