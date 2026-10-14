/**
 * 壳通知中心（平台事件可见面）
 * -----------------------------------------------------------------------------
 * 归属（AGENTS §5.1）：宿主只承载**机制面**——来源 / 图标 / 关联应用 / 时间 / 驻留 /
 * 动作，标题与正文由来源方给出；壳不解析任何业务内容（B4 不命中），也不把通知
 * 落库成真源（B3 不命中，刷新即清空）。
 *
 * 数据来源只有两条，都是平台自身的事实：
 *   ① 壳的动作结果（启动 / 停止 / 审批 / 安装 / 卸载 / 启动失败）——见 useShellApps；
 *   ② 宿主引擎事件（连接建立 / 重连 / 重认证被拒）——见 useShellHostEvents。
 * 待审批条目**不在这里**：它由应用状态 `needsApproval` 动态派生（批准后自然消失，
 * 无独立生命周期），派生点在通知栏组件里。
 *
 * 与「授权弹窗」的区别：弹窗是**必须当场裁决**的安全闸门（fail-closed），
 * 通知只是事后可见面，二者不互相替代。
 */

import { computed, ref, type ComputedRef, type Ref } from 'vue'

/** 通知种类：进度类带进度条，其余为普通卡片 */
export type ShellNotificationKind = 'event' | 'info' | 'progress'

/** 一条平台通知 */
export interface ShellNotification {
  id: string
  kind: ShellNotificationKind
  /** 来源方给出的图标（SVG path d，24×24 stroke 视框） */
  icon: string
  /** 来源名（已翻译） */
  source: string
  title: string
  body?: string
  /** 产生时刻（epoch ms，UI 侧换算相对时间） */
  at: number
  /** 关联应用 id：有值时提供「打开」动作 */
  appId?: string
  /** 进度 0–100（kind === 'progress'） */
  progress?: number
  /** 驻留项：清除全部时保留 */
  sticky?: boolean
}

/** push 入参：id 与时间由本模块生成，调用方不必关心 */
export type ShellNotificationInput = Omit<ShellNotification, 'id' | 'at' | 'sticky'> & {
  sticky?: boolean
  at?: number
}

/**
 * 列表上限：通知是可见面而非日志，超出后丢弃最旧的非驻留条目。
 * 没有上限的话，长时间运行的会话会把内存与渲染成本都拖起来。
 */
const MAX_NOTIFICATIONS = 50

let seq = 0

const notifications: Ref<ShellNotification[]> = ref([])

/** 最新在前（通知中心自上而下按时间倒序） */
function pushNotification(input: ShellNotificationInput): ShellNotification {
  const entry: ShellNotification = {
    id: `shell-n${++seq}`,
    at: input.at ?? Date.now(),
    sticky: input.sticky ?? false,
    kind: input.kind,
    icon: input.icon,
    source: input.source,
    title: input.title,
    body: input.body,
    appId: input.appId,
    progress: input.progress,
  }
  const next = [entry, ...notifications.value]
  notifications.value =
    next.length > MAX_NOTIFICATIONS
      ? [...next.filter((n) => n.sticky), ...next.filter((n) => !n.sticky)].slice(0, MAX_NOTIFICATIONS)
      : next
  return entry
}

function dismissNotification(id: string): void {
  notifications.value = notifications.value.filter((n) => n.id !== id)
}

/** 清除全部：驻留项保留（与桌面壳通知中心同口径） */
function clearNotifications(): void {
  notifications.value = notifications.value.filter((n) => n.sticky)
}

/** 通知条数（角标基数；待审批数由调用方另行相加） */
const notificationCount: ComputedRef<number> = computed(() => notifications.value.length)

export interface ShellNotifications {
  notifications: ComputedRef<ShellNotification[]>
  notificationCount: ComputedRef<number>
  pushNotification: typeof pushNotification
  dismissNotification: typeof dismissNotification
  clearNotifications: typeof clearNotifications
}

/** 获取壳通知中心句柄（模块级单例：全壳一份列表，任何屏幕都能推） */
export function useShellNotifications(): ShellNotifications {
  return {
    notifications: computed(() => notifications.value),
    notificationCount,
    pushNotification,
    dismissNotification,
    clearNotifications,
  }
}

/** 供测试复位（模块级单例需要显式清空） */
export function resetShellNotifications(): void {
  notifications.value = []
  seq = 0
}
