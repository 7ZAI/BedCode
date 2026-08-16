/**
 * 任务列表核心逻辑
 *
 * 权威数据源为 WASM 推送的 `plugin:file-transfer:tasks-changed` 快照
 * （每次状态迁移/进度更新后由 WASM 全量推送），前端不做增量合并，
 * 直接整表替换避免状态漂移。
 *
 * `plugin:transfer:progress` 事件携带插件任务 id（宿主以插件 task_id 为
 * 事件 taskId），与任务快照同一命名空间；仍仅用于聚合瞬时总速率，
 * 作为快照差分速率的上限补充（任务级进度以 tasks-changed 快照为准）。
 */
import { ref, computed, type Ref } from 'vue'
import type { Disposable, PluginContext } from '@binblink/plugin-sdk-desktop'
import type { Task, TaskStateName, TransferProgress } from '../types'

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
    initiator: raw.initiator === 'peer' ? 'peer' : 'me',
    batchId: raw.batch_id ?? raw.batchId ?? null,
  }
}

/** 状态 → 展示文案 key（错误类额外附原因，见 TaskPanel） */
export const TASK_STATE_KEYS: Record<TaskStateName, string> = {
  queued: 'transfer.task.state.queued',
  'waiting-approval': 'transfer.task.waitingApproval',
  transferring: 'transfer.task.state.transferring',
  paused: 'transfer.task.state.paused',
  resumable: 'transfer.task.state.resumable',
  completed: 'transfer.task.state.completed',
  failed: 'transfer.task.state.failed',
  rejected: 'transfer.task.state.rejected',
  cancelled: 'transfer.task.state.cancelled',
}

export function useTasks(context: PluginContext) {
  /** 任务列表（按 WASM 快照时间序，最新在前） */
  const tasks = ref<Task[]>([]) as Ref<Task[]>
  /** 逐任务速率（快照差分，字节/秒） */
  const speedMap = ref<Record<string, number>>({}) as Ref<Record<string, number>>
  /** 进度事件聚合瞬时速率（host task id → bps 的存活窗口求和） */
  const progressSpeed = ref(0)

  /** 快照差分样本表（任务生命周期内持续累积） */
  const offsetSamples = new Map<string, OffsetSample>()
  /** 进度事件瞬时速率样本表（host task id → bps + 时间戳） */
  const progressSamples = new Map<string, { at: number; bps: number }>()

  let dispTasks: Disposable | null = null
  let dispProgress: Disposable | null = null

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

  // ==================== 命令封装 ====================

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

  /**
   * 主动询问对端文件服务状态（防止状态事件遗漏时无法恢复）
   *
   * 经宿主 WS 控制面广播 Query，对端回复后宿主推送 filesrv:peer_changed，
   * peer 状态自动刷新；失败静默（无已认证客户端时广播为空操作）。
   */
  async function queryPeer(): Promise<boolean> {
    try {
      await context.commands.execute('file-transfer.query-peer', {})
      return true
    } catch (e) {
      console.error('[File Transfer] query-peer failed:', e)
      return false
    }
  }

  /** 批量入队下载（逐个入队，单个失败不中断整批） */
  async function enqueueDownload(
    paths: string[],
    peer: { id: string; name: string },
  ): Promise<number> {
    let ok = 0
    for (const remotePath of paths) {
      try {
        await context.commands.execute('file-transfer.enqueue', {
          direction: 'download',
          peerId: peer.id,
          peerName: peer.name,
          remotePath,
        })
        ok++
      } catch (e) {
        console.error(`[File Transfer] enqueue failed for "${remotePath}":`, e)
      }
    }
    return ok
  }

  /**
   * 批量入队上传（本地文件 → 对端共享根目录，spec §9.1「发送到手机」）
   *
   * remotePath 为目标相对路径（仅文件名，上传到对端当前挂载根）；
   * 逐个入队，单个失败不中断整批。
   * v2：一次「发送」动作生成一个批 ID（batchId），批内任务经
   * transfer-request 协议统一询问接收端（ask 策略），批准后免钩子直传。
   */
  async function enqueueUpload(
    localFiles: string[],
    peer: { id: string; name: string },
  ): Promise<number> {
    // 批 ID：webview 为安全上下文，crypto.randomUUID 可用；不可用时降级时间戳
    const batchId =
      typeof crypto !== 'undefined' && 'randomUUID' in crypto
        ? crypto.randomUUID()
        : `b-${Date.now().toString(16)}`
    let ok = 0
    for (const localPath of localFiles) {
      const name = localPath.split(/[\\/]/).pop() ?? localPath
      try {
        await context.commands.execute('file-transfer.enqueue', {
          direction: 'upload',
          peerId: peer.id,
          peerName: peer.name,
          remotePath: name,
          localPath,
          batchId,
        })
        ok++
      } catch (e) {
        console.error(`[File Transfer] upload enqueue failed for "${localPath}":`, e)
      }
    }
    return ok
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
  async function removeTask(id: string): Promise<void> {
    await context.commands.execute('file-transfer.remove-task', { taskId: id })
  }
  /** 在系统文件管理器中显示已完成任务的本地文件（仅 completed 有落盘文件） */
  async function openInDir(id: string): Promise<void> {
    const task = tasks.value.find((tk) => tk.id === id)
    if (!task || task.state !== 'completed' || !task.localPath) return
    // 兼容历史产物快照：旧 wasm 曾产出 `\\?\` verbatim 前缀 + 混合分隔符路径，
    // 宿主 canonicalize 前 exists 会报错（os error 123）；剥前缀后由宿主原生化。
    const path = task.localPath.replace(/^\\\\\\?\\/, '')
    // 下载方向 local_path 为 .part 临时名，完成后已 rename 到最终路径（去后缀，
    // 与 wasm 侧 strip_suffix 一致只剥一次，避免 `x.part.part` 类文件名错位）
    const finalPath = path.endsWith('.part')
      ? path.slice(0, -'.part'.length)
      : path
    // 诊断：点击「打开目录」时打印实际解析出的定位路径
    console.log(`[File Transfer] openInDir task=${task.id} dir=${task.direction} raw=${task.localPath} -> ${finalPath}`)
    try {
      await context.system.revealInDir(finalPath)
    } catch (err) {
      // 与 enqueue 失败同模式：仅 console 记录，不打断用户操作流
      console.error(`[File Transfer] reveal failed for "${finalPath}":`, err)
    }
  }
  async function resumeAll(): Promise<void> {
    await context.commands.execute('file-transfer.resume-all', {})
  }

  // ==================== 派生状态 ====================

  /** 队列汇总（状态 chips 数据源；v2：waiting-approval 计入排队类） */
  const summary = computed(() => {
    let active = 0
    let queued = 0
    let failed = 0
    let rejected = 0
    let resumable = 0
    let paused = 0
    for (const t of tasks.value) {
      switch (t.state) {
        case 'transferring': active++; break
        case 'queued':
        case 'waiting-approval': queued++; break
        case 'failed': failed++; break
        case 'rejected': rejected++; break
        case 'resumable': resumable++; break
        case 'paused': paused++; break
        default: break
      }
    }
    return { active, queued, failed, rejected, resumable, paused }
  })

  /** 可恢复任务数（resume-all 按钮启用条件） */
  const resumableCount = computed(() => summary.value.resumable + summary.value.paused)

  /** 传输中任务的总速率（快照差分与进度事件聚合取较大者，覆盖 500ms 窗口抖动） */
  const totalSpeed = computed(() => {
    let diff = 0
    for (const t of tasks.value) {
      if (t.state === 'transferring') diff += speedMap.value[t.id] ?? 0
    }
    return Math.max(diff, progressSpeed.value)
  })

  // ==================== 生命周期 ====================

  /** 注册事件监听（组件 onMounted 调用；context._disposables 亦会随插件停用清理） */
  function start(): void {
    stop()
    dispTasks = context.events.on('plugin:file-transfer:tasks-changed', onTasksChanged)
    dispProgress = context.events.on('plugin:transfer:progress', onProgress)
    void refresh()
  }

  /** 摘除事件监听并清空差分缓存（组件 onUnmounted 调用） */
  function stop(): void {
    dispTasks?.dispose()
    dispTasks = null
    dispProgress?.dispose()
    dispProgress = null
    offsetSamples.clear()
    progressSamples.clear()
    progressSpeed.value = 0
  }

  return {
    tasks,
    speedMap,
    summary,
    resumableCount,
    totalSpeed,
    refresh,
    enqueueDownload,
    enqueueUpload,
    queryPeer,
    pause,
    resume,
    cancel,
    retry,
    removeTask,
    openInDir,
    resumeAll,
    start,
    stop,
  }
}
