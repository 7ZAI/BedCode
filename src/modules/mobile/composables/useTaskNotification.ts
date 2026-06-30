/**
 * Task Notification - 任务状态通知管理
 *
 * 监听任务状态变更事件，根据执行模式（手动/自动）过滤通知规则，
 * 调用 Kotlin TaskNotificationPlugin 发送/取消 Android 系统通知。
 */
import { invoke } from '@tauri-apps/api/core'
import { usePlatform } from '@/modules/shared/composables/usePlatform'

/**
 * 判断任务状态是否需要提醒（震动/声音）
 *
 * 手动模式：asking（受 notifyOnWaiting 控制）、completed、interrupted
 * 自动模式：仅 completed、interrupted
 */
function shouldAlertForStatus(
  status: string,
  mode: 'manual' | 'auto',
  notifyOnWaiting: boolean
): boolean {
  if (mode === 'manual') {
    if (status === 'asking') return notifyOnWaiting
    return ['completed', 'interrupted'].includes(status)
  }
  // 自动模式：仅 completed、interrupted
  return ['completed', 'interrupted'].includes(status)
}

/** 读取移动端设置 */
function getMobileSettings() {
  const saved = localStorage.getItem('mobile-settings')
  return saved
    ? JSON.parse(saved)
    : { vibrate: true, notifyOnWaiting: true, soundOnTaskComplete: true }
}

// 模块级状态，缓存会话执行模式
const sessionModes = new Map<string, 'manual' | 'auto'>()

export function useTaskNotification() {
  const { platformInfo } = usePlatform()

  function isAndroid(): boolean {
    return platformInfo.value?.platform === 'android'
  }

  /**
   * 更新会话的执行模式缓存
   */
  function setSessionMode(sessionId: string, mode: 'manual' | 'auto') {
    sessionModes.set(sessionId, mode)
  }

  /**
   * 获取指定会话的执行模式
   */
  function getModeForSession(sessionId: string): 'manual' | 'auto' {
    return sessionModes.get(sessionId) || 'manual'
  }

  /**
   * 显示/更新指定会话的通知
   */
  async function showTaskNotification(params: {
    sessionId: string
    sessionName: string
    taskStatus: string
    taskReason?: string
  }): Promise<void> {
    if (!isAndroid()) return

    const mode = getModeForSession(params.sessionId)
    const settings = getMobileSettings()

    // 自动模式下 asking 不发通知（自动回复）
    if (mode === 'auto' && params.taskStatus === 'asking') return

    const shouldAlert = shouldAlertForStatus(
      params.taskStatus,
      mode,
      settings.notifyOnWaiting ?? true
    )
    const vibrate = shouldAlert && (settings.vibrate ?? true)
    const sound = shouldAlert && (settings.soundOnTaskComplete ?? true)

    try {
      await invoke('showTaskNotification', {
        sessionId: params.sessionId,
        sessionName: params.sessionName,
        taskStatus: params.taskStatus,
        taskReason: params.taskReason ?? null,
        vibrate,
        sound,
      })
    } catch (e) {
      // Android 原生插件命令可能未注册（Tauri 2.0 自定义插件需额外注册步骤）
      console.warn('[TaskNotification] showTaskNotification failed (plugin may not be registered):', e)
    }
  }

  /**
   * 取消指定会话的通知
   */
  async function cancelTaskNotification(sessionId: string): Promise<void> {
    if (!isAndroid()) return

    try {
      await invoke('cancelTaskNotification', { sessionId })
    } catch (e) {
      console.warn('[TaskNotification] cancelTaskNotification failed (plugin may not be registered):', e)
    }
  }

  /**
   * 取消所有任务通知
   */
  async function cancelAllTaskNotifications(): Promise<void> {
    if (!isAndroid()) return

    try {
      await invoke('cancelAllTaskNotifications')
    } catch (e) {
      console.warn('[TaskNotification] cancelAllTaskNotifications failed (plugin may not be registered):', e)
    }
  }

  return {
    setSessionMode,
    getModeForSession,
    showTaskNotification,
    cancelTaskNotification,
    cancelAllTaskNotifications,
  }
}
