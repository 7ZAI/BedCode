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
  | 'waiting-approval'
  | 'completed'
  | 'failed'
  | 'rejected'
  | 'cancelled'

/** 任务发起方（v2 队列分类依据；wire snake_case） */
export type TaskInitiator = 'me' | 'peer'

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
  /** v2：发起方（队列分类依据；wire snake_case，默认 me） */
  initiator: TaskInitiator
  /** v2：所属批 ID（发送方上传任务） */
  batchId?: string | null
  /** 下载落点标记（M2/M3）：system=公共下载目录 / private=私有目录回退 /
   * saved-to=已保存到所选位置 / save-failed=保存失败保留私有副本 */
  place: string | null
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

/** 共享目录条目类型（与 WASM SharedRoot.kind 对应） */
export type SharedRootKind = 'saf' | 'private_downloads'

/** 共享目录条目（camelCase 内部模型；WASM 侧字段为 snake_case document_id） */
export interface SharedRoot {
  /** 条目 id：SAF 树 URI（content://tree/...）；特殊条目为真实路径 */
  id: string
  /** 条目类型：saf = SAF 树授权条目；private_downloads = 免授权特殊条目 */
  kind: SharedRootKind
  /** 展示名 */
  name: string
  /** SAF 根 document id（App 内遍历起点；特殊条目为空串） */
  documentId: string
  /** 授权有效性（check_authorized 结果回写；false = 已失效，需重新授权） */
  authorized: boolean
}

/** 共享目录条目（用于上传页文件列表；SAF 条目与真实路径条目同构） */
export interface SharedEntry {
  name: string
  isDir: boolean
  /** 文件大小（字节；目录/未知为 0） */
  size: number
  /** 条目 document URI（SAF 条目）；真实路径条目为绝对路径 */
  uri: string
  /** 条目 document id（子目录遍历用；真实路径条目为空串） */
  documentId: string
}

/** 免授权特殊条目 kind 常量（前端识别用） */
export const KIND_PRIVATE_DOWNLOADS: SharedRootKind = 'private_downloads'

/** 插件设置（camelCase 内部模型；get-settings 的 download_dir 在 composable 归一化） */
export interface Settings {
  /** 共享目录条目（含派生免授权特殊条目，kind=private_downloads） */
  roots: SharedRoot[]
  downloadDir: string
  concurrency: number
  /** v2 接收策略：ask（默认，每次询问）| accept（直接接收）| reject（直接拒绝） */
  receivingPolicy: 'ask' | 'accept' | 'reject'
  /** v2 同意超时秒（10–600，仅 ask 策略生效） */
  approvalTimeoutSec: number
}

/** 接收策略取值常量（与 WASM POLICY_* 一致） */
export const RECEIVING_POLICIES = ['ask', 'accept', 'reject'] as const
export type ReceivingPolicy = (typeof RECEIVING_POLICIES)[number]

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
  'waiting-approval': 'transfer.task.waitingApproval',
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
  'waiting-approval': 'ft-color-queued',
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
  'waiting-approval': 'ft-progress-queued',
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

// ==================== v2 接收端 / 历史类型 ====================

/** pending 批（接收端应答卡数据源；batches-changed / list-batches） */
export interface PendingBatch {
  batchId: string
  /** 对端名（宿主公告携带；缺失时为对端 ID） */
  peerName: string
  files: { relativePath: string; size: number }[]
  totalSize: number
  createdAt: number
}

/** 接收中任务（v2「正在接收」tab；仅可取消，无暂停/恢复） */
export interface ReceivingTask {
  sessionId: string
  batchId?: string | null
  /** 远端相对路径（= 目标文件名） */
  remotePath: string
  size: number
  /** transferring / completed / failed / rejected / cancelled */
  state: string
  reason?: string | null
  peerId: string
  createdAt: number
  updatedAt: number
}

/** 传输历史条目（list-history / history-changed） */
export interface HistoryEntry {
  id: string
  /** upload = 我发出；download = 我接收 */
  direction: TaskDirection
  /** 发起方：me | peer */
  initiator: TaskInitiator
  fileName: string
  size: number
  /** completed / failed / rejected / cancelled */
  state: string
  reason?: string | null
  peerName: string
  /** 仅 completed 且本地有文件时非空（打开所在文件夹用；移动接收任务恒缺） */
  localPath?: string | null
  createdAt: number
  updatedAt: number
}

/** 接收端 toast 请求载荷（plugin:file-transfer:toast） */
export interface TransferToastPayload {
  /** 对端名 */
  name: string
  /** 文件数 */
  count: number
  /** 总大小（仅 batch 模式） */
  totalSize?: number
  /** batch = 批级一条立即弹；per-file = 3s 窗口合并去重 */
  mode: 'batch' | 'per-file'
}

/** 拒绝原因 wire → 展示文案 key 后缀（§8.4 映射；unknown 兜底） */
export function mapRejectReasonKey(reason: string | null | undefined): string {
  switch (reason) {
    case 'duplicate-name': return 'duplicateName'
    case 'user-rejected': return 'rejectedByUser'
    case 'timeout': return 'noResponse'
    case 'policy-denied': return 'policyDenied'
    default: return 'unknown'
  }
}
