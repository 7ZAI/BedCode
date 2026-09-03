/**
 * File Transfer 插件业务类型 (Mobile) — host-peer 契约版
 *
 * 前端内部统一 camelCase；宿主批级 DTO 由插件 Rust 代理翻译为
 * snake_case wire 形状，composable 边界归一化。组件只消费本文件的干净类型。
 */

/** 任务方向（wire lowercase） */
export type TaskDirection = 'download' | 'upload'

/** 任务状态（宿主托管后仅存在传输中与终态） */
export type TaskStateName =
  | 'transferring'
  | 'completed'
  | 'failed'
  | 'rejected'
  | 'cancelled'
  | 'interrupted'

/** 任务发起方（队列分类依据；wire snake_case） */
export type TaskInitiator = 'me' | 'peer'

/** 对端设备信息 */
export interface PeerInfo {
  deviceId: string
  name: string
}

/** 传输任务（一批 = 一条记录；由宿主 PeerTransferDto 翻译而来） */
export interface Task {
  /** 批 ID（宿主生成，cancel/retry 按其寻址） */
  id: string
  direction: TaskDirection
  peer: PeerInfo
  /** 展示名：首文件名（多文件追加 +N） */
  remotePath: string
  size: number
  /** 已传字节 */
  offset: number
  /** 瞬时速率 B/s（宿主滑动窗口） */
  rateBps: number
  state: TaskStateName
  reason: string | null
  initiator: TaskInitiator
  batchId?: string | null
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
  /** 发送加密开关（应用层 AES-256-GCM；默认关，接收端经 Offer 加密头自动解密） */
  encryption?: boolean
}

/** 接收策略取值常量（与宿主 policy mode 映射一致） */
export const RECEIVING_POLICIES = ['ask', 'accept', 'reject'] as const
export type ReceivingPolicy = (typeof RECEIVING_POLICIES)[number]

/** 任务状态 → 展示文案 key（错误类附加 reason，见 TaskQueueSheet） */
export const TASK_STATE_KEYS: Record<TaskStateName, string> = {
  transferring: 'transfer.task.state.transferring',
  completed: 'transfer.task.state.completed',
  failed: 'transfer.task.state.failed',
  rejected: 'transfer.task.state.rejected',
  cancelled: 'transfer.task.state.cancelled',
  interrupted: 'transfer.task.state.interrupted',
}

/** 任务状态 → 四色体系文本色 class（spec 9.3，定义在注入的 styles.css） */
export const TASK_STATE_COLOR_CLASS: Record<TaskStateName, string> = {
  transferring: 'ft-color-active',
  completed: 'ft-color-completed',
  failed: 'ft-color-failed',
  rejected: 'ft-color-failed',
  cancelled: 'ft-color-cancelled',
  interrupted: 'ft-color-cancelled',
}

/** 任务状态 → 进度条底色 class（与文本色分离，进度条需实色底） */
export const TASK_STATE_PROGRESS_CLASS: Record<TaskStateName, string> = {
  transferring: 'ft-progress-active',
  completed: 'ft-progress-completed',
  failed: 'ft-progress-failed',
  rejected: 'ft-progress-failed',
  cancelled: 'ft-progress-cancelled',
  interrupted: 'ft-progress-cancelled',
}

/** 任务状态是否为终态（用于队列结算判定） */
export function isTerminalState(state: TaskStateName): boolean {
  return (
    state === 'completed' ||
    state === 'failed' ||
    state === 'rejected' ||
    state === 'cancelled' ||
    state === 'interrupted'
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

/** 接收中任务（「正在接收」tab；一批 = 一条，仅可取消） */
export interface ReceivingTask {
  sessionId: string
  batchId?: string | null
  /** 展示名：首文件名（多文件追加 +N） */
  remotePath: string
  size: number
  offset: number
  /** running / completed / failed / rejected / cancelled */
  state: string
  reason?: string | null
  peerId: string
  peerName: string
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
