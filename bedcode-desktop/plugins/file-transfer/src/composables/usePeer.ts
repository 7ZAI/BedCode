/**
 * 对端在线状态与设备名
 *
 * 驱动顶栏对端 pill：`filesrv:peer_changed`（Tauri 事件，携带 peerId + online）
 * 为在线状态的权威来源；`fileService.getPeerInfo(peerId)` 补充对端 IP。
 *
 * 设备名说明：宿主 `PeerFileServiceInfo` 不包含设备名；`device-connected`
 * 事件（DeviceConnectionEvent.device_name）为 best-effort 富化来源，仅在
 * 插件面板存活期间监听到连接事件时可用，错过则回退 peerId/IP。
 */
import { ref, type Ref } from 'vue'
import type { Disposable, PluginContext } from '@bedcode/plugin-sdk-desktop'

/** 对端状态（用于顶栏 pill） */
export interface PeerState {
  id: string
  name: string
  online: boolean
  ip: string
}

export function usePeer(context: PluginContext) {
  const peer = ref<PeerState>({ id: '', name: '', online: false, ip: '' }) as Ref<PeerState>

  let dispPeer: Disposable | null = null
  let dispDevice: Disposable | null = null

  /** 经 fileService 拉取对端 IP（best-effort，失败不影响在线状态） */
  async function refreshInfo(): Promise<void> {
    if (!peer.value.id) return
    try {
      const info = await context.fileService.getPeerInfo(peer.value.id)
      if (info?.ip) {
        peer.value = { ...peer.value, ip: info.ip }
      }
    } catch (e) {
      console.error('[File Transfer] getPeerInfo failed:', e)
    }
  }

  /** 对端上/下线事件处理 */
  function handlePeerChanged(payload: { peerId?: string; online?: boolean }): void {
    if (!payload?.peerId) return
    if (payload.online) {
      peer.value = { ...peer.value, id: payload.peerId, online: true }
      void refreshInfo()
    } else {
      peer.value = { ...peer.value, online: false }
    }
  }

  /** 设备名富化（peer pill 显示用） */
  function setDeviceName(name: string): void {
    if (name) peer.value = { ...peer.value, name }
  }

  /** device-connected 事件载荷（DeviceConnectionEvent） */
  function handleDeviceConnected(payload: { device_name?: string }): void {
    if (payload?.device_name) setDeviceName(payload.device_name)
  }

  function start(): void {
    stop()
    dispPeer = context.events.on('filesrv:peer_changed', handlePeerChanged)
    dispDevice = context.events.on('device-connected', handleDeviceConnected)
  }

  function stop(): void {
    dispPeer?.dispose()
    dispPeer = null
    dispDevice?.dispose()
    dispDevice = null
  }

  return { peer, setDeviceName, refreshInfo, start, stop }
}
