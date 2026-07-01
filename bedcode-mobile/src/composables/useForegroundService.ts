/**
 * Android 前台服务管理
 *
 * 用于保持 WebSocket 连接在后台不被系统杀死
 */
import { invoke } from '@tauri-apps/api/core'
import i18n from '@/locales'
import { useMobileConnection } from './useMobileConnection'
import { usePlatform } from '@/composables/usePlatform'

export function useForegroundService() {
  const { platformInfo } = usePlatform()
  const {
    isConnected,
    connectionStatus,
    activeSessions,
    currentDevice,
    isConnecting,
  } = useMobileConnection()

  /**
   * 检查是否为 Android 平台
   */
  function isAndroid(): boolean {
    return platformInfo.value?.platform === 'android'
  }

  /**
   * 启动前台服务
   */
  async function startService(): Promise<void> {
    if (!isAndroid()) {
      return
    }

    const content = buildNotificationContent()

    try {
      await invoke('startForegroundService', {
        title: 'BedCode',
        content,
      })
    } catch (e) {
      // Android 原生插件命令可能未注册（Tauri 2.0 自定义插件需额外注册步骤）
      console.warn('[ForegroundService] startService failed (plugin may not be registered):', e)
    }
  }

  /**
   * 停止前台服务
   */
  async function stopService(): Promise<void> {
    if (!isAndroid()) {
      return
    }

    try {
      await invoke('stopForegroundService')
    } catch (e) {
      console.warn('[ForegroundService] stopService failed (plugin may not be registered):', e)
    }
  }

  /**
   * 更新通知内容
   */
  async function updateNotification(): Promise<void> {
    if (!isAndroid()) {
      return
    }

    const content = buildNotificationContent()

    try {
      await invoke('updateForegroundNotification', {
        title: 'BedCode',
        content,
      })
    } catch (e) {
      console.warn('[ForegroundService] updateNotification failed (plugin may not be registered):', e)
    }
  }

  /**
   * 构建通知内容
   *
   * 格式规则：
   * - 重连中（连接中但有错误）: "正在重连..."
   * - 已连接 + 有会话: "3 个会话运行中 · 已连接 Desktop-X"
   * - 已连接 + 无会话: "已连接 Desktop-X"
   * - 未连接: "后台运行中"
   */
  function buildNotificationContent(): string {
    const runningSessions = activeSessions.value.filter(
      (s: any) => s.status === 'running'
    )
    const sessionCount = runningSessions.length
    const deviceName = currentDevice.value?.name || ''

    // 重连中（正在连接但之前有错误或设备信息）
    if (isConnecting.value && currentDevice.value) {
      return i18n.global.t('mobile.connection.foregroundReconnecting')
    }

    // 已连接且有运行中的会话
    if (isConnected.value && sessionCount > 0) {
      return i18n.global.t('mobile.connection.foregroundSessions', { count: sessionCount, name: deviceName })
    }

    // 已连接无会话
    if (isConnected.value) {
      return i18n.global.t('mobile.connection.foregroundConnected', { name: deviceName })
    }

    // 未连接
    return i18n.global.t('mobile.connection.foregroundIdle')
  }

  return {
    startService,
    stopService,
    updateNotification,
    buildNotificationContent,
  }
}
