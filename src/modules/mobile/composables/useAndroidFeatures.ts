/**
 * Android 专用功能 Composable
 *
 * 处理移动端特定功能：
 * - 屏幕旋转锁定
 * - 后台运行状态
 * - 通知权限请求
 * - 锁屏优化
 *
 * 注意：任务状态通知由 useTaskNotification + Kotlin TaskNotificationPlugin 处理，
 * 本模块不再包含通知发送方法
 */
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { usePlatform } from '../../shared/composables/usePlatform'

/**
 * Android 设备专用功能
 */
export function useAndroidFeatures() {
  const { platformInfo } = usePlatform()
  const isAndroid = ref(false)
  const isInBackground = ref(false)
  const hasNotificationPermission = ref(false)

  // 监听生命周期事件
  let unlistenResume: (() => void) | null = null
  let unlistenPause: (() => void) | null = null

  onMounted(async () => {
    // 等待平台检测完成
    const info = platformInfo.value
    isAndroid.value = info.platform === 'android'

    if (!isAndroid.value) return

    // 检查通知权限，未授予时自动请求
    try {
      const { isPermissionGranted, requestPermission } = await import('@tauri-apps/plugin-notification')
      hasNotificationPermission.value = await isPermissionGranted()
      if (!hasNotificationPermission.value) {
        const result = await requestPermission()
        hasNotificationPermission.value = result === 'granted'
        console.log('[Android] Notification permission request result:', result)
      }
    } catch {
      console.log('[Android] Notification plugin not available')
    }

    // 监听应用生命周期事件
    try {
      unlistenResume = await listen('app-resume', () => {
        isInBackground.value = false
        console.log('[Android] App resumed')
      })

      unlistenPause = await listen('app-pause', () => {
        isInBackground.value = true
        console.log('[Android] App paused')
      })
    } catch {
      console.log('[Android] Lifecycle events not available')
    }
  })

  onUnmounted(() => {
    unlistenResume?.()
    unlistenPause?.()
  })

  /**
   * 请求通知权限
   */
  async function requestNotificationPermission(): Promise<boolean> {
    if (!isAndroid.value) return false

    try {
      const { requestPermission } = await import('@tauri-apps/plugin-notification')
      const result = await requestPermission()
      hasNotificationPermission.value = result === 'granted'
      return result === 'granted'
    } catch {
      return false
    }
  }

  /**
   * 设置屏幕方向
   * @param orientation - 'portrait' | 'landscape' | 'unspecified'
   */
  async function setScreenOrientation(orientation: 'portrait' | 'landscape' | 'unspecified'): Promise<void> {
    if (!isAndroid.value) return

    try {
      await invoke('set_screen_orientation', { orientation })
    } catch (e) {
      console.error('[Android] Failed to set screen orientation:', e)
    }
  }

  /**
   * 保持屏幕唤醒（防止锁屏）
   */
  async function keepScreenAwake(enabled: boolean): Promise<void> {
    if (!isAndroid.value) return

    try {
      await invoke('keep_screen_awake', { enabled })
    } catch (e) {
      console.error('[Android] Failed to keep screen awake:', e)
    }
  }

  return {
    isAndroid,
    isInBackground,
    hasNotificationPermission,
    requestNotificationPermission,
    setScreenOrientation,
    keepScreenAwake,
  }
}