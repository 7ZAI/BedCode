/**
 * 移动端宿主壳（Host Shell）公共入口
 * -----------------------------------------------------------------------------
 * 对外只暴露三类东西：契约类型、注册表、以及给应用用的注册 API。
 *
 * 应用（当前是插件，将来是 wasm-app）接入只需要这里的注册 API：
 *
 * ```ts
 * import { registerSurface, registerSlot, registerCapsuleItem } from '@/shell'
 *
 * // 运行面：应用在壳内被打开时渲染的界面（应用内页面全在这里，平台不实现）
 * registerSurface(appId, { component: MyAppView, accent: '#8BE9FD' })
 *
 * // 首页快捷卡片：应用自持内容（如「N 个活跃会话」「正在接收 3 个文件」）
 * registerSlot(appId, { id: 'status', component: MyStatusCard, order: 10 })
 *
 * // 胶囊附加项：插在平台项之间（平台项 order：权限 10 / 停用 30）
 * registerCapsuleItem(appId, { id: 'settings', label: '应用设置', order: 20 })
 *
 * // 平台设置入口
 * registerSettingsEntry(appId, { id: 'account', label: '账号', onSelect: open })
 * ```
 *
 * 返回值都是 Disposable：应用停用时 dispose 即摘除，不留残桩。
 */

export type {
  Disposable,
  ShellApp,
  ShellAppState,
  ShellAppSource,
  ShellPermissionGrant,
  ShellSlotContribution,
  ShellSurfaceContribution,
  ShellCapsuleItem,
  ShellSettingsEntry,
  ShellAppContributions,
} from './types'

export { getShellRegistry, type ShellRegistryClass } from './registry'
export { SHELL_PERMISSION_GROUPS } from './permissions'
export { formatBytes, initialOf, iconKindOf } from './utils'

export { useShellApps, type ShellApps, type ShellStats } from './composables/useShellApps'
export {
  useShellNavigation,
  resetShellNavigation,
  type ShellNavigation,
  type ShellScreenId,
  type ShellScreenEntry,
} from './composables/useShellNavigation'
export { useShellOverlays, type ShellOverlays, type ShellPermissionRequest } from './composables/useShellOverlays'
export { useShellRecent, pushRecent, clearRecent } from './composables/useShellRecent'
export { useShellOpen, openShellApp } from './composables/useShellOpen'

export { createPluginAppSource, PLUGIN_APP_SOURCE_ID, toShellApp } from './adapters/pluginAppSource'

import type { Disposable } from './types'
import type { ShellCapsuleItem, ShellSettingsEntry, ShellSlotContribution, ShellSurfaceContribution } from './types'
import { getShellRegistry } from './registry'

/**
 * 注册应用运行面
 *
 * 原型里的终端 / AI Chatbox / 文件传输界面都属于这一类：由应用自己实现，
 * 平台只提供挂载点。
 */
export function registerSurface(appId: string, surface: ShellSurfaceContribution): Disposable {
  return getShellRegistry().registerSurface(appId, surface)
}

/** 注册首页快捷卡片 */
export function registerSlot(appId: string, slot: ShellSlotContribution): Disposable {
  return getShellRegistry().registerSlot(appId, slot)
}

/** 注册胶囊菜单附加项 */
export function registerCapsuleItem(appId: string, item: ShellCapsuleItem): Disposable {
  return getShellRegistry().registerCapsuleItem(appId, item)
}

/** 注册平台设置入口 */
export function registerSettingsEntry(appId: string, entry: ShellSettingsEntry): Disposable {
  return getShellRegistry().registerSettingsEntry(appId, entry)
}
