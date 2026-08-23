/**
 * 任务核心逻辑 (Desktop) — host-peer 契约版
 *
 * 权威数据源为插件 WASM 转发的宿主对等事件（批级模型）：
 * - tasks-changed：发送方向 PeerTransferDto[] → Task（一批一条记录）
 * - devices-changed：DiscoveredPeerDto[]，前端挑选具备文件传输能力的设备
 *
 * 命令调用经 context.commands.execute('file-transfer.*')，由插件 Rust 侧
 * 薄代理转发 host.peer_* 并翻译 wire 形状。
 */
import { ref, computed, type Ref } from 'vue'
import type { Disposable, PluginContext } from '@binblink/plugin-sdk-desktop'
import type { Task, TaskStateName } from '../types'
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

/** 发现设备（devices-changed 事件项） */
interface DeviceInfo {
  nodeId: string
  deviceName: string
  fileTransfer?: boolean
}

export function useTasks(context: PluginContext) {
  /** 发送方向任务列表（批级） */
  const tasks = ref<Task[]>([]) as Ref<Task[]>
  /** 活跃传输总速率（B/s） */
  const totalSpeed = ref(0) as Ref<number>

  /** 存在具备文件传输能力的发现设备 */
  const peerOnline = ref(false) as Ref<boolean>
  const peerId = ref('') as Ref<string>
  const peerName = ref('') as Ref<string>

  let dispTasks: Disposable | null = null
  let dispDevices: Disposable | null = null

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
      void context.commands
        .execute('file-transfer.set-active-peer', { peerId: capable.nodeId })
        .catch(() => {})
    } else {
      peerOnline.value = false
    }
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

  /** 刷新设备列表（探测回复后由 devices-changed 事件驱动状态更新） */
  async function queryPeer(): Promise<boolean> {
    try {
      const devices = await context.commands.execute('file-transfer.query-peer', {})
      onDevicesChanged(devices)
      return peerOnline.value
    } catch (e) {
      console.error('[File Transfer] query-peer failed:', e)
      return false
    }
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
    dispDevices = context.events.on('plugin:file-transfer:devices-changed', onDevicesChanged)
    void refresh()
    void queryPeer()
  }

  function stop(): void {
    dispTasks?.dispose()
    dispTasks = null
    dispDevices?.dispose()
    dispDevices = null
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
