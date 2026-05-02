/**
 * Android 专用功能 Composable
 *
 * 处理移动端特定功能：
 * - 屏幕旋转锁定
 * - 后台运行状态
 * - 通知权限请求
 * - 状态栏高度获取
 * - 锁屏优化
 */
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { usePlatform } from './usePlatform'

// 导出 PlatformInfo 类型（从 usePlatform 复制）
export interface PlatformInfo {
  platform: 'windows' | 'macos' | 'linux' | 'android' | 'ios' | null
  arch: 'x86_64' | 'aarch64' | 'arm' | null
  osVersion: string | null
  osType: string | null
  isDesktop: boolean
  isMobile: boolean
  isWindows: boolean
  isMacos: boolean
  isLinux: boolean
}

/**
 * Android 设备专用功能
 */
export function useAndroidFeatures() {
  const { platformInfo } = usePlatform()
  const isAndroid = ref(false)
  const statusBarHeight = ref(0)
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

    // 获取状态栏高度
    try {
      const height = await invoke<number>('get_status_bar_height')
      statusBarHeight.value = height
    } catch {
      // Fallback: 使用 CSS env(safe-area-inset-top)
      console.log('[Android] get_status_bar_height not available, using CSS fallback')
    }

    // 检查通知权限
    try {
      const { isPermissionGranted } = await import('@tauri-apps/plugin-notification')
      hasNotificationPermission.value = await isPermissionGranted()
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
   * 发送通知
   */
  async function sendNotification(title: string, body: string): Promise<void> {
    if (!isAndroid.value || !hasNotificationPermission.value) return

    try {
      const { sendNotification } = await import('@tauri-apps/plugin-notification')
      await sendNotification({ title, body })
    } catch (e) {
      console.error('[Android] Failed to send notification:', e)
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
    statusBarHeight,
    isInBackground,
    hasNotificationPermission,
    requestNotificationPermission,
    sendNotification,
    setScreenOrientation,
    keepScreenAwake,
  }
}

/**
 * 移动端安全区域信息
 */
export function useSafeArea() {
  const { platformInfo } = usePlatform()
  const safeAreaInsets = ref({
    top: 0,
    bottom: 0,
    left: 0,
    right: 0,
  })

  onMounted(() => {
    // 仅在移动端计算安全区域
    if (!platformInfo.value.isMobile) return

    // 从 CSS 变量获取值
    const computeInsets = () => {
      const style = getComputedStyle(document.documentElement)
      safeAreaInsets.value = {
        top: parseInt(style.getPropertyValue('--safe-area-inset-top') || '0'),
        bottom: parseInt(style.getPropertyValue('--safe-area-inset-bottom') || '0'),
        left: parseInt(style.getPropertyValue('--safe-area-inset-left') || '0'),
        right: parseInt(style.getPropertyValue('--safe-area-inset-right') || '0'),
      }
    }

    // 立即计算一次
    computeInsets()

    // 监听窗口大小变化（可能影响安全区域）
    window.addEventListener('resize', computeInsets)
  })

  return {
    safeAreaInsets,
  }
}