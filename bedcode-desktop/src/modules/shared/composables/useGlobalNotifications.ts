import { listen } from '@tauri-apps/api/event'
import i18n from '@/locales'
import { useToast } from './useToast'
import { useSessionStore } from '@/modules/shared/stores/session'

// Re-export from model
import type { SessionEventPayload, DeviceEventPayload } from './model'
export type { SessionEventPayload, DeviceEventPayload }

let unlistenDeviceConnected: (() => void) | null = null
let unlistenDeviceDisconnected: (() => void) | null = null
let unlistenSessionCreated: (() => void) | null = null
let unlistenSessionStopped: (() => void) | null = null



/**
 * 全局通知监听
 *
 * 监听后端发出的设备连接/断开和会话创建/停止事件
 * 在桌面端显示 toast 通知
 */
export function useGlobalNotifications() {
  const toast = useToast()
  const sessionStore = useSessionStore()

  async function startListening() {
    // 设备连接事件
    if (!unlistenDeviceConnected) {
      unlistenDeviceConnected = await listen<DeviceEventPayload>('device-connected', (event) => {
        const deviceName = event.payload.device_name || i18n.global.t('common.misc.mobileDevice')
        toast.info(i18n.global.t('common.notification.deviceConnected', { name: deviceName }))
      })
    }

    // 设备断开事件
    if (!unlistenDeviceDisconnected) {
      unlistenDeviceDisconnected = await listen<DeviceEventPayload>('device-disconnected', (event) => {
        const deviceName = event.payload.device_name || i18n.global.t('common.misc.mobileDevice')
        toast.warning(i18n.global.t('common.notification.deviceDisconnected', { name: deviceName }))
      })
    }

    // 移动端创建的会话
    if (!unlistenSessionCreated) {
      unlistenSessionCreated = await listen<SessionEventPayload>('session-created-from-mobile', (event) => {
        const deviceName = event.payload.device_name || i18n.global.t('common.misc.mobileClient')
        const sessionName = event.payload.session?.name || ''
        const msg = sessionName
          ? i18n.global.t('common.notification.sessionCreated', { device: deviceName, name: sessionName })
          : i18n.global.t('common.notification.sessionCreatedNoName', { device: deviceName })
        toast.success(msg)
        sessionStore.loadSessions()
      })
    }

    // 移动端停止的会话
    if (!unlistenSessionStopped) {
      unlistenSessionStopped = await listen<SessionEventPayload>('session-stopped-from-mobile', (event) => {
        const deviceName = event.payload.device_name || i18n.global.t('common.misc.mobileClient')
        const sessionName = event.payload.session?.name || ''
        const msg = sessionName
          ? i18n.global.t('common.notification.sessionStoppedByDevice', { device: deviceName, name: sessionName })
          : i18n.global.t('common.notification.sessionStoppedNoName', { device: deviceName })
        toast.info(msg)
        sessionStore.loadSessions()
      })
    }
  }

  function stopListening() {
    if (unlistenDeviceConnected) {
      unlistenDeviceConnected()
      unlistenDeviceConnected = null
    }
    if (unlistenDeviceDisconnected) {
      unlistenDeviceDisconnected()
      unlistenDeviceDisconnected = null
    }
    if (unlistenSessionCreated) {
      unlistenSessionCreated()
      unlistenSessionCreated = null
    }
    if (unlistenSessionStopped) {
      unlistenSessionStopped()
      unlistenSessionStopped = null
    }
  }

  return {
    startListening,
    stopListening,
  }
}
