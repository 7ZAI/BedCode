/**
 * 设备连接通知 — 宿主全局通知的插件侧承接
 *
 * 背景：宿主 `useGlobalNotifications`（已退役）曾替所有业务域弹 toast。设备上下线
 * 是设备域事实，归属本插件，故在插件激活期常驻订阅宿主 `device-connected` /
 * `device-disconnected`（`server/ws/conn.rs` emit 的 snake_case 载荷），用共享
 * `vue-sonner` 实例 + 插件 i18n 展示——宿主不再替本域说话。
 *
 * 去重语义（沿用宿主原实现，防回归）：后端在 4 处发 `device-connected`
 * （配对码 / QR / reauth + 每条已认证事件 WS），「上线」只应提示一次 → 按稳定
 * 设备身份（指纹，兜底 device_id / addr）键控，仅在 offline→online 跃迁时提示；
 * `device-disconnected` 反向守卫：不在集合内的断开视为未知状态不提示。
 *
 * 启动期基线：应用启动时设备可能已在线（长驻事件 WS 存活），故激活时先经本插件
 * 命令面 `session.devices.connect-list`（连接注册表派生视图，含 fingerprint）
 * 种子化在线集合，否则该设备真实的断开事件会被误吞。种子化失败只降级为
 * 纯事件驱动，不阻断激活。
 *
 * 已退役（本模块不接管）：宿主的 `session-created-from-mobile` /
 * `session-stopped-from-mobile` 两个监听是**死链**（全仓无 emit 点），随宿主文件
 * 一并删除，不做迁移。
 */
import type { Disposable, PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import { toast } from 'vue-sonner'

/** 宿主 `device-*` 事件载荷（仅取身份字段；其余字段本模块不消费） */
export interface DeviceEventPayload {
  addr?: string
  device_id?: string
  fingerprint?: string
  device_name?: string
}

/** 事件方向：载荷本身不含方向，由订阅通道决定 */
export type DeviceEventKind = 'connected' | 'disconnected'

/** 在线态判定结果：`ignore` = 非状态跃迁（重复上线 / 未知断开 / 缺身份），不提示 */
export type DeviceEventDecision = 'connected' | 'disconnected' | 'ignore'

/** 在线设备跟踪器（纯逻辑，无 IO）：去重判定的可测单元 */
export interface DeviceOnlineTracker {
  /** 种入启动期已在线设备的指纹（激活基线） */
  seed(fingerprints: Iterable<string>): void
  /** 观察一条设备事件，返回应执行的动作 */
  observe(
    payload: DeviceEventPayload | null | undefined,
    kind: DeviceEventKind,
  ): DeviceEventDecision
  /** 当前在线指纹数（自检用） */
  readonly onlineCount: number
  /** 清空（停用/停止订阅时调用） */
  clear(): void
}

/**
 * 设备在线态去重主键：指纹优先，兜底 device_id / addr
 *
 * 三者皆缺失时返回空串 —— 调用方按「身份不明」忽略，绝不把空键写入集合
 * （否则所有缺字段事件会互相顶替，造成提示错乱）。
 */
export function deviceOnlineKey(payload: DeviceEventPayload | null | undefined): string {
  if (!payload) return ''
  return payload.fingerprint || payload.device_id || payload.addr || ''
}

export function createDeviceOnlineTracker(): DeviceOnlineTracker {
  const online = new Set<string>()
  return {
    seed(fingerprints) {
      for (const fp of fingerprints) {
        if (fp) online.add(fp)
      }
    },
    observe(payload, kind) {
      const key = deviceOnlineKey(payload)
      if (!key) return 'ignore'
      if (kind === 'connected') {
        // 已在线 = 重复上线（配对码 / QR / reauth / 事件 WS 各自发一次），静默
        if (online.has(key)) return 'ignore'
        online.add(key)
        return 'connected'
      }
      // 断开：非本会话已知在线（事件早于订阅或已被清理）→ 未知状态，不提示
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

/** 展示名：设备名优先，缺失回退「移动设备」（与宿主原口径一致） */
function deviceDisplayName(payload: DeviceEventPayload, fallback: string): string {
  return payload.device_name || fallback
}

/**
 * 启动设备连接通知（插件激活期常驻）
 *
 * @returns dispose 句柄：注销监听并清空在线集合
 */
export function startDeviceNotifications(context: PluginContext): Disposable {
  const tracker = createDeviceOnlineTracker()
  const mobileDevice = () => context.i18n.t('session.notification.mobileDevice')

  // 启动期种子化：在线设备在激活前已连接（长驻 WS），不预置会误吞其真实断开提示
  void (async () => {
    try {
      const reply = (await context.commands.execute('session.devices.connect-list', {})) as {
        connections?: DeviceEventPayload[]
      }
      const list = Array.isArray(reply?.connections) ? reply.connections : []
      tracker.seed(list.map((c) => c?.fingerprint || ''))
    } catch (e) {
      // 种子化失败不阻断监听：退化为纯事件驱动（连接事件仍会正确去重）
      console.warn('[Session Center] seed online devices failed:', e)
    }
  })()

  const connected = context.events.on('device-connected', (payload: DeviceEventPayload) => {
    if (tracker.observe(payload, 'connected') !== 'connected') return
    toast.info(
      context.i18n.t('session.notification.deviceConnected', {
        name: deviceDisplayName(payload ?? {}, mobileDevice()),
      }),
    )
  })

  const disconnected = context.events.on('device-disconnected', (payload: DeviceEventPayload) => {
    if (tracker.observe(payload, 'disconnected') !== 'disconnected') return
    toast.warning(
      context.i18n.t('session.notification.deviceDisconnected', {
        name: deviceDisplayName(payload ?? {}, mobileDevice()),
      }),
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
