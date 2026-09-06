/**
 * 附近设备面板编排 — 自建设备缓存版（issue 13 Phase 3 步骤 1）
 *
 * 数据源：`mdns-found` / `mdns-lost` 插件事件（宿主 mdns:* 原样透传）驱动的
 * 前端缓存状态机（去重 / TTL / 展示名 / 能力位见 deviceState 纯函数）；
 * `connection-changed` 维护连接态。缓存经 `save-device-snapshot` debounce
 * 落盘（含 last-seen），activate 首屏由 `get-device-snapshot` 恢复并标注
 * 「最近可见」（spec 故事 2）。连接动作携带显式 endpoint 三元组走
 * `dial-peer`（denied/unreachable 行内错误如实上报），断开走
 * `disconnect-peer`。
 */
import { computed, ref, type Ref } from 'vue'
import type { Disposable, PluginContext } from '@binblink/plugin-sdk-mobile'
import {
  applyLost,
  deriveDeviceRows,
  parseFoundPayload,
  sweepStaleDevices,
  type DeviceRow,
  type DiscoveredDevice,
  type DialErrorStatus,
  type DialStatus,
} from './deviceState'

/** 快照落盘 debounce 窗口（毫秒） */
const SNAPSHOT_SAVE_DEBOUNCE_MS = 2000

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

/** 快照恢复条目形状（get-device-snapshot 返回，camelCase） */
interface SnapshotEntry {
  nodeId: string
  deviceName?: string
  addr?: string
  port?: number
  capabilitiesHex?: string
  instanceName?: string
  lastSeenMs?: number
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

  /** 自建设备缓存（found/lost/TTL 三路事件驱动；快照恢复首屏） */
  const devices = ref<DiscoveredDevice[]>([]) as Ref<DiscoveredDevice[]>
  /** 正在握手的节点集合（防重复点击与并发拨号同一节点） */
  const connectingIds = ref<ReadonlySet<string>>(new Set())
  /** 已连接节点集合（connection-changed 维护的宿主真相副本） */
  const connectedIds = ref<ReadonlySet<string>>(new Set())
  /** 最近一次拨号未成功状态（nodeId → denied/unreachable；成功即清除） */
  const dialErrors = ref<Record<string, DialErrorStatus>>({})
  /** 活跃对端 id（'' = 未选择；仅已连接节点可成为活跃对端） */
  const activePeerId = ref('') as Ref<string>

  let started = false
  let disposables: Disposable[] = []
  let saveTimer: ReturnType<typeof setTimeout> | null = null

  // ==================== 派生 ====================

  /** 设备行（三态归并 + 行内错误 + 活跃标记 + 最近可见标注），面板直接渲染 */
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

  /** 缓存 upsert（不可变更新；found 即刷新 last-seen 并摘除快照恢复标记） */
  function upsertDevice(device: DiscoveredDevice): void {
    const stamped: DiscoveredDevice = { ...device, lastSeenMs: Date.now(), restored: false }
    const idx = devices.value.findIndex((d) => d.nodeId === device.nodeId)
    if (idx >= 0) {
      const next = [...devices.value]
      next[idx] = { ...next[idx], ...stamped }
      devices.value = next
    } else {
      devices.value = [...devices.value, stamped]
    }
    scheduleSnapshotSave()
  }

  /** 缓存快照 debounce 落盘（≤50 条裁剪；失败静默——下次变更重试） */
  function scheduleSnapshotSave(): void {
    if (saveTimer) clearTimeout(saveTimer)
    saveTimer = setTimeout(() => {
      saveTimer = null
      const entries: SnapshotEntry[] = devices.value.slice(0, 50).map((d) => ({
        nodeId: d.nodeId,
        deviceName: d.deviceName ?? '',
        addr: d.addr ?? '',
        port: d.port ?? 0,
        capabilitiesHex: d.capabilitiesHex ?? '',
        instanceName: d.instanceName ?? '',
        lastSeenMs: d.lastSeenMs ?? 0,
      }))
      context.commands.execute('file-transfer.save-device-snapshot', { devices: entries }).catch(
        (e: unknown) => {
          console.error('[File Transfer] save-device-snapshot failed:', e)
        },
      )
    }, SNAPSHOT_SAVE_DEBOUNCE_MS)
  }

  // ==================== 事件处理 ====================

  function handleFound(payload: unknown): void {
    const device = parseFoundPayload(payload)
    if (!device) return
    upsertDevice(device)
    ensureActiveFallback()
  }

  function handleLost(payload: unknown): void {
    const instanceName =
      payload && typeof payload === 'object'
        ? String((payload as Record<string, unknown>).instanceName ?? '')
        : ''
    if (!instanceName) return
    const next = applyLost(devices.value, instanceName)
    if (next.length !== devices.value.length) {
      devices.value = next
      scheduleSnapshotSave()
    }
  }

  function handleConnectionChanged(payload: ConnectionPayload): void {
    if (!payload?.nodeId) return
    if (payload.connected) {
      markConnected(payload.nodeId)
      ensureActiveFallback()
      // 入站（被连侧）连接没有拨号 memo：数据面命令（浏览/拉取/发送）经它
      // 重拨，endpoint 取自自建缓存（mdns-found 维护）
      rememberEndpoint(payload.nodeId)
    } else {
      connectedIds.value = withoutId(connectedIds.value, payload.nodeId)
      if (payload.nodeId === activePeerId.value) handleActiveLost()
    }
  }

  /** 入站连接的对端寻址登记（fire-and-forget；缓存缺失时跳过，对端不可达由命令错误呈现） */
  function rememberEndpoint(nodeId: string): void {
    const device = devices.value.find((d) => d.nodeId === nodeId)
    if (!device?.addr || !device.port) return
    context.commands
      .execute('file-transfer.remember-peer-endpoint', {
        endpoint: { nodeId, addr: device.addr, port: device.port },
      })
      .catch((e: unknown) => console.error('[File Transfer] remember-peer-endpoint failed:', e))
  }

  // ==================== 对外操作 ====================

  /** 向宿主请求发现刷新：宿主立即重查 + 缓存/连接态以事件重发（按钮直达后端，
   * 挂载晚于引擎发现事件时首屏即有数据） */
  function requestHostRefresh(): void {
    context.commands.execute('file-transfer.refresh-devices', {}).catch((e: unknown) => {
      console.error('[File Transfer] refresh-devices failed:', e)
    })
  }

  /**
   * 手动刷新：TTL 惰性清扫（含清算快照恢复条目）+ 落盘 + 宿主发现刷新。
   * 发现数据由 mDNS 事件流驱动，本操作触发宿主即时重查加速数据到位。
   */
  async function refresh(): Promise<void> {
    devices.value = sweepStaleDevices(devices.value, Date.now(), connectedIds.value)
    requestHostRefresh()
    scheduleSnapshotSave()
  }

  /**
   * 发起对等连接：握手期间该节点进入 connecting 态（重复发起被拒绝），
   * denied / unreachable 以行内错误呈现。endpoint 三元组取自自建缓存；
   * 返回终态；null 表示本次未实际发起（未发现/无能力/进行中的防御拦截）。
   */
  async function connect(nodeId: string): Promise<DialStatus | null> {
    const device = devices.value.find((d) => d.nodeId === nodeId)
    if (!device || !device.fileTransfer) return null
    if (!device.addr || !device.port) {
      setDialError(nodeId, 'unreachable')
      return 'unreachable'
    }
    if (connectingIds.value.has(nodeId) || connectedIds.value.has(nodeId)) return null

    connectingIds.value = withId(connectingIds.value, nodeId)
    clearDialError(nodeId)
    try {
      await context.commands.execute('file-transfer.dial-peer', {
        endpoint: { nodeId, addr: device.addr, port: device.port },
      })
      markConnected(nodeId)
      ensureActiveFallback()
      return 'connected'
    } catch (e) {
      // 命令面异常：denied 字样按拒绝呈现，其余按不可达（完整链路留在控制台）
      console.error('[File Transfer] dial-peer failed:', e)
      const msg = e instanceof Error ? e.message : String(e)
      setDialError(nodeId, msg.includes('denied') ? 'denied' : 'unreachable')
      return msg.includes('denied') ? 'denied' : 'unreachable'
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

  /** 幂等启动：先恢复快照首屏（「最近可见」标注），再订阅实时事件流 */
  function start(): void {
    if (started) return
    started = true
    disposables = [
      context.events.on('plugin:file-transfer:mdns-found', handleFound),
      context.events.on('plugin:file-transfer:mdns-lost', handleLost),
      context.events.on(
        'plugin:file-transfer:connection-changed',
        handleConnectionChanged,
      ),
    ]
    void (async () => {
      try {
        const snap = await context.commands.execute('file-transfer.get-device-snapshot', {})
        const list: DiscoveredDevice[] = Array.isArray(snap?.devices)
          ? (snap.devices as SnapshotEntry[])
              .filter((d) => typeof d?.nodeId === 'string' && d.nodeId !== '')
              .map((d) => ({
                nodeId: d.nodeId,
                deviceName: d.deviceName ?? '',
                addr: d.addr ?? '',
                port: d.port ?? 0,
                fileTransfer:
                  d.capabilitiesHex === ''
                    ? true
                    : (Number.parseInt(d.capabilitiesHex, 16) & 1) !== 0,
                ...(d.instanceName ? { instanceName: d.instanceName } : {}),
                lastSeenMs: d.lastSeenMs ?? 0,
                restored: true,
              }))
          : []
        if (list.length > 0 && devices.value.length === 0) devices.value = list
      } catch (e) {
        console.error('[File Transfer] get-device-snapshot failed:', e)
      }
    })()
    // 首屏即向宿主要一次发现/连接快照（挂载晚于引擎发现与连接建立时不空屏）
    requestHostRefresh()
  }

  function stop(): void {
    disposables.forEach((d) => d.dispose())
    disposables = []
    started = false
    if (saveTimer) {
      clearTimeout(saveTimer)
      saveTimer = null
    }
  }

  return {
    devices,
    rows,
    peers,
    peer,
    activePeerId,
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
