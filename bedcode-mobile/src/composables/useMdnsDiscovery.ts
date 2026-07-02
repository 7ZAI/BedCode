/**
 * mDNS Discovery Composable
 *
 * 局域网内 BedCode 设备的自动发现
 */

import { ref, readonly } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

/** mDNS 发现到的服务信息 */
export interface DiscoveredService {
  instance_name: string
  host_name: string
  address: string
  port: number
  txt_records: Record<string, string>
  platform: string
  device_name: string
}

// ==================== Global State ====================

const discoveredServices = ref<DiscoveredService[]>([])
const isScanning = ref(false)

let unlistenFound: UnlistenFn | null = null
let unlistenResolved: UnlistenFn | null = null
let unlistenRemoved: UnlistenFn | null = null
let listenersInitialized = false

// ==================== Event Listeners ====================

async function initListeners() {
  if (listenersInitialized) return
  listenersInitialized = true

  unlistenFound = await listen<{ instance_name: string }>('mdns_service_found', (event) => {
    console.debug('[MdnsDiscovery] Service found:', event.payload.instance_name)
  })

  unlistenResolved = await listen<DiscoveredService>('mdns_service_resolved', (event) => {
    const service = event.payload
    console.log('[MdnsDiscovery] Service resolved:', service.device_name, service.address, service.port)
    // 更新或添加到列表（同一实例名只保留最新）
    const index = discoveredServices.value.findIndex(s => s.instance_name === service.instance_name)
    if (index !== -1) {
      discoveredServices.value[index] = service
    } else {
      discoveredServices.value.push(service)
    }
  })

  unlistenRemoved = await listen<{ instance_name: string }>('mdns_service_removed', (event) => {
    console.debug('[MdnsDiscovery] Service removed:', event.payload.instance_name)
    discoveredServices.value = discoveredServices.value.filter(
      s => s.instance_name !== event.payload.instance_name
    )
  })
}

function cleanupListeners() {
  unlistenFound?.()
  unlistenResolved?.()
  unlistenRemoved?.()
  unlistenFound = null
  unlistenResolved = null
  unlistenRemoved = null
  listenersInitialized = false
}

// ==================== Composable ====================

/**
 * mDNS 设备发现 composable
 *
 * 全局单例模式，扫描状态跨页面共享
 */
export function useMdnsDiscovery() {
  async function startDiscovery() {
    if (isScanning.value) return
    await initListeners()
    discoveredServices.value = []
    isScanning.value = true
    try {
      await invoke('mdns_start_discovery')
    } catch (e) {
      console.error('[MdnsDiscovery] Failed to start:', e)
      isScanning.value = false
      throw e
    }
  }

  async function stopDiscovery() {
    if (!isScanning.value) return
    try {
      await invoke('mdns_stop_discovery')
    } catch (e) {
      console.error('[MdnsDiscovery] Failed to stop:', e)
    } finally {
      isScanning.value = false
      cleanupListeners()
    }
  }

  async function refreshServices() {
    try {
      const services: DiscoveredService[] = await invoke('mdns_get_discovered_services')
      discoveredServices.value = services
    } catch (e) {
      console.error('[MdnsDiscovery] Failed to refresh:', e)
    }
  }

  return {
    discoveredServices: readonly(discoveredServices),
    isScanning: readonly(isScanning),
    startDiscovery,
    stopDiscovery,
    refreshServices,
  }
}
