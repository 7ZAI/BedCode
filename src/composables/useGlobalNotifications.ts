import { listen } from '@tauri-apps/api/event'
import { useToast } from './useToast'
import { useSessionStore } from '@/stores/session'

let unlistenDeviceConnected: (() => void) | null = null
let unlistenDeviceDisconnected: (() => void) | null = null
let unlistenSessionCreated: (() => void) | null = null
let unlistenSessionStopped: (() => void) | null = null

interface SessionEventPayload {
  type: string
  event_type?: string
  session?: { id: string; name: string; status: string }
  device_name?: string
}

interface DeviceEventPayload {
  addr?: string
  device_id?: string
  device_name?: string
  event?: string
}

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
        const deviceName = event.payload.device_name || '移动设备'
        toast.info(`"${deviceName}" 已连接`)
      })
    }

    // 设备断开事件
    if (!unlistenDeviceDisconnected) {
      unlistenDeviceDisconnected = await listen<DeviceEventPayload>('device-disconnected', (event) => {
        const deviceName = event.payload.device_name || '移动设备'
        toast.warning(`"${deviceName}" 已断开连接`)
      })
    }

    // 移动端创建的会话
    if (!unlistenSessionCreated) {
      unlistenSessionCreated = await listen<SessionEventPayload>('session-created-from-mobile', (event) => {
        const deviceName = event.payload.device_name || '移动端'
        const sessionName = event.payload.session?.name || ''
        const msg = sessionName
          ? `"${deviceName}" 创建了会话 "${sessionName}"`
          : `"${deviceName}" 创建了新会话`
        toast.success(msg)
        sessionStore.loadSessions()
      })
    }

    // 移动端停止的会话
    if (!unlistenSessionStopped) {
      unlistenSessionStopped = await listen<SessionEventPayload>('session-stopped-from-mobile', (event) => {
        const deviceName = event.payload.device_name || '移动端'
        const sessionName = event.payload.session?.name || ''
        const msg = sessionName
          ? `"${deviceName}" 停止了会话 "${sessionName}"`
          : `"${deviceName}" 停止了会话`
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
