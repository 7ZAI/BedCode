/**
 * File Transfer 插件 dev-shell 领域种子类型（插件自有，SDK 不收录）
 *
 * SDK PluginDevMock 只约定「入口导出 devMock」的通用容器协议（Record<string, unknown>），
 * 不感知插件领域细节。本文件持有 file-transfer 对等域/传输域种子形状，
 * devMock.ts 与 dev-shell mock 消费时按这些形状 cast。
 */

/** 可信对端种子条目（插件本地扩展字段） */
export interface TrustedDevSeed {
  nodeId: string
  displayName: string | null
  fingerprintShort: string
  /** RFC3339 加入时间 */
  addedAt: string
}

/** 首连确认请求种子（dev-shell 延迟逐条推送 consent-requested 事件） */
export interface ConsentSeed {
  requestId: string
  nodeId: string
  fingerprintShort: string
  /** 设备名；null 时弹窗以短指纹兑底 + 身份提示展示 */
  deviceName: string | null
}

/** 设备种子（mdns:found 载荷形状 + 拨号行为标注） */
export interface PeerDeviceSeed {
  found: {
    instanceName: string
    addresses: string[]
    port: number
    txtRecords: { id: string; name: string; ver: string; cap: string }
  }
  dialBehavior: 'connected' | 'denied' | 'unreachable'
}

/** 对等域种子 */
export interface PeerDevMock {
  deviceSeeds: PeerDeviceSeed[]
  connectedNodeIds?: string[]
  activeNodeId?: string
  dialLatencyMs?: number
  consent?: ConsentSeed[]
  trusted?: TrustedDevSeed[]
}

/** 远端文件浏览种子：根清单 + 目录内容表（key = `${dirId}::${相对路径}`） */
export interface RemoteFsDevMock {
  roots: Array<{ id: string; name: string }>
  files?: Record<
    string,
    Array<{ name: string; size: number; mtime: number; isDir: boolean }>
  >
}

/** 本机共享设置种子（roots 为宿主 RootItem DTO 形状） */
export interface TransferSettingsDevMock {
  roots: Array<{ id: string; name: string; path?: string }>
  downloadDir?: string
  concurrency?: number
}

/** 传输域种子（任务列表为引擎 wire 形状，字段随引擎演进，dev-shell 按快照透传） */
export interface TransferDevMock {
  /** 活动条目（running/pending；终态条目请放 history） */
  tasks?: unknown[]
  /** 历史条目（终态：completed/failed/rejected/cancelled/interrupted） */
  history?: unknown[]
  remoteFs?: RemoteFsDevMock
  settings?: TransferSettingsDevMock
}

/** file-transfer devMock 容器（SDK PluginDevMock 通用协议的插件侧强类型视图） */
export interface FileTransferDevMock {
  peer?: PeerDevMock
  transfer?: TransferDevMock
}
