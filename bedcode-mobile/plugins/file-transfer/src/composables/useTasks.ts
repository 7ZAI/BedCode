/**
 * 任务核心逻辑 (Mobile)
 *
 * 权威数据源为 WASM 推送的 `plugin:file-transfer:tasks-changed` 快照
 * （每次状态迁移/进度更新后由 WASM 全量推送），前端不做增量合并，
 * 直接整表替换避免状态漂移。
 *
 * `plugin:transfer:progress` 事件携带的是宿主传输引擎 UUID（host_task_id），
 * 与插件任务 id 不同命名空间，无法逐任务映射；该事件仅用于聚合瞬时总速率，
 * 作为快照差分速率的上限补充。
 *
 * 命令调用约定与桌面端同构（context.commands.execute('file-transfer.*')）。
 * 同名被拒（enqueue 返回 rejected / reason=duplicate-name）→ context.dialogs 弹
 * 「无法上传」对话框；队列全部完成/失败 → context.notifications 通知。
 */
import { ref, computed } from 'vue'
import type { Disposable, PluginContext } from '@bedcode/plugin-sdk-mobile'
import type {
  Task,
  TaskStateName,
  TransferProgress,
  PeerStatus,
} from '../types'
import { isTerminalState } from '../types'

/** 快照差分缓存（任务 id → 上一次 offset + 时间戳），用于推导逐任务速率 */
interface OffsetSample {
  offset: number
  at: number
}

/** 将 WASM 快照项（snake_case）映射为前端 camelCase 内部模型 */
function mapWireTask(raw: any): Task {
  return {
    id: raw.id,
    direction: raw.direction === 'upload' ? 'upload' : 'download',
    peer: {
      deviceId: raw.peer?.device_id ?? raw.peer?.deviceId ?? '',
      name: raw.peer?.name ?? '',
    },
    remotePath: raw.remote_path ?? raw.remotePath ?? '',
    localPath: raw.local_path ?? raw.localPath ?? '',
    size: raw.size ?? 0,
    offset: raw.offset ?? 0,
    uploadSessionId: raw.upload_session_id ?? raw.uploadSessionId ?? null,
    fingerprint: raw.fingerprint ?? null,
    state: raw.state as TaskStateName,
    reason: raw.reason ?? null,
    createdAt: raw.created_at ?? raw.createdAt ?? 0,
    updatedAt: raw.updated_at ?? raw.updatedAt ?? 0,
  }
}

/** 入队参数（下载/上传通用） */
export interface EnqueueArgs {
  direction: 'download' | 'upload'
  peerId: string
  peerName: string
  remotePath: string
  localPath?: string
}

/** 是否被拒任务（enqueue 返回的 rejected / reason=duplicate-name） */
function isRejectedTask(result: any): boolean {
  return (
    result &&
    (result.state === 'rejected' ||
      result.reason === 'duplicate-name' ||
      result.reason === 'DuplicateName')
  )
}

export function useTasks(context: PluginContext) {
  /** 任务列表（按 WASM 快照时间序，最新在前） */
  const tasks = ref<Task[]>([])
  /** 逐任务速率（快照差分，字节/秒） */
  const speedMap = ref<Record<string, number>>({})
  /** 进度事件聚合瞬时速率（host task id → bps 的存活窗口求和） */
  const progressSpeed = ref(0)

  /** 对端在线状态（filesrv:peer_changed） */
  const peerOnline = ref(false)
  const peerId = ref('')
  /** 对端展示名：优先 tasks 中的 peer.name，其次对端 id */
  const peerName = ref('')

  /** 队列是否已结算（避免重复通知） */
  let notifiedSettled = false

  /** 快照差分样本表（任务生命周期内持续累积） */
  const offsetSamples = new Map<string, OffsetSample>()
  /** 进度事件瞬时速率样本表（host task id → bps + 时间戳） */
  const progressSamples = new Map<string, { at: number; bps: number }>()

  let dispTasks: Disposable | null = null
  let dispProgress: Disposable | null = null
  let dispPeer: Disposable | null = null

  /** 整表替换任务快照，并差分推导逐任务速率 */
  function applySnapshot(list: any[]): void {
    const now = Date.now()
    const next = list.map(mapWireTask)
    const nextSpeeds: Record<string, number> = {}

    for (const t of next) {
      const prev = offsetSamples.get(t.id)
      if (prev && t.state === 'transferring' && t.offset >= prev.offset) {
        const dt = (now - prev.at) / 1000
        nextSpeeds[t.id] = dt > 0 ? (t.offset - prev.offset) / dt : 0
      } else {
        nextSpeeds[t.id] = 0
      }
      offsetSamples.set(t.id, { offset: t.offset, at: now })
    }

    tasks.value = next
    speedMap.value = nextSpeeds

    // 对端名兜底：快照里携带 peer.name 时优先采用
    const firstNamed = next.find(t => t.peer?.name)
    if (firstNamed?.peer?.name) peerName.value = firstNamed.peer.name

    checkSettledNotification()
  }

  /** 任务快照事件（权威来源） */
  function onTasksChanged(payload: any): void {
    if (Array.isArray(payload)) applySnapshot(payload)
  }

  /** 进度事件：仅聚合存活窗口内的速率（>2s 未更新视为该 host 任务已结束） */
  function onProgress(payload: TransferProgress): void {
    const now = Date.now()
    if (payload?.state?.state === 'running') {
      progressSamples.set(payload.taskId, { at: now, bps: payload.bytesPerSec || 0 })
    }
    // 过期样本清理 + 求和（事件频率 ~500ms/任务，窗口 2s 内保留）
    let sum = 0
    for (const [id, sample] of progressSamples) {
      if (now - sample.at > 2000) {
        progressSamples.delete(id)
      } else {
        sum += sample.bps
      }
    }
    progressSpeed.value = sum
  }

  /** 对端上下线事件 */
  function onPeerChanged(payload: PeerStatus): void {
    if (payload?.peerId) peerId.value = payload.peerId
    peerOnline.value = !!payload?.online
  }

  // ==================== 命令封装（与桌面同构） ====================

  /** 拉取全量任务（宿主重启后补同步一次） */
  async function refresh(): Promise<void> {
    try {
      const data = await context.commands.execute('file-transfer.list-tasks', {})
      const arr = Array.isArray(data) ? data : (data?.tasks ?? [])
      applySnapshot(arr)
    } catch (e) {
      console.error('[File Transfer] list-tasks failed:', e)
    }
  }

  /** 入队单个任务（返回命令结果；被拒任务由调用方决定弹窗） */
  async function enqueue(args: EnqueueArgs): Promise<any> {
    return context.commands.execute('file-transfer.enqueue', {
      direction: args.direction,
      peerId: args.peerId,
      peerName: args.peerName,
      remotePath: args.remotePath,
      localPath: args.localPath ?? null,
    })
  }

  /**
   * 批量入队下载（逐个入队，单个失败不中断整批）。
   * 任一同名被拒即弹「无法上传」Material 对话框（context.dialogs）。
   */
  async function enqueueDownload(
    paths: string[],
    peer: { id: string; name: string },
  ): Promise<number> {
    let ok = 0
    for (const remotePath of paths) {
      try {
        const result = await enqueue({
          direction: 'download',
          peerId: peer.id,
          peerName: peer.name,
          remotePath,
        })
        if (isRejectedTask(result)) {
          void showDuplicateDialog()
        } else {
          ok++
        }
      } catch (e) {
        console.error(`[File Transfer] enqueue failed for "${remotePath}":`, e)
      }
    }
    return ok
  }

  /**
   * 入队上传（移动端无文件选择器：localPath 由调用方经 dialogs.showPrompt 手动输入）。
   * 同名被拒即时弹「无法上传」对话框。
   */
  async function enqueueUpload(args: Omit<EnqueueArgs, 'direction'>): Promise<boolean> {
    try {
      const result = await enqueue({ ...args, direction: 'upload' })
      if (isRejectedTask(result)) {
        void showDuplicateDialog()
        return false
      }
      return true
    } catch (e) {
      console.error('[File Transfer] enqueue upload failed:', e)
      return false
    }
  }

  async function pause(id: string): Promise<void> {
    await context.commands.execute('file-transfer.pause', { taskId: id })
  }
  async function resume(id: string): Promise<void> {
    await context.commands.execute('file-transfer.resume', { taskId: id })
  }
  async function cancel(id: string): Promise<void> {
    await context.commands.execute('file-transfer.cancel', { taskId: id })
  }
  async function retry(id: string): Promise<void> {
    await context.commands.execute('file-transfer.retry', { taskId: id })
  }
  async function resumeAll(): Promise<void> {
    await context.commands.execute('file-transfer.resume-all', {})
  }

  // ==================== 派生状态 ====================

  /** 队列汇总（底部栏与队列 sheet 数据源） */
  const summary = computed(() => {
    let active = 0
    let queued = 0
    let failed = 0
    let rejected = 0
    let resumable = 0
    let paused = 0
    let completed = 0
    let cancelled = 0
    for (const t of tasks.value) {
      switch (t.state) {
        case 'transferring': active++; break
        case 'queued': queued++; break
        case 'failed': failed++; break
        case 'rejected': rejected++; break
        case 'resumable': resumable++; break
        case 'paused': paused++; break
        case 'completed': completed++; break
        case 'cancelled': cancelled++; break
      }
    }
    return { active, queued, failed, rejected, resumable, paused, completed, cancelled }
  })

  /** 可恢复任务数（resume-all 按钮启用条件） */
  const resumableCount = computed(() => summary.value.resumable + summary.value.paused)

  /** 是否有未完成（非终态）任务 */
  const hasRunning = computed(() => tasks.value.some(t => !isTerminalState(t.state)))

  /** 传输中任务的总速率（快照差分与进度事件聚合取较大者，覆盖 500ms 窗口抖动） */
  const totalSpeed = computed(() => {
    let diff = 0
    for (const t of tasks.value) {
      if (t.state === 'transferring') diff += speedMap.value[t.id] ?? 0
    }
    return Math.max(diff, progressSpeed.value)
  })

  /** 当前主任务（迷你传输条展示：优先传输中，其次排队/可恢复；终态任务不入条） */
  const primaryTask = computed<Task | null>(() => {
    const order: TaskStateName[] = ['transferring', 'queued', 'resumable', 'paused']
    for (const state of order) {
      const t = tasks.value.find(t => t.state === state)
      if (t) return t
    }
    return null
  })

  /** 对端展示名（i18n key 或实际名字） */
  const displayPeerName = computed(() => {
    if (peerName.value) return peerName.value
    return peerId.value || context.i18n.t('transfer.peer.unpaired')
  })

  /** 队列全部完成/失败 → 系统通知（context.notifications），每批仅通知一次 */
  function checkSettledNotification(): void {
    const list = tasks.value
    if (list.length === 0) {
      notifiedSettled = false
      return
    }
    if (list.some(t => !isTerminalState(t.state))) {
      // 仍有活跃任务：重置结算标记，等待下一批
      notifiedSettled = false
      return
    }
    if (notifiedSettled) return
    notifiedSettled = true
    const completed = list.filter(t => t.state === 'completed').length
    const failed = list.filter(t => t.state === 'failed' || t.state === 'rejected').length
    const cancelled = list.filter(t => t.state === 'cancelled').length
    // 全部为用户取消 → 无需打扰
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

  /** 同名被拒对话框（spec 9.2：标题「无法上传」+ 单按钮「知道了」） */
  async function showDuplicateDialog(): Promise<void> {
    try {
      await context.dialogs.showDialog({
        title: context.i18n.t('transfer.dialog.duplicateTitle'),
        message: context.i18n.t('transfer.error.duplicateName'),
        variant: 'warning',
        confirmText: context.i18n.t('transfer.dialog.gotIt'),
        cancelText: context.i18n.t('transfer.dialog.gotIt'),
        dismissible: true,
      })
    } catch (e) {
      console.warn('[File Transfer] duplicate dialog failed:', e)
    }
  }

  // ==================== 生命周期 ====================

  /** 注册事件监听（组件 onMounted / 入口卡调用；context._disposables 亦会随插件停用清理） */
  function start(): void {
    stop()
    dispTasks = context.events.on('plugin:file-transfer:tasks-changed', onTasksChanged)
    dispProgress = context.events.on('plugin:transfer:progress', onProgress)
    dispPeer = context.events.on('filesrv:peer_changed', onPeerChanged)
    void refresh()
  }

  /** 摘除事件监听并清空差分缓存（组件 onUnmounted / 入口卡调用） */
  function stop(): void {
    dispTasks?.dispose()
    dispTasks = null
    dispProgress?.dispose()
    dispProgress = null
    dispPeer?.dispose()
    dispPeer = null
    offsetSamples.clear()
    progressSamples.clear()
    progressSpeed.value = 0
  }

  return {
    tasks,
    speedMap,
    summary,
    resumableCount,
    hasRunning,
    totalSpeed,
    primaryTask,
    peerOnline,
    peerId,
    peerName,
    displayPeerName,
    refresh,
    enqueueDownload,
    enqueueUpload,
    pause,
    resume,
    cancel,
    retry,
    resumeAll,
    showDuplicateDialog,
    start,
    stop,
  }
}
