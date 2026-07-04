/**
 * Notification - 移动端系统通知管理
 *
 * 基于 @tauri-apps/plugin-notification 实现任务状态通知和连接状态通知，
 * 替代原 Kotlin TaskNotificationPlugin。
 *
 * 前台服务通知仍由 Kotlin ForegroundServicePlugin 处理。
 */
import { sendNotification, isPermissionGranted, requestPermission } from '@tauri-apps/plugin-notification'
import i18n from '@/locales'
import { usePlatform } from '@/composables/usePlatform'

/** 判断任务状态是否需要提醒（震动/声音） */
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

/** 构建任务状态通知内容 */
function buildTaskBody(status: string, reason?: string): string {
  const t = i18n.global.t
  switch (status) {
    case 'idle':
      return t('mobile.notification.taskIdle')
    case 'in_progress':
      return t('mobile.notification.taskInProgress')
    case 'asking':
      return t('mobile.notification.taskAsking')
    case 'completed':
      return t('mobile.notification.taskCompleted')
    case 'interrupted':
      return reason
        ? t('mobile.notification.taskInterruptedReason', { reason })
        : t('mobile.notification.taskInterrupted')
    default:
      return status
  }
}

/**
 * 根据 sessionId 生成通知 ID（32-bit integer）
 *
 * 与 Kotlin 端 TaskNotificationManager 使用相同的算法，
 * 基数 2000 避免与前台服务通知 ID（1001）冲突
 */
function getNotificationId(sessionId: string): number {
  return 2000 + Math.abs(hashCode(sessionId)) % 1000
}

/** 简易 Java-style hashCode */
function hashCode(str: string): number {
  let hash = 0
  for (let i = 0; i < str.length; i++) {
    hash = ((hash << 5) - hash + str.charCodeAt(i)) | 0
  }
  return hash
}

// 连接通知固定 ID
const CONNECTION_NOTIFICATION_ID = 3001

// 模块级状态：缓存会话执行模式
const sessionModes = new Map<string, 'manual' | 'auto'>()

// 模块级状态：缓存权限状态，避免重复请求
let permissionGranted: boolean | null = null

export function useNotification() {
  const { platformInfo } = usePlatform()

  function isAndroid(): boolean {
    return platformInfo.value?.platform === 'android'
  }

  /**
   * 确保通知权限已授予
   *
   * 首次调用时检查权限，未授予则请求。结果缓存到模块级变量。
   */
  async function ensurePermission(): Promise<boolean> {
    if (!isAndroid()) return false

    // 使用缓存
    if (permissionGranted === true) return true

    try {
      let granted = await isPermissionGranted()
      if (!granted) {
        const result = await requestPermission()
        granted = result === 'granted'
      }
      permissionGranted = granted
      return granted
    } catch (e) {
      console.warn('[Notification] Permission check failed:', e)
      return false
    }
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
   * 显示/更新指定会话的任务通知
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

    // idle / in_progress 不发通知
    if (['idle', 'in_progress'].includes(params.taskStatus)) return

    const hasPermission = await ensurePermission()
    if (!hasPermission) return

    const shouldAlert = shouldAlertForStatus(
      params.taskStatus,
      mode,
      settings.notifyOnWaiting ?? true
    )

    const body = buildTaskBody(params.taskStatus, params.taskReason)

    try {
      sendNotification({
        id: getNotificationId(params.sessionId),
        title: params.sessionName,
        body,
        // 非提醒状态使用 silent 避免频繁打扰
        silent: !shouldAlert,
        extra: {
          type: 'task',
          sessionId: params.sessionId,
        },
      })
    } catch (e) {
      console.warn('[Notification] showTaskNotification failed:', e)
    }
  }

  /**
   * 取消指定会话的任务通知
   *
   * tauri-plugin-notification 不支持按 ID 取消单条通知，
   * 保留空实现以维持接口兼容
   */
  async function cancelTaskNotification(_sessionId: string): Promise<void> {
    // 官方插件无 cancel-by-id API
  }

  /**
   * 取消所有任务通知
   *
   * 同上，官方插件不支持批量取消
   */
  async function cancelAllTaskNotifications(): Promise<void> {
    // 保留空实现以维持接口兼容
  }

  /**
   * 显示连接状态通知
   */
  async function showConnectionNotification(params: {
    type: 'disconnected' | 'reconnect_failed' | 'auth_failed'
    deviceName?: string
    reason?: string
  }): Promise<void> {
    if (!isAndroid()) return

    const hasPermission = await ensurePermission()
    if (!hasPermission) return

    const t = i18n.global.t
    let body: string

    switch (params.type) {
      case 'disconnected':
        body = t('mobile.notification.connectionDisconnected', { name: params.deviceName || '' })
        break
      case 'reconnect_failed':
        body = t('mobile.notification.reconnectFailed', { reason: params.reason || '' })
        break
      case 'auth_failed':
        body = t('mobile.notification.authFailed')
        break
    }

    try {
      sendNotification({
        id: CONNECTION_NOTIFICATION_ID,
        title: 'BedCode',
        body,
        extra: {
          type: 'connection',
        },
      })
    } catch (e) {
      console.warn('[Notification] showConnectionNotification failed:', e)
    }
  }

  /**
   * 取消连接状态通知
   */
  async function cancelConnectionNotification(): Promise<void> {
    // 同 cancelTaskNotification，官方插件不支持按 ID 取消
  }

  return {
    ensurePermission,
    setSessionMode,
    getModeForSession,
    showTaskNotification,
    cancelTaskNotification,
    cancelAllTaskNotifications,
    showConnectionNotification,
    cancelConnectionNotification,
  }
}
