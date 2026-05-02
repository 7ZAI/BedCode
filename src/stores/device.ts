import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { usePairing, type Pairing } from '@/composables/useTauri'

export type { Pairing }

export interface DiscoveredDevice {
  name: string
  address: string
  port: number
}

export const useDeviceStore = defineStore('device', () => {
  const pairedDevices = ref<Pairing[]>([])
  const discoveredDevices = ref<DiscoveredDevice[]>([])
  const pairingCode = ref<string | null>(null)
  const pairingExpiry = ref<number>(0)
  const isScanning = ref(false)

  const pairingApi = usePairing()

  async function loadPairedDevices() {
    await pairingApi.loadDevices()
    pairedDevices.value = pairingApi.devices.value
  }

  async function startDiscovery() {
    isScanning.value = true
    discoveredDevices.value = []

    try {
      // Start discovery service
      await invoke('start_discovery')

      // Get discovered devices
      const devices = await invoke<DiscoveredDevice[]>('get_discovered_devices')
      discoveredDevices.value = devices
    } catch (error) {
      console.error('Discovery failed:', error)
    } finally {
      isScanning.value = false
    }
  }

  async function startPairing() {
    const result = await invoke<{ code: string; expires_in: number }>('generate_pairing_code')
    pairingCode.value = result.code
    pairingExpiry.value = result.expires_in

    // Start countdown
    const interval = setInterval(() => {
      if (pairingExpiry.value > 0) {
        pairingExpiry.value--
      } else {
        clearInterval(interval)
        pairingCode.value = null
      }
    }, 1000)
  }

  async function verifyPairing(code: string, _deviceAddress?: string, _devicePort?: number): Promise<boolean> {
    try {
      // In real implementation, this would connect to the desktop app
      // and verify the pairing code
      // For now, we'll use the local Tauri command
      await invoke('verify_pairing_code', { code })
      return true
    } catch (error) {
      console.error('Pairing verification failed:', error)
      return false
    }
  }

  async function removeDevice(deviceId: string) {
    await pairingApi.removeDevice(deviceId)
    pairedDevices.value = pairingApi.devices.value
  }

  function clearPairingCode() {
    pairingCode.value = null
    pairingExpiry.value = 0
  }

  return {
    pairedDevices,
    discoveredDevices,
    pairingCode,
    pairingExpiry,
    isScanning,
    loadPairedDevices,
    startDiscovery,
    startPairing,
    verifyPairing,
    removeDevice,
    clearPairingCode,
  }
})
