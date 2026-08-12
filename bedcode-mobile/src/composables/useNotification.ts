/**
 * Notification - 移动端系统通知管理
 *
 * 基于自定义 Android 插件（TaskNotificationPlugin）实现任务状态通知和连接状态通知，
 * 支持震动/声音分开控制，与设置页开关一一对应：
 * - vibrate：所有提醒通知是否震动
 * - soundOnTaskComplete：任务完成（completed）是否播放提示音
 * - notifyOnWaiting：手动模式下等待输入（asking）是否提醒
 *
 * 通知权限（Android 13+）由 TaskNotificationPlugin 原生实现检查与请求；
 * 前台服务通知仍由 Kotlin ForegroundServicePlugin 处理。
 */
import { invoke } from '@tauri-apps/api/core'
import i18n from '@/locales'
import { usePlatform } from '@/composables/usePlatform'

/** 提醒标志：震动与声音分开控制 */
interface AlertFlags {
  vibrate: boolean
  sound: boolean
}

/**
 * 计算任务状态通知的提醒标志
 *
 * 提醒状态判定：
 * - manual 模式：asking 受 notifyOnWaiting 控制；completed 受 soundOnTaskComplete 控制；interrupted 始终提醒
 * - auto 模式：completed 受 soundOnTaskComplete 控制；interrupted 始终提醒
 *
 * 提醒时震动统一受 vibrate 开关控制；声音仅 completed 受 soundOnTaskComplete 控制，
 * asking/interrupted 为重要中断提示，声音跟随系统通知渠道设置。
 * 震动与提示音开关都关闭时，所有提醒静默。
 */
function getAlertFlags(
  status: string,
  mode: 'manual' | 'auto',
  notifyOnWaiting: boolean,
  vibrate: boolean,
  soundOnTaskComplete: boolean
): AlertFlags {
  // 震动和提示音都关闭时，始终静默
  if (!vibrate && !soundOnTaskComplete) return { vibrate: false, sound: false }

  let shouldAlert: boolean
  if (mode === 'manual') {
    if (status === 'asking') shouldAlert = notifyOnWaiting
    else if (status === 'completed') shouldAlert = soundOnTaskComplete
    else shouldAlert = status === 'interrupted'
  } else {
    if (status === 'completed') shouldAlert = soundOnTaskComplete
    else shouldAlert = status === 'interrupted'
  }

  if (!shouldAlert) return { vibrate: false, sound: false }
  return {
    vibrate,
    sound: status === 'completed' ? soundOnTaskComplete : true,
  }
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
   * 首次调用时检查权限，未授予则请求（Android 13+ 弹系统授权框）。
   * 结果缓存到模块级变量。
   */
  async function ensurePermission(): Promise<boolean> {
    if (!isAndroid()) return false

    // 使用缓存
    if (permissionGranted === true) return true

    try {
      const check = await invoke<{ granted: boolean }>('plugin:task-notification|checkNotificationPermission')
      let granted = check.granted
      if (!granted) {
        const req = await invoke<{ granted: boolean }>('plugin:task-notification|requestNotificationPermission')
        granted = req.granted
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

    const { vibrate, sound } = getAlertFlags(
      params.taskStatus,
      mode,
      settings.notifyOnWaiting ?? true,
      settings.vibrate ?? true,
      settings.soundOnTaskComplete ?? true
    )

    const body = buildTaskBody(params.taskStatus, params.taskReason)

    try {
      await invoke('plugin:task-notification|showTaskNotification', {
        sessionId: params.sessionId,
        title: params.sessionName,
        body,
        vibrate,
        sound,
      })
    } catch (e) {
      console.warn('[Notification] showTaskNotification failed:', e)
    }
  }

  /**
   * 取消指定会话的任务通知
   */
  async function cancelTaskNotification(sessionId: string): Promise<void> {
    if (!isAndroid()) return
    try {
      await invoke('plugin:task-notification|cancelTaskNotification', { sessionId })
    } catch (e) {
      console.warn('[Notification] cancelTaskNotification failed:', e)
    }
  }

  /**
   * 取消所有任务通知
   */
  async function cancelAllTaskNotifications(): Promise<void> {
    if (!isAndroid()) return
    try {
      await invoke('plugin:task-notification|cancelAllTaskNotifications')
    } catch (e) {
      console.warn('[Notification] cancelAllTaskNotifications failed:', e)
    }
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

    const settings = getMobileSettings()

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
      await invoke('plugin:task-notification|showConnectionNotification', {
        title: 'BedCode',
        body,
        // 连接状态为重要事件：震动受 vibrate 开关控制，声音跟随系统渠道
        vibrate: settings.vibrate ?? true,
        sound: true,
      })
    } catch (e) {
      console.warn('[Notification] showConnectionNotification failed:', e)
    }
  }

  /**
   * 取消连接状态通知
   */
  async function cancelConnectionNotification(): Promise<void> {
    if (!isAndroid()) return
    try {
      await invoke('plugin:task-notification|cancelConnectionNotification')
    } catch (e) {
      console.warn('[Notification] cancelConnectionNotification failed:', e)
    }
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
