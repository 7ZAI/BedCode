import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import {
  generatePairingCode,
  verifyPairingCode,
  listPairedDevices,
  removePairedDevice,
  type PairingCodeInfo,
} from '@/modules/desktop/composables/useDesktopCommands'

export interface PairedDevice {
  id: string
  name: string
  deviceName?: string
  address: string
  port: number
  pairedAt?: string
}

export const useDeviceStore = defineStore('device', () => {
  const pairedDevices = ref<PairedDevice[]>([])
  const pairingCode = ref<PairingCodeInfo | null>(null)
  const pairingExpiry = ref<number>(0)

  async function loadPairedDevices() {
    pairedDevices.value = await listPairedDevices()
  }

  async function startPairing() {
    const result = await generatePairingCode()
    pairingCode.value = result
    // Use the expires_in from the result
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
      const result = await verifyPairingCode(code)
      return result
    } catch (error) {
      console.error('Pairing verification failed:', error)
      return false
    }
  }

  async function removeDevice(deviceId: string) {
    await removePairedDevice(deviceId)
    pairedDevices.value = await listPairedDevices()
  }

  function clearPairingCode() {
    pairingCode.value = null
    pairingExpiry.value = 0
  }

  return {
    pairedDevices,
    pairingCode,
    pairingExpiry,
    loadPairedDevices,
    startPairing,
    verifyPairing,
    removeDevice,
    clearPairingCode,
  }
})