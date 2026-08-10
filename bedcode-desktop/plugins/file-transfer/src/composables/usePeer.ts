/**
 * 对端（设备）列表与激活设备状态
 *
 * 权威数据源：插件 WASM 推送的 `plugin:file-transfer:peers-changed`
 * （携带在线对端 id 列表 + 激活对端 id；对端上/下线、切换激活均推送）。
 * `filesrv:peer_changed`（宿主 Tauri 事件）触发时同步重拉列表，弥合
 * 宿主事件与插件总线投递的时序差；`device-connected`/`device-disconnected`
 * 富化设备名（best-effort：错过的事件回退显示 IP/ID）。
 *
 * 语义分层：`connOnline`（WS 控制面连接）≠ `peer.online`（对端已公告共享）。
 */
import { computed, ref, type Ref } from 'vue'
import type { Disposable, PluginContext } from '@bedcode/plugin-sdk-desktop'

/** 在线对端（设备）条目 */
export interface PeerItem {
  id: string
  name: string
  ip: string
}

/** 激活对端派生状态（对端 pill / 目录加载依赖） */
export interface PeerState {
  id: string
  name: string
  online: boolean
  ip: string
}

export function usePeer(context: PluginContext) {
  /** 在线设备列表（插件 peers-changed 事件权威更新） */
  const peers = ref<PeerItem[]>([]) as Ref<PeerItem[]>
  /** 激活设备 id（'' = 无可用对端） */
  const activePeerId = ref('') as Ref<string>

  /** 设备名缓存（device-connected 富化，id → name） */
  const nameById = new Map<string, string>()
  /** 设备 IP 缓存（getPeerInfo 富化，id → ip） */
  const ipById = new Map<string, string>()

  /** 激活对端派生（无激活对端时 online=false） */
  const peer = computed<PeerState>(() => {
    const item = peers.value.find((p) => p.id === activePeerId.value)
    return item
      ? { id: item.id, name: item.name, online: true, ip: item.ip }
      : { id: '', name: '', online: false, ip: '' }
  })

  /** WS 控制面连接状态（device-connected / device-disconnected 事件驱动） */
  const connOnline = ref(false) as Ref<boolean>

  let dispPeers: Disposable | null = null
  let dispPeerChanged: Disposable | null = null
  let dispDevice: Disposable | null = null
  let dispDeviceDisc: Disposable | null = null

  /** 从插件拉取列表（初始 / peer_changed 触发 / 手动刷新） */
  async function refresh(): Promise<void> {
    try {
      const data = await context.commands.execute('file-transfer.list-peers', {})
      const list = (data?.peers ?? []) as { peerId?: string }[]
      const next: PeerItem[] = list
        .map((p) => p.peerId ?? '')
        .filter(Boolean)
        .map((id) => ({
          id,
          name: nameById.get(id) ?? '',
          ip: ipById.get(id) ?? '',
        }))
      peers.value = next
      // 激活对端以插件为准（插件保证激活必然在线；列表里查不到则清空）
      const active: string = data?.activePeerId ?? ''
      activePeerId.value = next.some((p) => p.id === active) ? active : ''
    } catch (e) {
      console.error('[File Transfer] list-peers failed:', e)
    }
  }

  /** 切换激活设备（调插件命令；响应 activePeerId 为权威，事件作兜底回刷） */
  async function switchPeer(id: string): Promise<boolean> {
    if (id === activePeerId.value) return true
    try {
      const data = await context.commands.execute('file-transfer.set-active-peer', { peerId: id })
      if (data?.activePeerId) {
        activePeerId.value = data.activePeerId
      } else {
        void refresh()
      }
      return true
    } catch (e) {
      console.error('[File Transfer] set-active-peer failed:', e)
      return false
    }
  }

  /** 对端上/下线事件（宿主通道，先于插件总线投递；重拉列表兜底） */
  function handlePeerChanged(payload: { peerId?: string; online?: boolean; deviceName?: string; ip?: string }): void {
    if (payload?.online && payload.peerId) {
      connOnline.value = true
      // 宿主公告携带真实设备名/IP（filesrv:peer_changed 载荷），直接富化缓存
      if (payload.deviceName) {
        nameById.set(payload.peerId, payload.deviceName)
        const item = peers.value.find((p) => p.id === payload.peerId)
        if (item) item.name = payload.deviceName
      }
      if (payload.ip) {
        ipById.set(payload.peerId, payload.ip)
      }
    }
    void refresh()
  }

  /** 设备名富化（device-connected 载荷 DeviceConnectionEvent） */
  function handleDeviceConnected(payload: { device_id?: string; device_name?: string }): void {
    connOnline.value = true
    const id = payload?.device_id
    if (id && payload?.device_name) {
      nameById.set(id, payload.device_name)
      const item = peers.value.find((p) => p.id === id)
      if (item) item.name = payload.device_name
    }
  }

  /** 设备断开事件（DeviceConnectionEvent） */
  function handleDeviceDisconnected(): void {
    connOnline.value = false
  }

  /** 经 fileService 拉取对端 IP（best-effort，失败不影响在线状态） */
  async function refreshInfo(): Promise<void> {
    for (const p of peers.value) {
      if (p.ip) continue
      try {
        const info = await context.fileService.getPeerInfo(p.id)
        if (info?.ip) {
          ipById.set(p.id, info.ip)
          p.ip = info.ip
        }
        // 名称缺失时用公告携带的真实设备名兜底（device-connected 事件遗漏场景）
        if (info?.device_name && !nameById.has(p.id)) {
          nameById.set(p.id, info.device_name)
          p.name = info.device_name
        }
      } catch {
        // 对端信息查询失败不影响列表展示
      }
    }
  }

  function start(): void {
    stop()
    dispPeers = context.events.on('plugin:file-transfer:peers-changed', () => void refresh())
    dispPeerChanged = context.events.on('filesrv:peer_changed', handlePeerChanged)
    dispDevice = context.events.on('device-connected', handleDeviceConnected)
    dispDeviceDisc = context.events.on('device-disconnected', handleDeviceDisconnected)
    void refresh()
    void refreshInfo()
  }

  function stop(): void {
    dispPeers?.dispose()
    dispPeers = null
    dispPeerChanged?.dispose()
    dispPeerChanged = null
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
    refreshInfo,
    start,
    stop,
  }
}
