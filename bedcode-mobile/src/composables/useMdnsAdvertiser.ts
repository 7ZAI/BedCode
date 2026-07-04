/**
 * mDNS Advertiser Composable
 *
 * 广播本设备的 mDNS 服务，允许桌面端发现移动端
 */

import { ref, readonly } from 'vue'
import { invoke } from '@tauri-apps/api/core'

// ==================== Global State ====================

const isAdvertising = ref(false)

// ==================== Composable ====================

/**
 * mDNS 广播 composable
 *
 * 全局单例模式
 */
export function useMdnsAdvertiser() {
  async function startAdvertise(port: number, deviceName: string) {
    if (isAdvertising.value) return
    try {
      await invoke('mdns_start_advertise', { port, deviceName })
      isAdvertising.value = true
    } catch (e) {
      console.error('[MdnsAdvertiser] Failed to start:', e)
      throw e
    }
  }

  async function stopAdvertise() {
    if (!isAdvertising.value) return
    try {
      await invoke('mdns_stop_advertise')
    } catch (e) {
      console.error('[MdnsAdvertiser] Failed to stop:', e)
    } finally {
      isAdvertising.value = false
    }
  }

  return {
    isAdvertising: readonly(isAdvertising),
    startAdvertise,
    stopAdvertise,
  }
}
