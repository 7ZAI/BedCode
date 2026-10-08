/**
 * 宿主壳权限展示元数据
 * -----------------------------------------------------------------------------
 * 权限词汇的真源是插件 SDK（packages/plugin-sdk-mobile/rust/src/permission.rs），
 * 本文件不复制词表，只做两件展示层的事：
 *   1. 把权限词映射成既有的本地化文案 key（复用 mobile.plugin.perm.*，不另造一套）
 *   2. 按能力域分组，让「应用详情 → 权限」是可读的清单而不是一串冒号字符串
 *
 * 授权裁决仍然在 Rust 端执行，前端开关只是 UX（AGENTS.md §8）——因此这里
 * 的分组与文案不参与任何安全判定。
 */

import type { ShellPermissionGrant } from './types'

/** 权限分组定义（展示顺序即数组顺序） */
export interface ShellPermissionGroup {
  id: string
  /** 分组标题的 i18n key */
  titleKey: string
  keys: string[]
}

/** 权限词 → 既有本地化文案 key 的后缀（前缀统一 mobile.plugin.perm.；
 *  terminal:input 已随票 15 阶段 B 整面退役，不在展示词表） */
const PERMISSION_I18N_KEY: Record<string, string> = {
  storage: 'storage',
  'terminal:output': 'terminalOutput',
  'session:read': 'sessionRead',
  'session:write': 'sessionWrite',
  'ui:toolbox': 'uiToolbox',
  'ui:navtab': 'uiNavtab',
  'ui:settings': 'uiSettings',
  'ui:input': 'uiInput',
  'network:http': 'networkHttp',
  'fs:read': 'fsRead',
  'fs:write': 'fsWrite',
  bus: 'bus',
}

/** 能力域分组：与移动端权限词汇的能力域划分一致 */
export const SHELL_PERMISSION_GROUPS: ShellPermissionGroup[] = [
  {
    id: 'terminal',
    titleKey: 'shell.permission.group.terminal',
    keys: ['terminal:output', 'session:read', 'session:write'],
  },
  {
    id: 'data',
    titleKey: 'shell.permission.group.data',
    keys: ['fs:read', 'fs:write', 'network:http'],
  },
  {
    id: 'interface',
    titleKey: 'shell.permission.group.interface',
    keys: ['ui:toolbox', 'ui:navtab', 'ui:settings', 'ui:input', 'bus', 'storage'],
  },
]

/**
 * 默认授予且不可关闭的权限。
 * storage 是插件底座能力（私有库 / KV），关掉等于让应用无法工作，因此不给开关。
 */
export const SHELL_LOCKED_PERMISSIONS: ReadonlySet<string> = new Set(['storage'])

/** 权限标题的 i18n key（未知权限返回 undefined，由调用方回退原始权限词） */
export function permissionTitleKey(key: string): string | undefined {
  const suffix = PERMISSION_I18N_KEY[key]
  return suffix ? `mobile.plugin.perm.${suffix}.title` : undefined
}

/** 权限说明的 i18n key（未知权限返回 undefined） */
export function permissionDescKey(key: string): string | undefined {
  const suffix = PERMISSION_I18N_KEY[key]
  return suffix ? `mobile.plugin.perm.${suffix}.desc` : undefined
}

/** 权限是否默认授予且不可关闭 */
export function isLockedPermission(key: string): boolean {
  return SHELL_LOCKED_PERMISSIONS.has(key)
}

/**
 * 按能力域分组权限授予项
 *
 * 分组外的权限（新增词表尚未归类）归入末尾的「其他」组，保证新权限不会因
 * 未归类而从 UI 上消失——消失比归类难看更危险。
 */
export function groupPermissions(
  grants: ShellPermissionGrant[],
  fallbackTitleKey: string,
): Array<ShellPermissionGroup & { items: ShellPermissionGrant[] }> {
  const grouped = SHELL_PERMISSION_GROUPS.map((g) => ({ ...g, items: [] as ShellPermissionGrant[] }))
  const rest: ShellPermissionGrant[] = []

  for (const grant of grants) {
    const target = grouped.find((g) => g.keys.includes(grant.key))
    if (target) target.items.push(grant)
    else rest.push(grant)
  }

  const nonEmpty = grouped.filter((g) => g.items.length > 0)
  if (rest.length > 0) {
    nonEmpty.push({ id: 'other', titleKey: fallbackTitleKey, keys: [], items: rest })
  }
  return nonEmpty
}
