import { ref } from 'vue'
import { generatePairingCode, clearPairingCode, getCurrentPairingCode } from '@/modules/desktop/composables/useDesktopCommands'

export function usePairing() {
  const pairingCode = ref<{ code: string; expiresIn: number } | null>(null)

  async function generateCode() {
    const code = await generatePairingCode()
    pairingCode.value = { code, expiresIn: 300 }
  }

  async function clearCode() {
    await clearPairingCode()
    pairingCode.value = null
  }

  async function checkCurrentCode() {
    const code = await getCurrentPairingCode()
    if (code) {
      pairingCode.value = { code, expiresIn: 300 }
    }
  }

  return {
    pairingCode,
    generateCode,
    clearCode,
    checkCurrentCode,
  }
}