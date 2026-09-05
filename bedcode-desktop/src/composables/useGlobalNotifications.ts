import { listen } from '@tauri-apps/api/event'
import i18n from '@/locales'
import { useToast } from './useToast'
import { useSessionStore } from '@/stores/session'

// Re-export from model
import type { SessionEventPayload, DeviceEventPayload } from './model'
export type { SessionEventPayload, DeviceEventPayload }

let unlistenDeviceConnected: (() => void) | null = null
let unlistenDeviceDisconnected: (() => void) | null = null
let unlistenSessionCreated: (() => void) | null = null
let unlistenSessionStopped: (() => void) | null = null
let unlistenPeerConsentRequested: (() => void) | null = null
let unlistenPeerConnected: (() => void) | null = null
let unlistenPeerDisconnected: (() => void) | null = null

/** peer-net 连接事件载荷（peer_net.rs emit_json 契约，camelCase） */
interface PeerEventPayload {
  nodeId?: string
  deviceName?: string | null
  fingerprintShort?: string
}

/** 短指纹兜底展示：无设备名时以指纹呈现，避免裸 nodeId 长串 */
function peerDisplayName(payload: PeerEventPayload): string {
  if (payload.deviceName) return payload.deviceName
  return payload.fingerprintShort || (payload.nodeId || '').slice(0, 8) || 'device'
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
        const deviceName = event.payload.device_name || i18n.global.t('common.misc.mobileDevice')
        toast.info(i18n.global.t('common.notification.deviceConnected', { name: deviceName }))
      })
    }

    // 设备断开事件
    if (!unlistenDeviceDisconnected) {
      unlistenDeviceDisconnected = await listen<DeviceEventPayload>(
        'device-disconnected',
        (event) => {
          const deviceName = event.payload.device_name || i18n.global.t('common.misc.mobileDevice')
          toast.warning(
            i18n.global.t('common.notification.deviceDisconnected', { name: deviceName }),
          )
        },
      )
    }

    // 移动端创建的会话
    if (!unlistenSessionCreated) {
      unlistenSessionCreated = await listen<SessionEventPayload>(
        'session-created-from-mobile',
        (event) => {
          const deviceName = event.payload.device_name || i18n.global.t('common.misc.mobileClient')
          const sessionName = event.payload.session?.name || ''
          const msg = sessionName
            ? i18n.global.t('common.notification.sessionCreated', {
                device: deviceName,
                name: sessionName,
              })
            : i18n.global.t('common.notification.sessionCreatedNoName', { device: deviceName })
          toast.success(msg)
          sessionStore.loadSessions()
        },
      )
    }

    // 移动端停止的会话
    if (!unlistenSessionStopped) {
      unlistenSessionStopped = await listen<SessionEventPayload>(
        'session-stopped-from-mobile',
        (event) => {
          const deviceName = event.payload.device_name || i18n.global.t('common.misc.mobileClient')
          const sessionName = event.payload.session?.name || ''
          const msg = sessionName
            ? i18n.global.t('common.notification.sessionStoppedByDevice', {
                device: deviceName,
                name: sessionName,
              })
            : i18n.global.t('common.notification.sessionStoppedNoName', { device: deviceName })
          toast.info(msg)
          sessionStore.loadSessions()
        },
      )
    }

    // 对等连接请求（文件传输首连确认）：宿主级感知——即使 file-transfer
    // 插件关闭（插件内确认弹窗不出现），桌面用户也能看到有设备请求连接
    if (!unlistenPeerConsentRequested) {
      unlistenPeerConsentRequested = await listen<PeerEventPayload>(
        'peer-consent-requested',
        (event) => {
          toast.warning(
            i18n.global.t('common.notification.peerConsentRequested', {
              name: peerDisplayName(event.payload),
            }),
          )
        },
      )
    }

    // 对等连接建立 / 断开（peer-net 链路，区别于 WS 终端链路的 device-*）
    if (!unlistenPeerConnected) {
      unlistenPeerConnected = await listen<PeerEventPayload>('peer-connected', (event) => {
        toast.success(
          i18n.global.t('common.notification.peerConnected', {
            name: peerDisplayName(event.payload),
          }),
        )
      })
    }
    if (!unlistenPeerDisconnected) {
      unlistenPeerDisconnected = await listen<PeerEventPayload>('peer-disconnected', (event) => {
        toast.warning(
          i18n.global.t('common.notification.peerDisconnected', {
            name: peerDisplayName(event.payload),
          }),
        )
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
    if (unlistenPeerConsentRequested) {
      unlistenPeerConsentRequested()
      unlistenPeerConsentRequested = null
    }
    if (unlistenPeerConnected) {
      unlistenPeerConnected()
      unlistenPeerConnected = null
    }
    if (unlistenPeerDisconnected) {
      unlistenPeerDisconnected()
      unlistenPeerDisconnected = null
    }
  }

  return {
    startListening,
    stopListening,
  }
}
