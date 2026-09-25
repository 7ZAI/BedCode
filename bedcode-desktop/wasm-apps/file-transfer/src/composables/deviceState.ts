/**
 * 设备状态派生 — 无头纯函数（无 Vue / 插件运行时依赖，可独立直测）
 *
 * 把发现缓存快照与三份连接态记录（握手中集合 / 已连接集合 / 拨号失败表）
 * 归并为设备行列表：connected > connecting > idle 优先级归并，拨号失败
 * 仅在非连接态呈现；活跃对端标记要求已连接。组件只消费派生结果，
 * 编排 composable（usePeerDevices）负责维护输入。
 */

/** 发现的对端条目（mdns:found 事件派生 + 快照恢复的 camelCase 子集） */
export interface DiscoveredDevice {
  /** 完整节点 ID（64 位小写 hex，兼作指纹展示源） */
  nodeId: string
  /** 设备名（TXT `name`；缺失回退实例名末段） */
  deviceName: string
  /** 对端监听地址（IPv4，不带端口），缺失时留空 */
  addr?: string
  /** 对端监听端口（endpoint 拨号三元组） */
  port?: number
  /** 是否具备文件传输能力（缺省视为具备，兼容旧载荷/种子） */
  fileTransfer?: boolean
  /** 能力位图小写 hex（bit0 = 文件传输）；与 fileTransfer 双轨兼容 */
  capabilitiesHex?: string
  /** mDNS 实例全名（lost 事件按它寻源移除） */
  instanceName?: string
  /** 最近被发现时刻（前端 Date.now 盖章；快照恢复时为历史值） */
  lastSeenMs?: number
  /** 快照恢复标记：未被实时事件刷新前展示「最近可见」且不参与自动清扫 */
  restored?: boolean
}

/** 在线缓存 TTL：与引擎 PEER_DISCOVERY_TTL 同值（120s 广播周期口径） */
export const DEVICE_TTL_MS = 120_000

/** mDNS 实例名前缀（引擎 INSTANCE_PREFIX 对齐） */
const INSTANCE_PREFIX = 'bedcode-peer-'

/** 能力位 bit0 = 文件传输（引擎 CAP_FILE_TRANSFER 对齐） */
const CAP_FILE_TRANSFER_BIT = 0x1

/**
 * mdns:found 载荷解析（纯函数）：`{ instanceName, addresses, port,
 * txtRecords: { id, name, ver, cap } }` → 缓存条目。
 * TXT `id` 缺失/非法（非 64 位 hex）丢弃；设备名缺失回退实例名末段。
 */
export function parseFoundPayload(payload: unknown): DiscoveredDevice | null {
  if (!payload || typeof payload !== 'object') return null
  const p = payload as Record<string, unknown>
  const instanceName = typeof p.instanceName === 'string' ? p.instanceName : ''
  const txt = (p.txtRecords ?? {}) as Record<string, unknown>
  const nodeId = typeof txt.id === 'string' ? txt.id.toLowerCase() : ''
  if (!/^[0-9a-f]{64}$/.test(nodeId)) return null
  const addresses = Array.isArray(p.addresses) ? p.addresses.filter((a) => typeof a === 'string') : []
  const port = typeof p.port === 'number' ? p.port : 0
  const capHexRaw = typeof txt.cap === 'string' ? txt.cap.replace(/^0x/i, '') : ''
  const cap = Number.parseInt(capHexRaw, 16)
  const capabilitiesHex = Number.isNaN(cap) ? '' : cap.toString(16)
  const fallbackName = instanceName.startsWith(INSTANCE_PREFIX)
    ? instanceName.slice(INSTANCE_PREFIX.length)
    : instanceName
  const name = typeof txt.name === 'string' && txt.name !== '' ? txt.name : fallbackName
  const fileTransferByCap = Number.isNaN(cap) ? undefined : (cap & CAP_FILE_TRANSFER_BIT) !== 0
  return {
    nodeId,
    deviceName: name,
    addr: addresses[0] ?? '',
    port,
    // 能力位缺失时视为具备（兼容无 cap 的旧节点/种子）
    fileTransfer: fileTransferByCap ?? true,
    ...(capabilitiesHex !== '' ? { capabilitiesHex } : {}),
    ...(instanceName !== '' ? { instanceName } : {}),
  }
}

/**
 * lost 事件应用（纯函数）：实例名 → 短指纹（前 8 位）→ 按 nodeId 前缀匹配移除。
 * 返回新数组（不可变更新）。
 */
export function applyLost(list: readonly DiscoveredDevice[], instanceName: string): DiscoveredDevice[] {
  if (!instanceName.startsWith(INSTANCE_PREFIX)) return [...list]
  const short = instanceName.slice(INSTANCE_PREFIX.length, INSTANCE_PREFIX.length + 8)
  if (!short) return [...list]
  return list.filter((d) => !d.nodeId.startsWith(short))
}

/**
 * TTL 惰性清扫（纯函数）：移除 lastSeenMs 超 TTL 且未连接的条目（spec 步骤 1
 * 「读取列表时顺带清理」）。快照恢复条目按其历史 lastSeenMs 参与判定——重启后
 * 首屏先渲染，用户首次手动刷新时清算过期项；近期恢复的条目（< TTL）保留。
 */
export function sweepStaleDevices(
  list: readonly DiscoveredDevice[],
  nowMs: number,
  connectedIds: ReadonlySet<string>,
): DiscoveredDevice[] {
  return list.filter((d) => {
    if (connectedIds.has(d.nodeId)) return true
    if (!d.lastSeenMs) return true
    return nowMs - d.lastSeenMs < DEVICE_TTL_MS
  })
}

/** 拨号未成功终态 */
export type DialErrorStatus = 'denied' | 'unreachable'

/** 拨号命令返回状态（含成功） */
export type DialStatus = 'connected' | DialErrorStatus

/** 设备行归并三态 */
export type DeviceRowStatus = 'connected' | 'connecting' | 'idle'

/** 派生后的设备行（面板渲染模型） */
export interface DeviceRow {
  nodeId: string
  deviceName: string
  addr: string
  /** 无能力节点可见但不可发起连接 */
  fileTransfer: boolean
  status: DeviceRowStatus
  /** 拨号失败文案 key 后缀（denied/unreachable）；连接中/已连接时为 null */
  dialError: DialErrorStatus | null
  /** 是否为当前活跃对端（仅已连接态可为活跃） */
  isActive: boolean
  /** 快照恢复且未被实时事件刷新：展示「最近可见」标注（spec 故事 2） */
  recent: boolean
}

/** deriveDeviceRows 输入（全部为只读快照） */
export interface DeviceStateInput {
  devices: readonly DiscoveredDevice[]
  connectingIds: ReadonlySet<string>
  connectedIds: ReadonlySet<string>
  dialErrors: Readonly<Record<string, DialErrorStatus>>
  activePeerId: string
}

/**
 * 归并设备行：状态优先级 connected > connecting > idle；
 * 拨号失败只在 idle 态呈现（连接中/已连接即视为旧错误已消除）
 */
export function deriveDeviceRows(input: DeviceStateInput): DeviceRow[] {
  return input.devices.map((d) => {
    const connected = input.connectedIds.has(d.nodeId)
    const connecting = !connected && input.connectingIds.has(d.nodeId)
    const dialError =
      connected || connecting ? null : (input.dialErrors[d.nodeId] ?? null)
    return {
      nodeId: d.nodeId,
      deviceName: d.deviceName || d.nodeId,
      addr: d.addr ?? '',
      fileTransfer: d.fileTransfer !== false,
      status: connected ? 'connected' : connecting ? 'connecting' : 'idle',
      dialError,
      isActive: connected && d.nodeId === input.activePeerId,
      recent: d.restored === true && !connected,
    }
  })
}
