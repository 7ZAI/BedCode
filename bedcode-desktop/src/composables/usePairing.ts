import { ref } from 'vue'
import {
  generatePairingCode,
  clearPairingCode,
  getCurrentPairingCode,
  type PairingCodeInfo
} from '@/composables/useDesktopCommands'

export function usePairing() {
  const pairingCode = ref<PairingCodeInfo | null>(null)

  /**
   * 生成新的配对码
   */
  async function generateCode() {
    const result = await generatePairingCode()
    pairingCode.value = result
  }

  /**
   * 清除当前配对码
   */
  async function clearCode() {
    await clearPairingCode()
    pairingCode.value = null
  }

  /**
   * 检查并恢复当前有效的配对码
   * 返回 true 表示有有效的配对码已恢复
   */
  async function checkCurrentCode(): Promise<boolean> {
    const result = await getCurrentPairingCode()
    if (result && result.code) {
      pairingCode.value = result
      return true
    }
    return false
  }

  return {
    pairingCode,
    generateCode,
    clearCode,
    checkCurrentCode,
  }
}
