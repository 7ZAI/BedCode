/**
 * 附近设备面板编排 — 发现 / 三态连接管理 / 活跃对端切换（host-peer 契约版）
 *
 * 数据源：插件事件 `plugin:file-transfer:devices-changed`（发现缓存全量替换）
 * 与 `plugin:file-transfer:connection-changed`（{ nodeId, connected } 连接态
 * 增量）。连接发起经 `file-transfer.dial-peer`（denied/unreachable 终态如实
 * 上报到行内错误），断开走 `file-transfer.disconnect-peer`，活跃对端切换走
 * `file-transfer.set-active-peer`。
 *
 * 语义边界：connOnline 由旧 WS 控制面事件 device-connected/disconnected 驱动
 * （终端远控链路的设备在线），与对等传输连接态（connectedIds）互不混淆——
 * 前者只影响顶栏 pill 文案，后者决定能否互传。
 *
 * 状态派生归并在 deviceState.deriveDeviceRows 纯函数中，本文件只做编排；
 * 测试以 mock PluginContext（commands.execute 记录 + events.on 捕获手动派发）
 * 驱动，见宿主测试套件 plugins/file-transfer 子目录。
 */
import { computed, ref, type Ref } from 'vue'
import type { Disposable, PluginContext } from '@binblink/plugin-sdk-desktop'
import {
  deriveDeviceRows,
  type DeviceRow,
  type DiscoveredDevice,
  type DialErrorStatus,
  type DialStatus,
} from './deviceState'

/** 可传输对端条目（工作台兼容形状） */
export interface PeerItem {
  id: string
  name: string
}

/** 激活对端派生状态 */
export interface PeerState {
  id: string
  name: string
  online: boolean
}

interface DevicePayload {
  nodeId?: string
  deviceName?: string
  addr?: string
  fileTransfer?: boolean
}

interface ConnectionPayload {
  nodeId?: string
  connected?: boolean
}

function withId(set: ReadonlySet<string>, id: string): Set<string> {
  return new Set(set).add(id)
}

function withoutId(set: ReadonlySet<string>, id: string): Set<string> {
  const next = new Set(set)
  next.delete(id)
  return next
}

export function usePeerDevices(context: PluginContext) {
  // ==================== 状态 ====================

  /** 发现缓存快照（事件全量替换；refresh 主动拉取首帧） */
  const devices = ref<DiscoveredDevice[]>([]) as Ref<DiscoveredDevice[]>
  /** 正在握手的节点集合（防重复点击与并发拨号同一节点） */
  const connectingIds = ref<ReadonlySet<string>>(new Set())
  /** 已连接节点集合（connection-changed 维护的宿主真相副本） */
  const connectedIds = ref<ReadonlySet<string>>(new Set())
  /** 最近一次拨号未成功状态（nodeId → denied/unreachable；成功即清除） */
  const dialErrors = ref<Record<string, DialErrorStatus>>({})
  /** 活跃对端 id（'' = 未选择；仅已连接节点可成为活跃对端） */
  const activePeerId = ref('') as Ref<string>
  /** WS 控制面连接态（device-connected/disconnected 驱动，语义独立于对等连接） */
  const connOnline = ref(false) as Ref<boolean>

  let started = false
  let disposables: Disposable[] = []

  // ==================== 派生 ====================

  /** 设备行（三态归并 + 行内错误 + 活跃标记），面板直接渲染 */
  const rows = computed<DeviceRow[]>(() =>
    deriveDeviceRows({
      devices: devices.value,
      connectingIds: connectingIds.value,
      connectedIds: connectedIds.value,
      dialErrors: dialErrors.value,
      activePeerId: activePeerId.value,
    }),
  )

  /** 具备传输能力的发现节点（工作台设备名映射用，不要求已连接） */
  const peers = computed<PeerItem[]>(() =>
    devices.value
      .filter((d) => d.fileTransfer !== false)
      .map((d) => ({ id: d.nodeId, name: d.deviceName || d.nodeId })),
  )

  /** 已连接且可传输的节点 id（活跃兜底切换候选） */
  const connectableConnectedIds = computed<string[]>(() =>
    devices.value
      .filter((d) => d.fileTransfer !== false && connectedIds.value.has(d.nodeId))
      .map((d) => d.nodeId),
  )

  /** 激活对端派生 */
  const peer = computed<PeerState>(() => {
    const item = devices.value.find((d) => d.nodeId === activePeerId.value)
    return item
      ? {
          id: item.nodeId,
          name: item.deviceName || item.nodeId,
          online: connectedIds.value.has(item.nodeId),
        }
      : { id: '', name: '', online: false }
  })

  // ==================== 内部辅助 ====================

  function setDialError(nodeId: string, status: DialErrorStatus): void {
    dialErrors.value = { ...dialErrors.value, [nodeId]: status }
  }

  function clearDialError(nodeId: string): void {
    if (!(nodeId in dialErrors.value)) return
    const next = { ...dialErrors.value }
    delete next[nodeId]
    dialErrors.value = next
  }

  /** 登记已连接并清除历史拨号错误 */
  function markConnected(nodeId: string): void {
    connectedIds.value = withId(connectedIds.value, nodeId)
    clearDialError(nodeId)
  }

  /** 活跃对端乐观切换：本地立即生效，命令失败仅记日志（事件/刷新兜底纠正） */
  function activateOptimistic(nodeId: string): void {
    activePeerId.value = nodeId
    context.commands
      .execute('file-transfer.set-active-peer', { peerId: nodeId })
      .catch((e: unknown) => {
        console.error('[File Transfer] set-active-peer failed:', e)
      })
  }

  /**
   * 活跃对端兜底：当前活跃节点已连接则保持；否则乐观切到首个已连接可传输节点。
   * 无任何已连接节点时不动（启动早期连接事件未到，不可凭空清掉宿主侧活跃态）。
   */
  function ensureActiveFallback(): boolean {
    const candidates = connectableConnectedIds.value
    if (candidates.length === 0 || candidates.includes(activePeerId.value)) return false
    activateOptimistic(candidates[0]!)
    return true
  }

  /** 活跃对端自身会话丢失的收尾：有其他候选则乐观切换，否则本地清空活跃 */
  function handleActiveLost(): void {
    const candidates = connectableConnectedIds.value
    if (candidates.length > 0) {
      activateOptimistic(candidates[0]!)
      return
    }
    activePeerId.value = ''
  }

  /** 发现快照全量替换（过滤无 nodeId 的畸形条目） */
  function applyDevices(payload: unknown): void {
    // 诊断插桩：区分「没收到事件 / 收到空 / 收到数据但渲染问题」（排查双端互不可见）
    console.log('[File Transfer] devices payload:', JSON.stringify(payload))
    if (!Array.isArray(payload)) return
    devices.value = (payload as DevicePayload[])
      .filter((d) => d && typeof d.nodeId === 'string' && d.nodeId !== '')
      .map((d) => ({
        nodeId: d.nodeId!,
        deviceName: d.deviceName ?? '',
        addr: d.addr ?? '',
        fileTransfer: d.fileTransfer !== false,
      }))
  }

  // ==================== 事件处理 ====================

  function handleDevicesChanged(payload: unknown): void {
    applyDevices(payload)
    ensureActiveFallback()
  }

  function handleConnectionChanged(payload: ConnectionPayload): void {
    if (!payload?.nodeId) return
    if (payload.connected) {
      markConnected(payload.nodeId)
      ensureActiveFallback()
    } else {
      connectedIds.value = withoutId(connectedIds.value, payload.nodeId)
      if (payload.nodeId === activePeerId.value) handleActiveLost()
    }
  }

  /** WS 控制面在线（仅 pill 文案语义，不触碰对等连接状态） */
  function handleControlPlaneConnected(): void {
    connOnline.value = true
  }

  // ==================== 对外操作 ====================

  /** 初始/手动刷新：拉发现快照 + 活跃对端（list-peers 旧契约携带 activePeerId） */
  async function refresh(): Promise<void> {
    try {
      const raw = await context.commands.execute('file-transfer.query-peer', {})
      applyDevices(raw)
      try {
        const legacy = await context.commands.execute('file-transfer.list-peers', {})
        if (typeof legacy?.activePeerId === 'string') {
          activePeerId.value = legacy.activePeerId
        }
      } catch {
        // list-peers 缺失不致命：活跃对端由兜底逻辑从已连接集合推导
      }
      ensureActiveFallback()
    } catch (e) {
      console.error('[File Transfer] query-peer failed:', e)
    }
  }

  /**
   * 发起对等连接：握手期间该节点进入 connecting 态（重复发起被拒绝），
   * denied / unreachable 以行内错误呈现。返回终态；null 表示本次未实际发起
   * （未发现 / 无能力 / 进行中的防御拦截）。
   */
  async function connect(nodeId: string): Promise<DialStatus | null> {
    const device = devices.value.find((d) => d.nodeId === nodeId)
    if (!device || !device.fileTransfer) return null
    if (connectingIds.value.has(nodeId) || connectedIds.value.has(nodeId)) return null

    connectingIds.value = withId(connectingIds.value, nodeId)
    clearDialError(nodeId)
    try {
      const result = await context.commands.execute('file-transfer.dial-peer', { nodeId })
      const status = result?.status
      if (status === 'connected') {
        markConnected(nodeId)
        // 活跃对端缺位或未连接时，新连接自动接管（多设备场景仍可手动切换）
        ensureActiveFallback()
        return 'connected'
      }
      const terminal: DialErrorStatus = status === 'denied' ? 'denied' : 'unreachable'
      setDialError(nodeId, terminal)
      return terminal
    } catch (e) {
      // 命令面异常（节点引擎未启动等）按不可达呈现，完整链路留在控制台
      console.error('[File Transfer] dial-peer failed:', e)
      setDialError(nodeId, 'unreachable')
      return 'unreachable'
    } finally {
      connectingIds.value = withoutId(connectingIds.value, nodeId)
    }
  }

  /** 断开对等连接：乐观摘除即时反馈；断的是活跃对端则按兜底规则收尾 */
  async function disconnect(nodeId: string): Promise<void> {
    connectedIds.value = withoutId(connectedIds.value, nodeId)
    if (nodeId === activePeerId.value) handleActiveLost()
    try {
      await context.commands.execute('file-transfer.disconnect-peer', { nodeId })
    } catch (e) {
      console.error('[File Transfer] disconnect-peer failed:', e)
    }
  }

  /** 切换活跃对端（含清空：id 为空串表示放弃当前活跃） */
  async function switchPeer(id: string): Promise<boolean> {
    try {
      await context.commands.execute('file-transfer.set-active-peer', { peerId: id })
      activePeerId.value = id
      return true
    } catch (e) {
      console.error('[File Transfer] set-active-peer failed:', e)
      return false
    }
  }

  // ==================== 生命周期 ====================

  /** 幂等启动：重复调用不叠加订阅（视图挂载/重挂载安全） */
  function start(): void {
    if (started) return
    started = true
    disposables = [
      context.events.on('plugin:file-transfer:devices-changed', handleDevicesChanged),
      context.events.on(
        'plugin:file-transfer:connection-changed',
        handleConnectionChanged,
      ),
      context.events.on('device-connected', handleControlPlaneConnected),
      context.events.on('device-disconnected', () => {
        connOnline.value = false
      }),
    ]
    void refresh()
  }

  function stop(): void {
    disposables.forEach((d) => d.dispose())
    disposables = []
    started = false
  }

  return {
    devices,
    rows,
    peers,
    peer,
    activePeerId,
    connOnline,
    connectingIds,
    connectedIds,
    dialErrors,
    connect,
    disconnect,
    switchPeer,
    refresh,
    start,
    stop,
  }
}
