/**
 * 宿主壳权限元数据 行为契约测试
 *
 * 覆盖：能力域分组、未归类权限兜底、空组剔除、文案 key 映射、锁定权限。
 */
import { describe, it, expect } from 'vitest'
import {
  groupPermissions,
  isLockedPermission,
  permissionDescKey,
  permissionTitleKey,
  SHELL_PERMISSION_GROUPS,
} from '@/shell/permissions'
import type { ShellPermissionGrant } from '@/shell/types'

function grant(key: string, granted = true): ShellPermissionGrant {
  return { key, granted }
}

describe('权限文案映射', () => {
  it('should_mapToExistingI18nKey_when_permissionKnown', () => {
    expect(permissionTitleKey('terminal:output')).toBe('mobile.plugin.perm.terminalOutput.title')
    expect(permissionDescKey('fs:write')).toBe('mobile.plugin.perm.fsWrite.desc')
  })

  it('should_notMapRetiredTerminalInput_when_permissionRetired', () => {
    // 票 15 阶段 B：terminal:input 权限位整面退役，文案映射一并移除
    expect(permissionTitleKey('terminal:input')).toBeUndefined()
    expect(permissionDescKey('terminal:input')).toBeUndefined()
  })

  it('should_returnUndefined_when_permissionUnknown', () => {
    // 未知权限返回 undefined，由 UI 回退原始权限词——回退成空白会让新权限凭空消失
    expect(permissionTitleKey('does:not:exist')).toBeUndefined()
    expect(permissionDescKey('does:not:exist')).toBeUndefined()
  })
})

describe('锁定权限', () => {
  it('should_lockStorageOnly_when_checked', () => {
    expect(isLockedPermission('storage')).toBe(true)
    expect(isLockedPermission('fs:write')).toBe(false)
  })
})

describe('权限分组', () => {
  it('should_groupByCapabilityDomain_when_permissionsMixed', () => {
    const groups = groupPermissions(
      [grant('terminal:output'), grant('fs:read'), grant('bus'), grant('session:write')],
      'shell.permission.group.other',
    )

    expect(groups.map((g) => g.id)).toEqual(['terminal', 'data', 'interface'])
    expect(groups[0].items.map((i) => i.key)).toEqual(['terminal:output', 'session:write'])
    expect(groups[1].items.map((i) => i.key)).toEqual(['fs:read'])
    expect(groups[2].items.map((i) => i.key)).toEqual(['bus'])
  })

  it('should_putUnknownPermissionIntoOtherGroup_when_notCategorized', () => {
    const groups = groupPermissions([grant('quantum:entangle')], 'shell.permission.group.other')

    // 新权限词表未归类时进「其他」，绝不因未归类而从 UI 上消失
    expect(groups).toHaveLength(1)
    expect(groups[0].id).toBe('other')
    expect(groups[0].items.map((i) => i.key)).toEqual(['quantum:entangle'])
  })

  it('should_skipEmptyGroups_when_noPermissionInDomain', () => {
    const groups = groupPermissions([grant('terminal:output')], 'shell.permission.group.other')

    // 空分组不渲染：只申请了终端权限时，不该出现空的「文件与网络」标题
    expect(groups.map((g) => g.id)).toEqual(['terminal'])
  })

  it('should_returnEmptyList_when_noPermissions', () => {
    expect(groupPermissions([], 'shell.permission.group.other')).toEqual([])
  })

  it('should_coverEveryDeclaredPermissionKey_when_groupDefinitionsComplete', () => {
    // 守卫：新增权限词若忘了归类，会静默掉进「其他」——这里把已归类集合显式钉住，
    // 便于评审时发现「新词未归类」而不是上线后才发现
    const covered = new Set(SHELL_PERMISSION_GROUPS.flatMap((g) => g.keys))
    expect([...covered].sort()).toEqual(
      [
        'bus',
        'fs:read',
        'fs:write',
        'network:http',
        'session:read',
        'session:write',
        'storage',
        'terminal:output',
        'ui:input',
        'ui:navtab',
        'ui:settings',
        'ui:toolbox',
      ].sort(),
    )
  })
})
