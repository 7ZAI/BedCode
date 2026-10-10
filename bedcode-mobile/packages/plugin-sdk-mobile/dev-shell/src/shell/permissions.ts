/**
 * Dev Shell 权限展示元数据（与宿主 shell/permissions.ts 同构）
 * -----------------------------------------------------------------------------
 * 权限词汇的真源在插件 SDK（packages/plugin-sdk-mobile/rust/src/permission.rs），
 * 本文件不复制词表，只做展示层两件事：
 *   1. 把权限词映射成 i18n 文案 key（dev-shell 自有 `shell.permission.*` 命名空间）
 *   2. 按能力域分组，让「应用详情 → 权限」是可读清单而不是一串冒号字符串
 *
 * 真源裁决提醒：宿主壳的分组表（bedcode-mobile/src/shell/permissions.ts）才是
 * 面向用户的口径；本表与它保持同构，dev-shell 上看到的分组应与真机一致。
 */

import type { ShellPermissionGrant } from './types'

/** 权限分组定义（展示顺序即数组顺序） */
export interface ShellPermissionGroup {
  id: string
  /** 分组标题的 i18n key */
  titleKey: string
  keys: string[]
}

/** 权限词 → 本地化文案 key 的后缀（前缀统一 shell.permission.perm.） */
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
 * 默认授予且不可关闭的权限
 *
 * storage 是插件底座能力（私有库 / KV），关掉等于让应用无法工作，因此不给开关。
 */
export const SHELL_LOCKED_PERMISSIONS: ReadonlySet<string> = new Set(['storage'])

/** 权限标题的 i18n key（未知权限返回 undefined，由调用方回退原始权限词） */
export function permissionTitleKey(key: string): string | undefined {
  const suffix = PERMISSION_I18N_KEY[key]
  return suffix ? `shell.permission.perm.${suffix}.title` : undefined
}

/** 权限是否默认授予且不可关闭 */
export function isLockedPermission(key: string): boolean {
  return SHELL_LOCKED_PERMISSIONS.has(key)
}

/**
 * 按能力域分组权限授予项
 *
 * 分组外的权限（新增词表尚未归类）归入末尾的「其他」组，保证新权限不会因未归类
 * 而从 UI 上消失——消失比归类难看更危险。
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