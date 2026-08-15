/**
 * 任务核心逻辑 (Mobile)
 *
 * 权威数据源为 WASM 推送的 `plugin:file-transfer:tasks-changed` 快照
 * （每次状态迁移/进度更新后由 WASM 全量推送），前端不做增量合并，
 * 直接整表替换避免状态漂移。
 *
 * `plugin:transfer:progress` 事件携带插件任务 id（宿主以插件 task_id 为
 * 事件 taskId），与任务快照同一命名空间；仍仅用于聚合瞬时总速率，
 * 作为快照差分速率的上限补充（任务级进度以 tasks-changed 快照为准）。
 *
 * 命令调用约定与桌面端同构（context.commands.execute('file-transfer.*')）。
 * 同名被拒（enqueue 返回 rejected / reason=duplicate-name）→ context.dialogs 弹
 * 「无法上传」对话框；队列全部完成/失败 → context.notifications 通知。
 */
import { ref, computed } from 'vue'
import type { Disposable, PluginContext } from '@bedcode/plugin-sdk-mobile'
import { getMobileApi } from '@bedcode/plugin-sdk-mobile'
import type {
  Task,
  TaskStateName,
  TransferProgress,
  PeerStatus,
  PendingBatch,
  ReceivingTask,
  HistoryEntry,
  TransferToastPayload,
} from '../types'
import { isTerminalState } from '../types'
import { MOCK_ENABLED, MOCK_PEER, mockTasks } from '../mock'

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
    initiator: raw.initiator === 'peer' ? 'peer' : 'me',
    batchId: raw.batch_id ?? raw.batchId ?? null,
    place: raw.place ?? null,
    createdAt: raw.created_at ?? raw.createdAt ?? 0,
    updatedAt: raw.updated_at ?? raw.updatedAt ?? 0,
  }
}

/** 将 WASM 接收任务快照项（camelCase，ReceivingTask serde）映射为前端模型 */
function mapWireReceiving(raw: any): ReceivingTask {
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

/** 将 WASM 历史条目快照（camelCase，HistoryEntry serde）映射为前端模型 */
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
    localPath: raw.local_path ?? raw.localPath ?? null,
    createdAt: raw.created_at ?? raw.createdAt ?? 0,
    updatedAt: raw.updated_at ?? raw.updatedAt ?? 0,
  }
}

/** 将 WASM pending 批快照项（batches-snapshot camelCase）映射为前端模型 */
function mapWireBatch(raw: any): PendingBatch {
  return {
    batchId: raw.batch_id ?? raw.batchId ?? '',
    peerName: raw.peer_name ?? raw.peerName ?? '',
    files: Array.isArray(raw.files) ? raw.files : [],
    totalSize: raw.total_size ?? raw.totalSize ?? 0,
    createdAt: raw.created_at ?? raw.createdAt ?? 0,
  }
}

/** 入队参数（下载/上传通用） */
export interface EnqueueArgs {
  direction: 'download' | 'upload'
  peerId: string
  peerName: string
  remotePath: string
  localPath?: string
  /** 上传完成后删除本地源文件（中转复制 cache 副本标记；真实路径源勿传） */
  cleanupLocal?: boolean
  /** 下载「保存到…」（M3）：完成时弹系统保存对话框（用户选位置），代替默认 MediaStore 落位 */
  saveTo?: boolean
  /** v2：声明的文件大小（字节，上传批请求 totalSize 与进度展示用；0 = 未知） */
  size?: number
  /** v2：所属批 ID（一次「发送」动作一匹；不传时 WASM 自动生成每任务一批） */
  batchId?: string
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

/**
 * 队列结算通知去重标记（模块级共享）。
 *
 * useTasks 会被多个组件实例化（主视图 FileTransferView + 常驻入口卡片
 * ToolboxEntry），若各实例独立结算，同一批失败会各自弹一次系统通知
 * （表现为「发了两条/不停发」）。共享标记保证每批只通知一次。
 */
let settledNotified = false
/** saveTo 结果 toast 去重（模块级共享，防多实例重复弹） */
const notifiedSaveTo = new Set<string>()

export function useTasks(context: PluginContext) {
  /** 任务列表（按 WASM 快照时间序，最新在前） */
  const tasks = ref<Task[]>([])
  /** v2 接收中任务（「正在接收」tab 数据源） */
  const receivingTasks = ref<ReceivingTask[]>([])
  /** v2 传输历史（「历史」tab 数据源） */
  const history = ref<HistoryEntry[]>([])
  /** v2 pending 批（接收端应答卡数据源） */
  const batches = ref<PendingBatch[]>([])
  /** 逐任务速率（快照差分，字节/秒） */
  const speedMap = ref<Record<string, number>>({})
  /** 进度事件聚合瞬时速率（host task id → bps 的存活窗口求和） */
  const progressSpeed = ref(0)

  /** WS 控制面连接状态（宿主 ws_* 事件驱动；与 peerOnline「对端已公告共享」分离） */
  const connOnline = ref(false)
  /** 对端已公告文件服务（filesrv:peer_changed，语义 = 对端已共享） */
  const peerOnline = ref(false)
  const peerId = ref('')
  /** 对端展示名：优先 tasks 中的 peer.name，其次对端 id */
  const peerName = ref('')

  /** 快照差分样本表（任务生命周期内持续累积） */
  const offsetSamples = new Map<string, OffsetSample>()
  /** 进度事件瞬时速率样本表（host task id → bps + 时间戳） */
  const progressSamples = new Map<string, { at: number; bps: number }>()

  let dispTasks: Disposable | null = null
  let dispProgress: Disposable | null = null
  let dispPeer: Disposable | null = null
  /** v2 接收端/历史事件监听 */
  let dispBatches: Disposable | null = null
  let dispReceiving: Disposable | null = null
  let dispHistory: Disposable | null = null
  let dispToast: Disposable | null = null
  /** WS 连接状态事件监听集合 */
  let dispConn: Disposable[] = []
  /** 开发期 mock：传输中任务进度推进定时器 */
  let mockTimer: ReturnType<typeof setInterval> | null = null
  /** 开发期 mock：入队任务自增序号 */
  let mockEnqueueSeq = 0

  /** v2 toast：per-file 3s 窗口合并去重（窗口内只更新计数不重复弹） */
  let perFileToastTimer: ReturnType<typeof setTimeout> | null = null
  let perFileToastPeer = ''
  let perFileToastCount = 0

  // 初始连接状态：读取宿主共享连接状态（视图挂载可能晚于 ws_paired 事件，
  // 事件驱动会漏掉已连接场景；host 未就绪（dev-shell）时保持 false 等事件）
  try {
    connOnline.value = getMobileApi().isConnected?.value === true
  } catch {
    connOnline.value = false
  }

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

    // 「保存到…」结果提示（M3）：下载任务完成且带落点标记时弹 toast
    //（saved-to = 已保存到所选位置；save-failed = 失败/取消，副本保留私有目录）
    for (const t of next) {
      if (t.direction === 'download' && t.state === 'completed' && t.place) {
        const key = `${t.id}:${t.place}`
        if (!notifiedSaveTo.has(key)) {
          notifiedSaveTo.add(key)
          if (t.place === 'saved-to') {
            context.dialogs.showToast(context.i18n.t('transfer.saveTo.saved'), 'success')
          } else if (t.place === 'save-failed') {
            context.dialogs.showToast(context.i18n.t('transfer.saveTo.failed'), 'error')
          }
        }
      }
    }

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

  /** 对端上下线事件（对端已公告/撤回文件服务） */
  function onPeerChanged(payload: PeerStatus): void {
    if (payload?.peerId) peerId.value = payload.peerId
    peerOnline.value = !!payload?.online
    // 对端真实设备名（桌面端 Announce 公告携带，替代任务名/占位文案）
    if (payload?.deviceName) peerName.value = payload.deviceName
    // 公告必然来自已认证连接：对端共享可用 ⇒ 连接必然已建立
    // （自愈视图挂载前 ws_paired 已发出导致 connOnline 未置位的场景）
    if (payload?.online) connOnline.value = true
  }

  /** v2 接收端 toast 请求：batch 模式立即弹；per-file 3s 窗口合并去重 */
  function onToast(payload: TransferToastPayload): void {
    if (!payload || payload.mode !== 'batch' && payload.mode !== 'per-file') return
    if (payload.mode === 'batch') {
      context.dialogs.showToast(
        context.i18n.t('transfer.toast.receiving', {
          name: payload.name || context.i18n.t('transfer.peer.unknown'),
          count: payload.count ?? 0,
        }),
        'info',
      )
      return
    }
    // per-file：3 秒窗口合并（窗口内新文件启动只更新计数不重复弹）——
    // 窗口内再次收到事件：重置窗口并重弹（宿主 toast 替换旧 toast，等效更新计数）
    perFileToastPeer = payload.name || perFileToastPeer
    perFileToastCount += payload.count ?? 1
    if (perFileToastTimer) clearTimeout(perFileToastTimer)
    const peer = perFileToastPeer
    const count = perFileToastCount
    context.dialogs.showToast(
      context.i18n.t('transfer.toast.receiving', { name: peer || context.i18n.t('transfer.peer.unknown'), count }),
      'info',
    )
    perFileToastTimer = setTimeout(() => {
      perFileToastTimer = null
      perFileToastPeer = ''
      perFileToastCount = 0
    }, 3000)
  }

  /** v2 pending 批快照事件（应答卡数据源） */
  function onBatchesChanged(payload: any): void {
    if (!Array.isArray(payload)) return
    batches.value = payload.map(mapWireBatch)
  }

  /** v2 接收任务快照事件 */
  function onReceivingChanged(payload: any): void {
    if (!Array.isArray(payload)) return
    receivingTasks.value = payload.map(mapWireReceiving)
  }

  /** v2 历史快照事件 */
  function onHistoryChanged(payload: any): void {
    if (!Array.isArray(payload)) return
    history.value = payload.map(mapWireHistory)
  }

  /** WS 连接状态事件（宿主 ws_* 事件；连接 ≠ 对端已共享） */
  function onConnChanged(online: boolean): void {
    connOnline.value = online
  }

  // ==================== 命令封装（与桌面同构） ====================

  /** 开发期 mock：构造入队任务（队列中新任务保持 queued，由用户手动恢复） */
  function mockEnqueuedTask(args: EnqueueArgs): Task {
    const now = Date.now()
    return {
      id: `mock-e${++mockEnqueueSeq}`,
      direction: args.direction,
      peer: { deviceId: args.peerId, name: args.peerName },
      remotePath: args.remotePath,
      localPath:
        args.localPath ??
        `/storage/emulated/0/Download/${args.remotePath.split('/').pop() ?? 'file'}`,
      size: args.size ?? 86_400_000,
      offset: 0,
      uploadSessionId: null,
      fingerprint: null,
      state: 'queued',
      reason: null,
      initiator: 'me',
      batchId: args.batchId ?? null,
      place: null,
      createdAt: now,
      updatedAt: now,
    }
  }

  /** 开发期 mock：本地改写任务状态并重放快照（差分速率随之归零/重算） */
  function mockMutateState(id: string, state: TaskStateName, reason: string | null = null): void {
    const next = tasks.value.map((tk) =>
      tk.id === id ? { ...tk, state, reason, updatedAt: Date.now() } : tk,
    )
    applySnapshot(next)
  }

  /** 开发期 mock：填充对端 + 任务快照，并匀速推进传输中任务进度（约 2MB/s） */
  function startMock(): void {
    connOnline.value = true
    peerOnline.value = true
    peerId.value = MOCK_PEER.id
    peerName.value = MOCK_PEER.name
    applySnapshot(mockTasks())
    mockTimer = setInterval(() => {
      const now = Date.now()
      const next = tasks.value.map((tk) => {
        if (tk.state !== 'transferring' || tk.size <= 0 || tk.offset >= tk.size) return tk
        return { ...tk, offset: Math.min(tk.size, tk.offset + 2_400_000), updatedAt: now }
      })
      if (next.some((tk, i) => tk.offset !== tasks.value[i].offset)) applySnapshot(next)
    }, 1200)
  }

  /** 拉取全量任务（宿主重启后补同步一次；mock 下直接返回本地快照） */
  async function refresh(): Promise<void> {
    if (MOCK_ENABLED) {
      applySnapshot(mockTasks())
      return
    }
    try {
      const data = await context.commands.execute('file-transfer.list-tasks', {})
      const arr = Array.isArray(data) ? data : (data?.tasks ?? [])
      applySnapshot(arr)
    } catch (e) {
      console.error('[File Transfer] list-tasks failed:', e)
    }
  }

  /** 入队单个任务（返回命令结果；被拒任务由调用方决定弹窗；mock 下直接入本地队列） */
  async function enqueue(args: EnqueueArgs): Promise<any> {
    if (MOCK_ENABLED) {
      const task = mockEnqueuedTask(args)
      applySnapshot([task, ...tasks.value])
      return { id: task.id, state: 'queued' }
    }
    return context.commands.execute('file-transfer.enqueue', {
      direction: args.direction,
      peerId: args.peerId,
      peerName: args.peerName,
      remotePath: args.remotePath,
      localPath: args.localPath ?? null,
      cleanupLocal: args.cleanupLocal ?? false,
      saveTo: args.saveTo ?? false,
      size: args.size ?? 0,
      batchId: args.batchId ?? null,
    })
  }

  /**
   * 主动询问对端文件服务状态（防止状态事件遗漏时无法恢复）
   *
   * 经宿主 WS 控制面发 Query，对端回复后宿主推送 filesrv:peer_changed，
   * peerOnline/peerId/peerName 自动刷新；失败静默（连接未建立时宿主直接忽略）。
   */
  async function queryPeer(): Promise<boolean> {
    if (MOCK_ENABLED) {
      connOnline.value = true
      peerOnline.value = true
      peerId.value = MOCK_PEER.id
      peerName.value = MOCK_PEER.name
      return true
    }
    try {
      await context.commands.execute('file-transfer.query-peer', {})
      return true
    } catch (e) {
      console.error('[File Transfer] query-peer failed:', e)
      return false
    }
  }

  /**
   * 批量入队下载（逐个入队，单个失败不中断整批）。
   * 任一同名被拒即弹「无法上传」Material 对话框（context.dialogs）。
   * saveTo（M3）：每项下载完成后弹系统保存对话框（用户选位置）。
   */
  async function enqueueDownload(
    paths: string[],
    peer: { id: string; name: string },
    options?: { saveTo?: boolean },
  ): Promise<number> {
    let ok = 0
    for (const remotePath of paths) {
      try {
        const result = await enqueue({
          direction: 'download',
          peerId: peer.id,
          peerName: peer.name,
          remotePath,
          saveTo: options?.saveTo ?? false,
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
    if (MOCK_ENABLED) {
      mockMutateState(id, 'paused')
      return
    }
    await context.commands.execute('file-transfer.pause', { taskId: id })
  }
  async function resume(id: string): Promise<void> {
    if (MOCK_ENABLED) {
      mockMutateState(id, 'transferring')
      return
    }
    await context.commands.execute('file-transfer.resume', { taskId: id })
  }
  async function cancel(id: string): Promise<void> {
    if (MOCK_ENABLED) {
      mockMutateState(id, 'cancelled')
      return
    }
    await context.commands.execute('file-transfer.cancel', { taskId: id })
  }
  async function retry(id: string): Promise<void> {
    if (MOCK_ENABLED) {
      mockMutateState(id, 'queued')
      return
    }
    await context.commands.execute('file-transfer.retry', { taskId: id })
  }
  async function removeTask(id: string): Promise<void> {
    if (MOCK_ENABLED) {
      applySnapshot(tasks.value.filter((tk) => tk.id !== id))
      return
    }
    await context.commands.execute('file-transfer.remove-task', { taskId: id })
  }
  /** 用系统查看器打开已完成任务的本地文件（仅 completed 有落盘文件） */
  async function openTask(id: string): Promise<void> {
    const task = tasks.value.find((tk) => tk.id === id)
    if (!task || task.state !== 'completed' || !task.localPath) return
    // 下载方向 local_path 为 .part 临时名，完成后已 rename 到最终路径（去后缀）
    const finalPath = task.localPath.endsWith('.part')
      ? task.localPath.slice(0, -'.part'.length)
      : task.localPath
    // MediaStore 按文件名命中公共下载副本（displayName = 远端文件名）
    const displayName = task.remotePath.split('/').pop() ?? ''
    try {
      await context.system.openFile(finalPath, displayName)
    } catch (err) {
      console.error(`[File Transfer] open failed for "${finalPath}":`, err)
      context.dialogs.showToast(String(err), 'error')
    }
  }
  async function resumeAll(): Promise<void> {
    if (MOCK_ENABLED) {
      const next = tasks.value.map((tk) =>
        tk.state === 'paused' || tk.state === 'resumable'
          ? { ...tk, state: 'transferring' as TaskStateName, updatedAt: Date.now() }
          : tk,
      )
      applySnapshot(next)
      return
    }
    await context.commands.execute('file-transfer.resume-all', {})
  }

  // ==================== v2 接收端命令与历史 ====================

  /** 批准传输批（应答卡「接受全部」） */
  async function approveBatch(batchId: string): Promise<void> {
    if (MOCK_ENABLED) {
      batches.value = batches.value.filter((b) => b.batchId !== batchId)
      return
    }
    try {
      await context.commands.execute('file-transfer.approve-batch', { batchId })
    } catch (e) {
      console.error(`[File Transfer] approve-batch failed for "${batchId}":`, e)
    }
  }

  /** 拒绝传输批（应答卡「拒绝全部」） */
  async function rejectBatch(batchId: string): Promise<void> {
    if (MOCK_ENABLED) {
      batches.value = batches.value.filter((b) => b.batchId !== batchId)
      return
    }
    try {
      await context.commands.execute('file-transfer.reject-batch', { batchId })
    } catch (e) {
      console.error(`[File Transfer] reject-batch failed for "${batchId}":`, e)
    }
  }

  /** 取消接收中的上传会话（「正在接收」tab 仅此操作） */
  async function cancelReceiving(sessionId: string): Promise<void> {
    if (MOCK_ENABLED) {
      receivingTasks.value = receivingTasks.value.filter((r) => r.sessionId !== sessionId)
      return
    }
    try {
      await context.commands.execute('file-transfer.cancel-receiving', { sessionId })
    } catch (e) {
      console.error(`[File Transfer] cancel-receiving failed for "${sessionId}":`, e)
    }
  }

  /** 清空传输历史 */
  async function clearHistory(): Promise<void> {
    if (MOCK_ENABLED) {
      history.value = []
      return
    }
    try {
      await context.commands.execute('file-transfer.clear-history', {})
    } catch (e) {
      console.error('[File Transfer] clear-history failed:', e)
    }
  }

  /** 打开历史条目的本地文件所在目录（仅 completed 且带 localPath；文件管理器展示） */
  async function openHistoryEntry(entry: HistoryEntry): Promise<void> {
    if (entry.state !== 'completed' || !entry.localPath) return
    // 与 openTask 一致：.part 临时名还原为最终路径（完成后已 rename）
    const finalPath = entry.localPath.endsWith('.part')
      ? entry.localPath.slice(0, -'.part'.length)
      : entry.localPath
    try {
      await context.system.revealInDir(finalPath)
    } catch (err) {
      console.error(`[File Transfer] reveal failed for "${finalPath}":`, err)
      context.dialogs.showToast(String(err), 'error')
    }
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
    if (peerId.value) return peerId.value
    // 已连接但尚未收到对端公告（未共享）：无可辨识信息时用「未知设备」占位，
    // 避免与「未连接」文案混用
    if (connOnline.value) return context.i18n.t('transfer.peer.unknown')
    return context.i18n.t('transfer.peer.unpaired')
  })

  /** 队列全部完成/失败 → 系统通知（context.notifications），每批仅通知一次 */
  function checkSettledNotification(): void {
    const list = tasks.value
    if (list.length === 0) {
      settledNotified = false
      return
    }
    if (list.some(t => !isTerminalState(t.state))) {
      // 仍有活跃任务：重置结算标记，等待下一批
      settledNotified = false
      return
    }
    if (settledNotified) return
    settledNotified = true
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
    // v2 接收端快照事件：pending 批 / 接收任务 / 历史 / toast（桌面端 useReceiving
    // 同构；漏订阅会导致应答卡、历史、接收中列表在前端永不更新——事件到达插件后
    // 无人接收，前端只能靠 onMounted 的一次性 refreshV2 拿到空快照）
    dispBatches = context.events.on('plugin:file-transfer:batches-changed', onBatchesChanged)
    dispReceiving = context.events.on('plugin:file-transfer:receiving-changed', onReceivingChanged)
    dispHistory = context.events.on('plugin:file-transfer:history-changed', onHistoryChanged)
    dispToast = context.events.on('plugin:file-transfer:toast', onToast)
    // WS 控制面连接状态：已连接（含重连成功）/ 断开（含重连中与失败）
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
    if (MOCK_ENABLED) startMock()
    else void refresh()
  }

  /** 拉取 v2 初始快照（pending 批 + 接收任务 + 历史；宿主重启后补同步） */
  async function refreshV2(): Promise<void> {
    if (MOCK_ENABLED) return
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
      console.error('[File Transfer] list batches/receiving/history failed:', e)
    }
  }

  /** 摘除事件监听并清空差分缓存（组件 onUnmounted / 入口卡调用） */
  function stop(): void {
    dispTasks?.dispose()
    dispTasks = null
    dispProgress?.dispose()
    dispProgress = null
    dispPeer?.dispose()
    dispPeer = null
    dispBatches?.dispose()
    dispBatches = null
    dispReceiving?.dispose()
    dispReceiving = null
    dispHistory?.dispose()
    dispHistory = null
    dispToast?.dispose()
    dispToast = null
    dispConn.forEach(d => d.dispose())
    dispConn = []
    if (mockTimer) {
      clearInterval(mockTimer)
      mockTimer = null
    }
    if (perFileToastTimer) {
      clearTimeout(perFileToastTimer)
      perFileToastTimer = null
      perFileToastPeer = ''
      perFileToastCount = 0
    }
    offsetSamples.clear()
    progressSamples.clear()
    progressSpeed.value = 0
  }

  return {
    tasks,
    receivingTasks,
    history,
    batches,
    speedMap,
    summary,
    resumableCount,
    hasRunning,
    totalSpeed,
    primaryTask,
    connOnline,
    peerOnline,
    peerId,
    peerName,
    displayPeerName,
    refresh,
    refreshV2,
    enqueueDownload,
    enqueueUpload,
    queryPeer,
    pause,
    resume,
    cancel,
    retry,
    removeTask,
    openTask,
    resumeAll,
    approveBatch,
    rejectBatch,
    cancelReceiving,
    clearHistory,
    openHistoryEntry,
    showDuplicateDialog,
    start,
    stop,
  }
}
