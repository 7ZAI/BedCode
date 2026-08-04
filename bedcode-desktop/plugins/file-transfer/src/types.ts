/**
 * File Transfer 插件业务类型
 *
 * 前端内部统一使用 camelCase 字段，WASM 侧字段命名差异（Task/RemoteEntry 快照为
 * snake_case、enqueue/get-settings 参数为 camelCase）在 composable 边界归一化，
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
