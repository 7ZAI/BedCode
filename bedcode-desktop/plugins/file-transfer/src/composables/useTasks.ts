/**
 * 任务核心逻辑 (Desktop) — 自有存储 wire 版（issue 13 Phase 3）
 *
 * 权威数据源为插件 WASM 自持存储的派生事件（tasks-changed，引擎
 * PeerTransferDto camelCase 形状 + retryMeta 扩展）：本文件只做一层
 * camelCase wire → 前端内部模型的映射，组件消费模型不变。
 */
import { ref, computed, type Ref } from 'vue'
import type { Disposable, PluginContext } from '@binblink/plugin-sdk-desktop'
import type { Task, TaskStateName } from '../types'
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
    localPath: null,
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

export function useTasks(context: PluginContext) {
  /** 发送方向任务列表（批级） */
  const tasks = ref<Task[]>([]) as Ref<Task[]>
  /** 活跃传输总速率（B/s） */
  const totalSpeed = ref(0) as Ref<number>

  /** 存在具备文件传输能力的发现设备（devices 由 usePeerDevices 维护；本处仅镜像布尔） */
  const peerOnline = ref(false) as Ref<boolean>
  const peerId = ref('') as Ref<string>
  const peerName = ref('') as Ref<string>

  let dispTasks: Disposable | null = null

  /** 任务/速率快照整表替换（对端名镜像自活跃条目，供迷你条展示） */
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
  }

  // ==================== 命令封装 ====================

  /** 拉取发送方向任务（初始同步；自持存储读命令） */
  async function refresh(): Promise<void> {
    try {
      const data = await context.commands.execute('file-transfer.list-tasks', {})
      if (Array.isArray(data)) applySnapshot(data)
    } catch (e) {
      console.error('[File Transfer] list-tasks failed:', e)
    }
  }

  /** 兼容保留：设备在线探测已由 usePeerDevices 的事件流接管，此处仅回当前值 */
  async function queryPeer(): Promise<boolean> {
    return peerOnline.value
  }

  /** 系统选择器多选文件直发活跃对端（返回成功入队的文件数，0 = 取消/失败） */
  async function sendPickedFiles(): Promise<number> {
    try {
      const raw = await context.commands.execute('file-transfer.pick-files', {})
      const paths: string[] = Array.isArray(raw) ? raw : []
      if (paths.length === 0) return 0
      await context.commands.execute('file-transfer.enqueue', { paths })
      return paths.length
    } catch (e) {
      console.error('[File Transfer] pick/send failed:', e)
      return 0
    }
  }

  async function cancel(id: string): Promise<void> {
    await context.commands.execute('file-transfer.cancel', { taskId: id })
  }
  async function retry(id: string): Promise<void> {
    await context.commands.execute('file-transfer.retry', { taskId: id })
  }

  // ==================== 派生状态 ====================

  /** 是否有未完成任务 */
  const hasRunning = computed(() => tasks.value.some((t) => !isTerminalState(t.state)))

  /** 当前主任务（迷你传输条展示：优先传输中） */
  const primaryTask = computed<Task | null>(
    () => tasks.value.find((t) => t.state === 'transferring') ?? null,
  )

  // ==================== 生命周期 ====================

  /** 注册事件监听 */
  function start(): void {
    stop()
    dispTasks = context.events.on('plugin:file-transfer:tasks-changed', (payload: unknown) => {
      if (Array.isArray(payload)) applySnapshot(payload)
    })
    void refresh()
  }

  function stop(): void {
    dispTasks?.dispose()
    dispTasks = null
    totalSpeed.value = 0
  }

  return {
    tasks,
    totalSpeed,
    hasRunning,
    primaryTask,
    peerOnline,
    peerId,
    peerName,
    refresh,
    queryPeer,
    sendPickedFiles,
    cancel,
    retry,
    start,
    stop,
  }
}
