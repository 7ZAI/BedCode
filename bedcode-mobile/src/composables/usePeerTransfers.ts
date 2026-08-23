/**
 * 传输任务编排 — 对等网络发送侧（issue 09）
 *
 * 数据源为宿主任务表：start 时拉一次全量（活跃 + 历史），此后监听
 * `peer-transfer-changed` 全量列表事件随进度/终态自动刷新（宿主侧节流推送，
 * 前端免轮询 IPC）。扇出发送经 `send_files_to_peer` 按接收方逐台发起——
 * 领域模型为 N 条相互独立的传输批，任一被拒不影响其余；取消/重试/清空历史
 * 直连对应命令。与 usePeerDevices 同款模块级单例模式。
 */
import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

/** 批内单文件元数据 */
export interface PeerTransferFile {
  /** 发送方相对路径（`/` 分隔） */
  path: string
  /** 文件字节数 */
  size: number
}

/** 任务状态：running 进行中，其余为终态（进历史） */
export type PeerTransferStatus = 'running' | 'completed' | 'rejected' | 'cancelled' | 'failed'

/** 单条传输任务/历史记录（后端 PeerTransferDto，camelCase） */
export interface PeerTransferTask {
  batchId: string
  nodeId: string
  peerName: string
  /** 方向：send | receive（receive 由 issue 10 接入） */
  direction: string
  status: PeerTransferStatus
  files: PeerTransferFile[]
  totalBytes: number
  transferredBytes: number
  /** 瞬时速率（B/s） */
  rateBps: number
  detail?: string | null
  /** 拒绝原因（status=rejected 时存在；wire kebab-case） */
  rejectReason?: string | null
  createdAtMs: number
  updatedAtMs: number
}

/** 扇出单台设备的发起结果 */
export interface SendOutcome {
  nodeId: string
  ok: boolean
  batchId?: string
}

// ==================== 模块级共享状态（跨组件单例） ====================

/** 全量任务列表（宿主事件全量替换；最新在前） */
const transfers = ref<PeerTransferTask[]>([])

let unlistenFns: UnlistenFn[] = []
let started = false

function isTerminal(status: PeerTransferStatus): boolean {
  return status !== 'running'
}

async function refresh(): Promise<void> {
  try {
    transfers.value = await invoke<PeerTransferTask[]>('list_peer_transfers')
  } catch (error) {
    console.error('[PeerTransfers] list transfers failed:', error)
  }
}

/**
 * 扇出发送：对每个选中设备独立发起一条传输批
 *
 * 经 Promise.allSettled 保证相互独立——单台失败/拒绝不阻断其余设备；
 * 返回逐台结果供调用方反馈。paths 为空或无目标时直接返回不发命令。
 */
async function sendToPeers(paths: string[], nodeIds: string[]): Promise<SendOutcome[]> {
  if (paths.length === 0 || nodeIds.length === 0) return []
  const results = await Promise.allSettled(
    nodeIds.map((nodeId) =>
      invoke<PeerTransferTask>('send_files_to_peer', { nodeId, paths }).then((task) => ({
        nodeId,
        ok: true,
        batchId: task.batchId,
      })),
    ),
  )
  return results.map((result, index) => {
    const nodeId = nodeIds[index]!
    if (result.status === 'fulfilled') return result.value
    console.error(`[PeerTransfers] send to ${nodeId} failed:`, result.reason)
    return { nodeId, ok: false }
  })
}

/** 取消进行中的传输（已终态幂等无害） */
async function cancel(batchId: string): Promise<void> {
  try {
    await invoke('cancel_peer_transfer', { batchId })
  } catch (error) {
    console.error('[PeerTransfers] cancel failed:', error)
  }
}

/** 重试终态任务：同批 ID 续传，接收端从断点继续 */
async function retry(batchId: string): Promise<boolean> {
  try {
    await invoke('retry_peer_transfer', { batchId })
    return true
  } catch (error) {
    // 源清单丢失（重启后）/对端离线等：错误链如实上抛给调用方提示
    console.error('[PeerTransfers] retry failed:', error)
    return false
  }
}

/** 清空传输历史（进行中任务不受影响）；返回清除条数 */
async function clearHistory(): Promise<number> {
  try {
    const removed = await invoke<number>('clear_peer_transfer_history')
    return removed ?? 0
  } catch (error) {
    console.error('[PeerTransfers] clear history failed:', error)
    return 0
  }
}

/**
 * 传输任务控制器：应用生命周期内幂等调用一次 `start`（页面挂载时触发即可）
 *
 * 监听宿主全量列表事件并维护本地状态副本；首帧主动拉取兜底页面晚启动。
 */
async function start(): Promise<void> {
  if (started) return
  started = true
  unlistenFns.push(
    await listen<PeerTransferTask[]>('peer-transfer-changed', (event) => {
      transfers.value = event.payload
    }),
  )
  await refresh()
}

export function usePeerTransfers() {
  /** 进行中任务（活跃区） */
  const activeTransfers = computed(() => transfers.value.filter((t) => !isTerminal(t.status)))
  /** 终态任务（历史区） */
  const historyTransfers = computed(() => transfers.value.filter((t) => isTerminal(t.status)))
  return {
    transfers,
    activeTransfers,
    historyTransfers,
    start,
    refresh,
    sendToPeers,
    cancel,
    retry,
    clearHistory,
  }
}

// ==================== 测试辅助 ====================

/** 重置模块级状态（仅测试用：用例间隔离共享单例） */
export function _resetPeerTransfersForTest(): void {
  unlistenFns.forEach((fn) => fn())
  unlistenFns = []
  started = false
  transfers.value = []
}
