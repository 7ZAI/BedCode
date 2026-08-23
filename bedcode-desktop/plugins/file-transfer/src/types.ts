/**
 * File Transfer 插件业务类型 — host-peer 契约版
 *
 * 宿主批级 DTO 由插件 Rust 代理翻译为 snake_case wire 形状，composable
 * 边界归一化为 camelCase 内部模型；组件只消费本文件定义的干净类型。
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

/** 发起方（wire snake_case，默认 me） */
export type TaskInitiator = 'me' | 'peer'

/** 对端设备信息 */
export interface PeerInfo {
  deviceId: string
  name: string
}

/** 传输任务（一批 = 一条记录；由宿主 PeerTransferDto 翻译而来） */
export interface Task {
  /** 批 ID（cancel/retry 按其寻址） */
  id: string
  direction: TaskDirection
  peer: PeerInfo
  /** 展示名：首文件名（多文件追加 +N） */
  remotePath: string
  size: number
  offset: number
  /** 瞬时速率 B/s（宿主滑动窗口） */
  rateBps: number
  state: TaskStateName
  reason: string | null
  createdAt: number
  updatedAt: number
  initiator: TaskInitiator
  batchId?: string | null
}

/** 远端目录项（list-remote 返回） */
export interface RemoteEntry {
  name: string
  size: number
  mtime: number
  isDir: boolean
}

/** 插件设置（roots 条目清单由 useSettings.rootItems 维护，含移除寻址 id） */
export interface Settings {
  downloadDir: string
  concurrency: number
  /** 接收策略：ask 每次询问 / accept 直接接收 / reject 直接拒绝 */
  receivingPolicy: 'ask' | 'accept' | 'reject'
  /** 同意超时（秒，10–600，仅 ask 生效） */
  approvalTimeoutSec: number
}

/** 任务状态是否为终态 */
export function isTerminalState(state: TaskStateName): boolean {
  return (
    state === 'completed' ||
    state === 'failed' ||
    state === 'rejected' ||
    state === 'cancelled'
  )
}

/** pending 批（接收端应答卡数据源，list-batches 返回） */
export interface PendingBatch {
  batchId: string
  peerId: string
  peerName: string
  files: { relativePath: string; size: number }[]
  totalSize: number
  createdAt: number
}

/** 接收中任务（「正在接收」tab，list-receiving 返回） */
export interface ReceivingTask {
  sessionId: string
  batchId: string | null
  remotePath: string
  size: number
  offset?: number
  state: string
  reason: string | null
  peerId: string
  peerName?: string
  createdAt: number
  updatedAt: number
}

/** 传输历史条目（list-history 返回） */
export interface HistoryEntry {
  id: string
  direction: TaskDirection
  initiator: TaskInitiator
  fileName: string
  size: number
  state: 'completed' | 'failed' | 'rejected' | 'cancelled'
  reason: string | null
  peerName: string
  localPath: string | null
  createdAt: number
  updatedAt: number
}
