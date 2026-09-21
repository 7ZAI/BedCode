/**
 * 对等连接通知 — 宿主全局通知的插件侧承接
 *
 * 背景：宿主 `useGlobalNotifications`（已退役）曾替所有业务域弹 toast。对等链路
 * 连接建立/断开是本插件（file-transfer）的业务事实，故在插件激活期常驻订阅宿主
 * `peer-connected` / `peer-disconnected`（`peer_net.rs` emit 的 camelCase 载荷），
 * 用共享 `vue-sonner` 实例 + 插件 i18n 展示。
 *
 * 去重语义（沿用宿主原实现，防回归）：数据面短连接（浏览 / 拉取各自新拨）会让
 * `peer-connected` / `peer-disconnected` 高频重复，后端已按引用计数去重
 * （inbound 首连才发 connected、末连才发 disconnected），本处按 nodeId 再兜一层：
 * 仅真实状态跃迁才提示。
 *
 * 首连确认（`peer-consent-requested`）不在本模块：该链路已由 `useConsent` 经宿主
 * `peer:consent` topic → WASM 代理 → 插件事件闭环（应用内确认弹窗），宿主侧原先
 * 那条「插件关闭时的 toast 兜底」随 `useGlobalNotifications` 一并删除——插件未激活
 * 即无该业务，不再由宿主代答。
 */
import type { Disposable, PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import { toast } from 'vue-sonner'

/** 宿主 peer-* 事件载荷（`peer_net.rs` emit_json 契约，camelCase） */
export interface PeerEventPayload {
  nodeId?: string
  deviceName?: string | null
  fingerprintShort?: string
}

/** 事件方向：载荷本身不含方向，由订阅通道决定 */
export type PeerEventKind = 'connected' | 'disconnected'

/** 判定结果：`ignore` = 非状态跃迁（重复连接 / 未知断开 / 缺身份） */
export type PeerEventDecision = 'connected' | 'disconnected' | 'ignore'

/** 已连接对端跟踪器（纯逻辑，无 IO） */
export interface PeerOnlineTracker {
  observe(payload: PeerEventPayload | null | undefined, kind: PeerEventKind): PeerEventDecision
  readonly onlineCount: number
  clear(): void
}

/** 去重主键：nodeId 是 peer-net 的稳定节点身份（缺失即忽略） */
export function peerOnlineKey(payload: PeerEventPayload | null | undefined): string {
  return payload?.nodeId || ''
}

export function createPeerOnlineTracker(): PeerOnlineTracker {
  const online = new Set<string>()
  return {
    observe(payload, kind) {
      const key = peerOnlineKey(payload)
      if (!key) return 'ignore'
      if (kind === 'connected') {
        if (online.has(key)) return 'ignore'
        online.add(key)
        return 'connected'
      }
      if (!online.has(key)) return 'ignore'
      online.delete(key)
      return 'disconnected'
    },
    get onlineCount() {
      return online.size
    },
    clear() {
      online.clear()
    },
  }
}

/**
 * 展示名：设备名优先，兜底短指纹，再兜底 nodeId 前 8 位
 *
 * 仅在事件身份有效（nodeId 非空）时调用——缺 nodeId 的事件已被 tracker 忽略，
 * 故不存在「无任何可展示身份」的分支（无需再套一层兜底文案）。
 */
export function peerDisplayName(payload: PeerEventPayload | null | undefined): string {
  if (!payload) return ''
  if (payload.deviceName) return payload.deviceName
  return payload.fingerprintShort || (payload.nodeId || '').slice(0, 8)
}

/**
 * 启动对等连接通知（插件激活期常驻）
 *
 * @returns dispose 句柄：注销监听并清空在线集合
 */
export function startPeerNotifications(context: PluginContext): Disposable {
  const tracker = createPeerOnlineTracker()

  const connected = context.events.on('peer-connected', (payload: PeerEventPayload) => {
    if (tracker.observe(payload, 'connected') !== 'connected') return
    toast.success(
      context.i18n.t('transfer.notification.peerConnected', { name: peerDisplayName(payload) }),
    )
  })

  const disconnected = context.events.on('peer-disconnected', (payload: PeerEventPayload) => {
    if (tracker.observe(payload, 'disconnected') !== 'disconnected') return
    toast.warning(
      context.i18n.t('transfer.notification.peerDisconnected', { name: peerDisplayName(payload) }),
    )
  })

  return {
    dispose() {
      connected.dispose()
      disconnected.dispose()
      tracker.clear()
    },
  }
}
