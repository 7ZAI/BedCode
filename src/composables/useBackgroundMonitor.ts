import { ref, onMounted, onUnmounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { usePlatform } from './usePlatform'

/**
 * 前后台状态监听
 *
 * 监听应用进入前台/后台的状态变化
 * 用于后台运行时触发通知等场景
 */
export function useBackgroundMonitor() {
  const { platformInfo } = usePlatform()
  const isInBackground = ref(false)
  const wasInBackground = ref(false)  // 用于检测刚从后台恢复

  let unlistenResume: (() => void) | null = null
  let unlistenPause: (() => void) | null = null

  onMounted(async () => {
    // 等待平台检测完成
    const info = platformInfo.value
    if (!info.isMobile) return

    // 监听应用生命周期事件
    try {
      unlistenResume = await listen('app-resume', () => {
        wasInBackground.value = isInBackground.value
        isInBackground.value = false
        console.log('[BackgroundMonitor] App resumed, wasInBackground:', wasInBackground.value)
      })

      unlistenPause = await listen('app-pause', () => {
        isInBackground.value = true
        console.log('[BackgroundMonitor] App paused')
      })
    } catch (e) {
      console.error('[BackgroundMonitor] Failed to listen lifecycle events:', e)
    }
  })

  onUnmounted(() => {
    unlistenResume?.()
    unlistenPause?.()
  })

  return {
    isInBackground,
    wasInBackground,
  }
}