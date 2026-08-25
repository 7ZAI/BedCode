/**
 * 设备状态派生 — 无头纯函数（无 Vue / 插件运行时依赖，可独立直测）
 *
 * 把发现缓存快照与三份连接态记录（握手中集合 / 已连接集合 / 拨号失败表）
 * 归并为设备行列表：connected > connecting > idle 优先级归并，拨号失败
 * 仅在非连接态呈现；活跃对端标记要求已连接。组件只消费派生结果，
 * 编排 composable（usePeerDevices）负责维护输入。
 */

/** 发现的对端条目（宿主 DiscoveredPeerDto 的 camelCase 子集） */
export interface DiscoveredDevice {
  /** 完整节点 ID（64 位小写 hex，兼作指纹展示源） */
  nodeId: string
  /** 设备名（mDNS 广播名） */
  deviceName: string
  /** 对端监听地址（IP:port），缺失时留空 */
  addr?: string
  /** 是否具备文件传输能力（缺省视为具备，兼容旧载荷） */
  fileTransfer?: boolean
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
    }
  })
}
