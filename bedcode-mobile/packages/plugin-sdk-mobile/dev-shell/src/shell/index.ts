/**
 * Dev Shell 宿主壳公共入口
 * -----------------------------------------------------------------------------
 * 对外暴露三类东西：契约类型、注册表、以及给内置应用 / 调试代码用的注册 API。
 *
 * 内置应用接入只需这里的注册 API：
 *
 * ```ts
 * import { registerSurface, registerSlot } from './shell'
 *
 * registerSurface('dev.my-app', { component: MyAppView })          // 运行面
 * registerSlot('dev.my-app', { id: 'status', component: MyCard })   // 首页快捷卡片
 * ```
 *
 * 被调试插件走的是 `context.ui.*`（mock-context 内的同款实现），appId 由插件 id 代填。
 *
 * 返回值都是 Disposable：应用停用时 dispose 即摘除，不留残桩。
 */

export type {
  Disposable,
  ShellApp,
  ShellAppState,
  ShellAppContributions,
  ShellAppSource,
  ShellCapsuleItem,
  ShellPermissionGrant,
  ShellSettingsEntry,
  ShellSlotContribution,
  ShellSurfaceContribution,
} from './types'

export { getShellRegistry, type ShellRegistryClass } from './registry'
export {
  SHELL_PERMISSION_GROUPS,
  isLockedPermission,
  permissionTitleKey,
  groupPermissions,
} from './permissions'
export { formatBytes, initialOf, iconKindOf, type IconKind } from './utils'
export { logger } from './logger'

export { useShellApps, type ShellApps, type ShellStats } from './composables/useShellApps'
export {
  useShellNavigation,
  resetShellNavigation,
  type ShellNavigation,
  type ShellScreenId,
  type ShellScreenEntry,
  type ShellNavDirection,
} from './composables/useShellNavigation'
export {
  useShellOverlays,
  type ShellOverlays,
  type ShellPermissionRequest,
} from './composables/useShellOverlays'
export { useShellRecent, pushRecent, clearRecent } from './composables/useShellRecent'
export { useShellOpen, openShellApp } from './composables/useShellOpen'
export { useToast, type ToastOptions } from './composables/useToast'

export {
  createDevAppSource,
  DEV_APP_SOURCE_ID,
  toShellApp,
  type DevAppSourceDeps,
} from './adapters/devAppSource'

import type {
  Disposable,
  ShellCapsuleItem,
  ShellSettingsEntry,
  ShellSlotContribution,
  ShellSurfaceContribution,
} from './types'
import { getShellRegistry } from './registry'

/** 注册应用运行面 */
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