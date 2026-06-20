import { ref, onUnmounted } from 'vue'
import { getConnectedDevices, onDeviceConnected, onDeviceDisconnected } from '@/modules/desktop/composables/useDesktopCommands'

export interface ConnectedDeviceInfo {
  id: string
  name: string
  address: string
  port: number
  /** 设备指纹，用于与数据库 pairings 记录关联匹配 */
  fingerprint?: string
  connected_at?: string
}



export function useConnectedDevices() {
  const connectedDevices = ref<ConnectedDeviceInfo[]>([])
  let unlistenConnected: (() => void) | null = null
  let unlistenDisconnected: (() => void) | null = null

  async function loadConnectedDevices() {
    const devices = await getConnectedDevices()
    connectedDevices.value = devices.map(d => ({
      id: d.device_id || '',
      name: d.device_id || 'Unknown',
      address: d.addr || '',
      port: 0,
      fingerprint: d.fingerprint,
    }))
  }

  async function init() {
    // 加载初始设备列表
    await loadConnectedDevices()

    // 监听设备连接事件
    unlistenConnected = await onDeviceConnected(async () => {
      await loadConnectedDevices()
    })

    // 监听设备断开事件
    unlistenDisconnected = await onDeviceDisconnected(async () => {
      await loadConnectedDevices()
    })
  }

  onUnmounted(() => {
    if (unlistenConnected) unlistenConnected()
    if (unlistenDisconnected) unlistenDisconnected()
  })

  // Initialize on creation
  init()

  return {
    connectedDevices,
    loadConnectedDevices,
  }
}