import { listen } from '@tauri-apps/api/event'
import i18n from '@/locales'
import { useToast } from './useToast'
import { useSessionStore } from '@/stores/session'
import { getConnectedDevices } from './useDesktopCommands'
import { logger } from '@/utils/frontendLogger'

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

/**
 * 当前已连接的对端 nodeId 集合（去重）：数据面短连接（浏览/拉取各自新拨）会
 * 让 peer-connected / peer-disconnected 高频重复，仅在真实状态跃迁时 toast
 * （2026-09-07 实机实证：每次连接/断开弹出一堆 toast）。
 *
 * 后端 peer_net.rs 已按引用计数去重（inbound 首连才发 connected、末连才发
 * disconnected），本处是防回归兜底——若后端计数逻辑漂移漏发/多发，前端仍有
 * 最后一道去重；两侧去重语义不同步时以实际为准，勿把本处当成唯一防抖层
 */
const connectedPeerIds = new Set<string>()

/**
 * 当前已在线设备指纹集合（去重）：后端在 4 处发 `device-connected`
 * （HTTP 配对码 / QR / reauth + 每条已认证事件 WS），而设备「上线」语义
 * 应只提示一次——按指纹键控，仅在真实 offline→online 跃迁时 toast；
 * `device-disconnected` 反向守卫：不在集合内的断开视为未知状态不提示。
 *
 * 与 connectedPeerIds 同理（2026-09-07 实机实证的 peer 后端计数去重 +
 * 前端兜底模式）；device 事件后端无连接计数去重，本处即主去重层。
 */
const connectedDeviceFps = new Set<string>()

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

/** 设备在线态去重主键：指纹（稳定设备身份，与 DevicesView 追踪键一致），
 *  兜底 device_id / addr——后端 `device-*` 事件载荷均携带其一 */
function deviceOnlineKey(payload: DeviceEventPayload): string {
  return payload.fingerprint || payload.device_id || payload.addr || ''
}

/** 种子化已在线设备指纹：应用启动时设备可能已连接（长驻事件 WS 存活），
 *  不预置会将该设备真实的断开 toast 误吞（其连接事件先于本会话发生） */
async function seedConnectedDevices() {
  try {
    const devices = await getConnectedDevices()
    for (const d of devices) {
      if (d.fingerprint) connectedDeviceFps.add(d.fingerprint)
    }
    logger.log(
      `[useGlobalNotifications] seeded ${connectedDeviceFps.size} online device fingerprint(s)`,
    )
  } catch (e) {
    // 种子化失败不阻断监听：退化为纯事件驱动（连接事件仍会正确去重）
    logger.warn('[useGlobalNotifications] seed connected devices failed:', e)
  }
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
    // 种子化已在线设备指纹：先于监听注册，保证断开判定有基线
    await seedConnectedDevices()

    // 设备连接事件（上线去重：指纹已在集合 = 设备本就在线，静默——配对码 /
    // QR / reauth 与事件 WS 每次认证都会发 device-connected，设备未离线时
    // 重复提示是噪音）
    if (!unlistenDeviceConnected) {
      unlistenDeviceConnected = await listen<DeviceEventPayload>('device-connected', (event) => {
        const key = deviceOnlineKey(event.payload)
        if (!key || connectedDeviceFps.has(key)) return
        connectedDeviceFps.add(key)
        const deviceName = event.payload.device_name || i18n.global.t('common.misc.mobileDevice')
        toast.info(i18n.global.t('common.notification.deviceConnected', { name: deviceName }))
      })
    }

    // 设备断开事件（后端仅在最后一条事件通道关闭时发；集合守卫：仅对已
    // 识别为在线的设备提示，避免未知状态误报）
    if (!unlistenDeviceDisconnected) {
      unlistenDeviceDisconnected = await listen<DeviceEventPayload>(
        'device-disconnected',
        (event) => {
          const key = deviceOnlineKey(event.payload)
          if (!key || !connectedDeviceFps.has(key)) return
          connectedDeviceFps.delete(key)
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

    // 对等连接建立 / 断开（peer-net 链路，区别于 WS 终端链路的 device-*）。
    // 按 nodeId 去重：仅在真实状态跃迁时 toast（connected 去重/ disconnected
    // 去重），避免数据面短连接 churn 造成 toast 风暴
    if (!unlistenPeerConnected) {
      unlistenPeerConnected = await listen<PeerEventPayload>('peer-connected', (event) => {
        const id = event.payload.nodeId
        if (!id || connectedPeerIds.has(id)) return
        connectedPeerIds.add(id)
        toast.success(
          i18n.global.t('common.notification.peerConnected', {
            name: peerDisplayName(event.payload),
          }),
        )
      })
    }
    if (!unlistenPeerDisconnected) {
      unlistenPeerDisconnected = await listen<PeerEventPayload>('peer-disconnected', (event) => {
        const id = event.payload.nodeId
        if (!id || !connectedPeerIds.has(id)) return
        connectedPeerIds.delete(id)
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
    connectedPeerIds.clear()
    connectedDeviceFps.clear()
  }

  return {
    startListening,
    stopListening,
  }
}
