/**
 * File Transfer 插件业务类型 (Mobile)
 *
 * 前端内部统一使用 camelCase 字段；WASM 快照字段为 snake_case（Task/PeerInfo），
 * enqueue/get-settings 参数为 camelCase，归一化统一在 composable 边界完成。
 * 组件只消费本文件定义的干净类型。
 */

/** 任务方向（与 WASM TaskState::Direction serde lowercase 对应） */
export type TaskDirection = 'download' | 'upload'

/** 任务状态（与 WASM TaskState serde lowercase 对应） */
export type TaskStateName =
  | 'queued'
  | 'transferring'
  | 'paused'
  | 'resumable'
  | 'completed'
  | 'failed'
  | 'rejected'
  | 'cancelled'

/** 对端设备信息 */
export interface PeerInfo {
  deviceId: string
  name: string
}

/** 文件指纹（续传有效性校验） */
export interface Fingerprint {
  size: number
  mtime: number
}

/** 传输任务（camelCase 内部模型，由 WASM 快照映射而来） */
export interface Task {
  id: string
  direction: TaskDirection
  peer: PeerInfo
  remotePath: string
  localPath: string
  size: number
  offset: number
  uploadSessionId: string | null
  fingerprint: Fingerprint | null
  state: TaskStateName
  reason: string | null
  createdAt: number
  updatedAt: number
}

/** 远端目录项（list-remote 返回，isDir 为 WASM 显式 camelCase 字段） */
export interface RemoteEntry {
  name: string
  size: number
  mtime: number
  isDir: boolean
}

/** 插件设置（camelCase 内部模型；get-settings 的 download_dir 在 composable 归一化） */
export interface Settings {
  roots: string[]
  downloadDir: string
  concurrency: number
}

/**
 * TransferProgress.state 的 serde 形状（tag="state" content="reason"）：
 * - running  → { state: "running" }
 * - completed→ { state: "completed" }
 * - failed   → { state: "failed", reason: "..." }
 * - cancelled→ { state: "cancelled" }
 */
export type TransferProgressState =
  | { state: 'running' }
  | { state: 'completed' }
  | { state: 'failed'; reason: string }
  | { state: 'cancelled' }

/** 宿主传输引擎进度事件载荷（taskId 为宿主 UUID，非插件任务 id） */
export interface TransferProgress {
  taskId: string
  transferred: number
  total: number
  bytesPerSec: number
  state: TransferProgressState
}

/** 对端在线状态（filesrv:peer_changed 事件载荷） */
export interface PeerStatus {
  peerId: string
  online: boolean
  /** 对端真实设备名（宿主公告携带，可为空串） */
  deviceName?: string
  /** 对端 IP（宿主公告携带，可为空串） */
  ip?: string
}

/** 任务状态 → 展示文案 key（错误类附加 reason，见 TaskQueueSheet） */
export const TASK_STATE_KEYS: Record<TaskStateName, string> = {
  queued: 'transfer.task.state.queued',
  transferring: 'transfer.task.state.transferring',
  paused: 'transfer.task.state.paused',
  resumable: 'transfer.task.state.resumable',
  completed: 'transfer.task.state.completed',
  failed: 'transfer.task.state.failed',
  rejected: 'transfer.task.state.rejected',
  cancelled: 'transfer.task.state.cancelled',
}

/** 任务状态 → 四色体系文本色 class（spec 9.3，定义在注入的 styles.css） */
export const TASK_STATE_COLOR_CLASS: Record<TaskStateName, string> = {
  transferring: 'ft-color-active',
  queued: 'ft-color-queued',
  paused: 'ft-color-paused',
  resumable: 'ft-color-paused',
  completed: 'ft-color-completed',
  failed: 'ft-color-failed',
  rejected: 'ft-color-rejected',
  cancelled: 'ft-color-cancelled',
}

/** 任务状态 → 进度条底色 class（与文本色分离，进度条需实色底） */
export const TASK_STATE_PROGRESS_CLASS: Record<TaskStateName, string> = {
  transferring: 'ft-progress-active',
  queued: 'ft-progress-queued',
  paused: 'ft-progress-paused',
  resumable: 'ft-progress-paused',
  completed: 'ft-progress-completed',
  failed: 'ft-progress-failed',
  rejected: 'ft-progress-rejected',
  cancelled: 'ft-progress-cancelled',
}

/** 任务状态是否为终态（用于队列结算判定） */
export function isTerminalState(state: TaskStateName): boolean {
  return (
    state === 'completed' ||
    state === 'failed' ||
    state === 'rejected' ||
    state === 'cancelled'
  )
}
