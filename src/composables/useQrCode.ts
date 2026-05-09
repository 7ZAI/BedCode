import { ref, computed } from 'vue'
import { useQrCodeApi } from './useTauri'
import type { QrConnectionInfo } from './useTauri'

export function useQrCode() {
  const api = useQrCodeApi()

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
      }
    }, 1000)
  }

  function stopCountdown() {
    if (countdownInterval) {
      clearInterval(countdownInterval)
      countdownInterval = null
    }
  }

  async function generateQr() {
    isLoading.value = true
    try {
      const token = await api.generateQrCode()
      const info = await api.getQrConnectionInfo()
      if (info) {
        // 先获取 TTL，再同步设置 qrData 和倒计时
        // 避免 await 导致的中间状态：qrData 已更新但倒计时未启动，
        // 此时 hasQr 仍为 false，watch 触发时 canvas 未挂载，导致首次空白
        const ttl = await api.getQrTokenTtl()
        qrData.value = info
        startCountdown(ttl)
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

  async function clearQr() {
    await api.clearQrCode()
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
    clearQr,
  }
}
