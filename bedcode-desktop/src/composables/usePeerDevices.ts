/**
 * 设备发现列表编排 — 对等网络设备面（issue 08）
 *
 * 数据源为宿主发现缓存：start 时拉一次全量，此后监听 `peer-devices-changed`
 * 事件随节点上下线自动刷新（宿主侧快照比对驱动推送，前端免轮询 IPC）。
 * 连接发起经 `dial_peer` 命令——对端确认后宿主持有连接句柄并发
 * `peer-connected` 事件，已连接徽标以此为准；断开/节点停止由
 * `peer-disconnected` 事件摘除。与 usePeerConsent 同款模块级单例模式。
 */
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

/** 发现的对端条目（后端 DiscoveredPeerDto，camelCase） */
export interface DiscoveredPeer {
  /** 完整节点 ID（64 位小写 hex） */
  nodeId: string
  /** 设备名（mDNS 广播名，缺失时后端已回退实例名） */
  deviceName: string
  /** 对端监听地址（IP:port） */
  addr: string
  /** 通告协议版本 */
  protocolVersion: number
  /** 原始能力位图（供未来能力位扩展展示） */
  capabilities: number
  /** 是否具备文件传输能力（bit0；无此能力的节点可见但不可发起传输连接） */
  fileTransfer: boolean
}

/** 拨号结果三态：connected 已连接 / denied 对端拒绝 / unreachable 不可达 */
export type PeerDialStatus = 'connected' | 'denied' | 'unreachable'

/** dial_peer 命令返回（后端 DialPeerResultDto） */
export interface DialPeerResult {
  status: PeerDialStatus
  deviceName: string | null
}

// ==================== 模块级共享状态（跨组件单例） ====================

/** 发现缓存快照（宿主事件全量替换；start 时主动拉取首帧） */
const peers = ref<DiscoveredPeer[]>([])
/** 正在拨号的节点集合（防重复点击与并发拨号同一节点） */
const connectingIds = ref<ReadonlySet<string>>(new Set())
/** 已连接节点集合（peer-connected/disconnected 事件维护的宿主真相副本） */
const connectedIds = ref<ReadonlySet<string>>(new Set())
/** 最近一次拨号未成功状态（nodeId → denied/unreachable；连接成功即清除） */
const dialErrors = ref<Record<string, PeerDialStatus>>({})

let unlistenFns: UnlistenFn[] = []
let started = false

/** 不可变集合更新辅助：以新 Set 替换触发响应式（避免依赖深层集合插桩） */
function withId(set: ReadonlySet<string>, id: string): Set<string> {
  return new Set(set).add(id)
}

function withoutId(set: ReadonlySet<string>, id: string): Set<string> {
  const next = new Set(set)
  next.delete(id)
  return next
}

function setDialError(nodeId: string, status: PeerDialStatus): void {
  dialErrors.value = { ...dialErrors.value, [nodeId]: status }
}

function clearDialError(nodeId: string): void {
  if (!(nodeId in dialErrors.value)) return
  const next = { ...dialErrors.value }
  delete next[nodeId]
  dialErrors.value = next
}

async function refresh(): Promise<void> {
  try {
    peers.value = await invoke<DiscoveredPeer[]>('list_discovered_peers')
  } catch (error) {
    console.error('[PeerDevices] list discovered peers failed:', error)
  }
}

/**
 * 发起对等连接：对端确认后进入已连接态
 *
 * 无能力/未发现/进行中的操作直接拒绝发起（按钮层已禁用，此处防御兜底）。
 * 返回终态供调用方提示；null 表示本次调用未实际发起。
 */
async function connect(nodeId: string): Promise<PeerDialStatus | null> {
  const peer = peers.value.find((p) => p.nodeId === nodeId)
  if (!peer || !peer.fileTransfer) return null
  if (connectingIds.value.has(nodeId) || connectedIds.value.has(nodeId)) return null

  connectingIds.value = withId(connectingIds.value, nodeId)
  clearDialError(nodeId)
  try {
    const result = await invoke<DialPeerResult>('dial_peer', { nodeId })
    if (result.status === 'connected') {
      connectedIds.value = withId(connectedIds.value, nodeId)
      clearDialError(nodeId)
    } else {
      setDialError(nodeId, result.status)
    }
    return result.status
  } catch (error) {
    // 命令面异常（节点未启动等）按不可达呈现，完整链路留在控制台
    console.error('[PeerDevices] dial failed:', error)
    setDialError(nodeId, 'unreachable')
    return 'unreachable'
  } finally {
    connectingIds.value = withoutId(connectingIds.value, nodeId)
  }
}

/** 断开对等连接：乐观摘除徽标即时反馈，命令失败仅记日志（徽标由 refresh 兜底） */
async function disconnect(nodeId: string): Promise<void> {
  connectedIds.value = withoutId(connectedIds.value, nodeId)
  try {
    await invoke<boolean>('disconnect_peer', { nodeId })
  } catch (error) {
    console.error('[PeerDevices] disconnect failed:', error)
  }
}

/**
 * 设备列表控制器：应用生命周期内幂等调用一次 `start`（页面挂载时触发即可）
 *
 * 监听宿主三类事件并维护本地状态副本；首帧经 list_discovered_peers 主动
 * 拉取，保证页面晚于推送启动时也能立即得到当前快照。
 */
async function start(): Promise<void> {
  if (started) return
  started = true
  unlistenFns.push(
    await listen<DiscoveredPeer[]>('peer-devices-changed', (event) => {
      peers.value = event.payload
    }),
    await listen<{ nodeId: string }>('peer-connected', (event) => {
      connectedIds.value = withId(connectedIds.value, event.payload.nodeId)
      clearDialError(event.payload.nodeId)
    }),
    await listen<{ nodeId: string }>('peer-disconnected', (event) => {
      connectedIds.value = withoutId(connectedIds.value, event.payload.nodeId)
    }),
  )
  await refresh()
}

export function usePeerDevices() {
  return { peers, connectingIds, connectedIds, dialErrors, start, refresh, connect, disconnect }
}

// ==================== 测试辅助 ====================

/** 重置模块级状态（仅测试用：用例间隔离共享单例） */
export function _resetPeerDevicesForTest(): void {
  unlistenFns.forEach((fn) => fn())
  unlistenFns = []
  started = false
  peers.value = []
  connectingIds.value = new Set()
  connectedIds.value = new Set()
  dialErrors.value = {}
}
