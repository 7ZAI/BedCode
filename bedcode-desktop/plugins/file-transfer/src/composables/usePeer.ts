/**
 * 对端（设备）列表与激活设备状态 — host-peer 契约版
 *
 * 权威数据源：插件 WASM 转发的 `plugin:file-transfer:devices-changed`
 * （DiscoveredPeerDto[]，宿主 emit_json 同步发布到总线）。前端挑选首个
 * 具备文件传输能力的节点为活跃对端；`device-connected`/`device-disconnected`
 * 事件驱动 WS 控制面连接态（connOnline ≠ 对端可传输）。
 */
import { computed, ref, type Ref } from 'vue'
import type { Disposable, PluginContext } from '@binblink/plugin-sdk-desktop'

/** 在线对端（设备）条目 */
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

interface DeviceInfo {
  nodeId: string
  deviceName: string
  fileTransfer?: boolean
}

export function usePeer(context: PluginContext) {
  /** 可传输设备列表 */
  const peers = ref<PeerItem[]>([]) as Ref<PeerItem[]>
  /** 激活设备 id（'' = 无可用对端） */
  const activePeerId = ref('') as Ref<string>

  /** 激活对端派生 */
  const peer = computed<PeerState>(() => {
    const item = peers.value.find((p) => p.id === activePeerId.value)
    return item
      ? { id: item.id, name: item.name || item.id, online: true }
      : { id: '', name: '', online: false }
  })

  /** WS 控制面连接状态（device-connected / device-disconnected 驱动） */
  const connOnline = ref(false) as Ref<boolean>

  let dispDevices: Disposable | null = null
  let dispDevice: Disposable | null = null
  let dispDeviceDisc: Disposable | null = null

  function applyDevices(payload: unknown): void {
    if (!Array.isArray(payload)) return
    const capable = payload.filter(
      (d: DeviceInfo) => d.fileTransfer !== false && !!d.nodeId,
    ) as DeviceInfo[]
    peers.value = capable.map((d) => ({ id: d.nodeId, name: d.deviceName }))
    if (peers.value.length > 0) {
      connOnline.value = true
      // 激活对端不在列表中（下线）→ 自动切到首个可用设备；否则保持
      if (!peers.value.some((p) => p.id === activePeerId.value)) {
        void switchPeer(peers.value[0].id)
      }
    } else {
      activePeerId.value = ''
    }
  }

  /** 从插件拉取设备列表（初始/手动刷新） */
  async function refresh(): Promise<void> {
    try {
      const devices = await context.commands.execute('file-transfer.query-peer', {})
      applyDevices(devices)
    } catch (e) {
      console.error('[File Transfer] query-peer failed:', e)
    }
  }

  /** 切换激活设备 */
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

  /** 设备名富化（device-connected 载荷） */
  function handleDeviceConnected(payload: { device_id?: string; device_name?: string }): void {
    connOnline.value = true
    if (payload?.device_id && payload?.device_name) {
      const item = peers.value.find((p) => p.id === payload.device_id)
      if (item && !item.name) item.name = payload.device_name!
    }
  }

  function start(): void {
    stop()
    dispDevices = context.events.on('plugin:file-transfer:devices-changed', applyDevices)
    dispDevice = context.events.on('device-connected', handleDeviceConnected)
    dispDeviceDisc = context.events.on('device-disconnected', () => {
      connOnline.value = false
    })
    void refresh()
  }

  function stop(): void {
    dispDevices?.dispose()
    dispDevices = null
    dispDevice?.dispose()
    dispDevice = null
    dispDeviceDisc?.dispose()
    dispDeviceDisc = null
  }

  return {
    peers,
    activePeerId,
    peer,
    connOnline,
    switchPeer,
    refresh,
    start,
    stop,
  }
}
