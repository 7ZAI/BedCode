/**
 * Android 专用功能 Composable
 *
 * 处理移动端特定功能：
 * - 屏幕旋转锁定
 * - 后台运行状态
 * - 锁屏优化
 *
 * 通知权限管理已迁移到 useNotification，
 * 任务状态通知由 useNotification + @tauri-apps/plugin-notification 处理
 */
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { usePlatform } from './usePlatform'
import { useNotification } from './useNotification'

/**
 * Android 设备专用功能
 */
export function useAndroidFeatures() {
  const { platformInfo } = usePlatform()
  const isAndroid = ref(false)
  const isInBackground = ref(false)

  // 监听生命周期事件
  let unlistenResume: (() => void) | null = null
  let unlistenPause: (() => void) | null = null

  onMounted(async () => {
    // 等待平台检测完成
    const info = platformInfo.value
    isAndroid.value = info.platform === 'android'

    if (!isAndroid.value) return

    // 初始化通知权限（useNotification.ensurePermission 会在发送通知时自动调用）
    try {
      const { ensurePermission } = useNotification()
      await ensurePermission()
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
    setScreenOrientation,
    keepScreenAwake,
  }
}