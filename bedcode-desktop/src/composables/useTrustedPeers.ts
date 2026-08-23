/**
 * 可信对端管理 — 对等网络信任层命令封装（issue 04）
 *
 * 列表/撤销走宿主命令；首连确认应答经 usePeerConsent 编排。
 * 字段名与后端 TrustedPeerDto（serde camelCase）对齐。
 */
import { invoke } from '@tauri-apps/api/core'

/** 可信对端条目（list_trusted_peers 返回） */
export interface TrustedPeer {
  nodeId: string
  /** 展示名：持久化名优先、在线缓存名次之；均缺为 null（UI 以短指纹兜底） */
  displayName: string | null
  fingerprintShort: string
  /** 加入可信列表时刻（RFC3339，展示层用 Intl.DateTimeFormat 本地化） */
  addedAt: string
}

/** 获取可信对端列表（节点停止时也可读——后端句柄独立于运行时存活） */
export function listTrustedPeers(): Promise<TrustedPeer[]> {
  return invoke<TrustedPeer[]>('list_trusted_peers')
}

/** 撤销可信对端；返回该 ID 原本是否存在。撤销后对端重连将重新走首连确认 */
export function revokeTrustedPeer(nodeId: string): Promise<boolean> {
  return invoke<boolean>('revoke_trusted_peer', { nodeId })
}
