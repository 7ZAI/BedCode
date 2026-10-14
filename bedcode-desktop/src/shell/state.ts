/**
 * 设备壳纯逻辑助手（无 Vue 依赖，可直接单元测试）
 *
 * 全部为确定性纯函数：卡片 MRU 重排 / 关闭、展示态归并、待审批派生、
 * 通知计数、图标色调哈希。业务事实不落这里——应用数据在 useShellApps。
 */
import type { AppDisplayState, CardKey, HostNotif, SysPageId } from './types'
import type { PluginInfo, PluginState } from '@/plugin/types'

/** 系统页清单（宿主自有面；nameKey/descKey 为 i18n key，非直接文案） */
export const SYS_PAGES: ReadonlyArray<{
  id: SysPageId
  nameKey: string
  descKey: string
  icon: string
}> = [
  { id: 'apps', nameKey: 'shell.sys.apps.name', descKey: 'shell.sys.apps.desc', icon: 'grid' },
  { id: 'perms', nameKey: 'shell.sys.perms.name', descKey: 'shell.sys.perms.desc', icon: 'shield' },
  {
    id: 'settings',
    nameKey: 'shell.sys.settings.name',
    descKey: 'shell.sys.settings.desc',
    icon: 'gear',
  },
]

/** 系统页元数据（id → 条目）；未知 id 返回 undefined */
export function sysPageOf(id: string): (typeof SYS_PAGES)[number] | undefined {
  return SYS_PAGES.find((s) => s.id === id)
}

// ==================== 卡片键 ====================

export function appCardKey(pluginId: string): CardKey {
  return `app:${pluginId}`
}

export function sysCardKey(pageId: string): CardKey {
  return `sys:${pageId}`
}

export function cardKind(key: CardKey): 'app' | 'sys' {
  return key.startsWith('app:') ? 'app' : 'sys'
}

export function cardId(key: CardKey): string {
  return key.slice(key.indexOf(':') + 1)
}

/** MRU 激活：最新在前，重复激活只重排不重复产卡（spec-c §1） */
export function mruActivate(cards: readonly CardKey[], key: CardKey): CardKey[] {
  return [key, ...cards.filter((k) => k !== key)]
}

/** MRU 移除 */
export function mruRemove(cards: readonly CardKey[], key: CardKey): CardKey[] {
  return cards.filter((k) => k !== key)
}

// ==================== 展示态归并 ====================

/**
 * PluginState → 壳展示态（与原型 STATE_TEXT 五档对齐）：
 * - activated → running；degraded → degraded（实例在跑，扩展点已注册）
 * - needsApproval → approval（进审批屏卡）
 * - error → error
 * - 其余（loaded / activating / deactivated）→ dormant（未启动，即点即启）
 */
export function appDisplayState(state: PluginState | undefined): AppDisplayState {
  if (!state) return 'dormant'
  switch (state.state) {
    case 'activated':
      return 'running'
    case 'degraded':
      return 'degraded'
    case 'needsApproval':
      return 'approval'
    case 'error':
      return 'error'
    default:
      return 'dormant'
  }
}

/** 实例在跑（running / degraded）——关闭卡片会真正停用实例，卸载需先停用 */
export function isRunningState(state: PluginState | undefined): boolean {
  const d = appDisplayState(state)
  return d === 'running' || d === 'degraded'
}

// ==================== 待审批派生 ====================

/** 待审批应用列表（审批通知与审批屏卡的动态真源，批准后自然消失） */
export function approvalApps(apps: readonly PluginInfo[]): PluginInfo[] {
  return apps.filter((a) => appDisplayState(a.state) === 'approval')
}

/** 通知计数 = 通知列表 + 待审批数（铃铛角标） */
export function notifCount(notifs: readonly HostNotif[], approvals: readonly PluginInfo[]): number {
  return notifs.length + approvals.length
}

/** 高危权限位（manifest 声明里真正危险的位；进程/终端/输入/主库） */
export const HIGH_RISK_PERMISSIONS: ReadonlySet<string> = new Set([
  'process:run',
  'pty:spawn',
  'terminal:input',
  'database:main',
])

export function isHighRisk(perm: string): boolean {
  return HIGH_RISK_PERMISSIONS.has(perm)
}

// ==================== 应用图标色调（机制级，零产品映射） ====================

/** 壳内可用的图标底色档（tone-* 类，全部 color-mix 自既有语义 token） */
export const APP_TONES = ['primary', 'info', 'success', 'warning', 'mix'] as const
export type AppTone = (typeof APP_TONES)[number]

/**
 * 插件 id → 色调档（确定性哈希，双端一致）。
 *
 * 显式理由：原型按产品 id 硬编码色调映射（terminal-session=primary 等）是
 * B5 业务默认值，宿主红线禁止——改为纯机制哈希，新增插件自动获得稳定配色，
 * 不因产品命名而特殊。
 */
export function toneOf(pluginId: string): AppTone {
  let h = 0
  for (let i = 0; i < pluginId.length; i++) {
    h = (h * 31 + pluginId.charCodeAt(i)) >>> 0
  }
  return APP_TONES[h % APP_TONES.length]
}

// ==================== 壁纸 ====================

/** 主屏壁纸清单（渐变真源是 style.css :root 的 --wall-*，这里只列 id 与 i18n key） */
export const WALLPAPERS: ReadonlyArray<{ id: string; nameKey: string }> = [
  { id: 'none', nameKey: 'shell.settings.background.wall.none' },
  { id: 'aurora', nameKey: 'shell.settings.background.wall.aurora' },
  { id: 'dune', nameKey: 'shell.settings.background.wall.dune' },
  { id: 'reef', nameKey: 'shell.settings.background.wall.reef' },
]

export const WALLPAPER_DEFAULT = 'aurora'
export const WALL_DIM_DEFAULT = 30
export const WALL_DIM_MAX = 70
