import { ref, computed } from 'vue'
import {
  generateQrCode,
  clearQrCode,
  getQrConnectionInfo,
  getQrTokenTtl,
} from '@/composables/useDesktopCommands'

// Re-export from model
import type { QrConnectionInfo } from './model'
export type { QrConnectionInfo }

export function useQrCode() {
  const qrData = ref<QrConnectionInfo | null>(null)
  const remainingSeconds = ref(0)
  const isLoading = ref(false)
  let countdownInterval: ReturnType<typeof setInterval> | null = null

  const isExpired = computed(() => remainingSeconds.value <= 0)
  const hasQr = computed(() => qrData.value !== null && !isExpired.value)

  function startCountdown(ttlSeconds: number) {
    stopCountdown()
    remainingSeconds.value = ttlSeconds
    countdownInterval = setInterval(() => {
      remainingSeconds.value--
      if (remainingSeconds.value <= 0) {
        stopCountdown()
        qrData.value = null
      }
    }, 1000)
  }

  function stopCountdown() {
    if (countdownInterval) {
      clearInterval(countdownInterval)
      countdownInterval = null
    }
  }

  /**
   * 生成新的二维码
   */
  async function generateQr(host?: string) {
    isLoading.value = true
    try {
      await generateQrCode()
      const info = await getQrConnectionInfo(host)
      if (info) {
        qrData.value = info
        // 使用后端返回的剩余时间
        startCountdown(info.remaining_secs)
      } else {
        qrData.value = null
      }
    } catch (e) {
      console.error('Failed to generate QR:', e)
      qrData.value = null
    } finally {
      isLoading.value = false
    }
  }

  /**
   * 恢复现有二维码（不重新生成）
   * 返回 true 表示成功恢复
   */
  async function restoreQr(host?: string): Promise<boolean> {
    try {
      const info = await getQrConnectionInfo(host)
      if (info && info.token) {
        qrData.value = info
        // 使用后端返回的剩余时间
        startCountdown(info.remaining_secs)
        return true
      }
      return false
    } catch (e) {
      console.error('Failed to restore QR:', e)
      return false
    }
  }

  async function clearQr() {
    await clearQrCode()
    qrData.value = null
    stopCountdown()
  }

  return {
    qrData,
    remainingSeconds,
    isLoading,
    isExpired,
    hasQr,
    generateQr,
    restoreQr,
    clearQr,
    getQrTokenTtl,
  }
}