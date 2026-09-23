/**
 * 权限词汇前端锁（票 01）
 *
 * 前端权限面是插件调用宿主 API 的快速失败层，它读的是 SDK 生成物；本文件断言的是
 * 「前端这份消费面真的覆盖了插件前端实际会调的每一个 API 名」，而不是复述生成物内容：
 * - L1 `context.ts` 里每一个 `requirePermission(x)` 都能被唯一权限门住，且只有授予该
 *   权限时通过（历史上 `ui.registerPage` 只存在于前端手抄表、SDK 侧没有，
 *   宿主授权时被静默过滤 → 声明了等于没声明）
 * - L2 前端生成物与打包 CLI 生成物集合相等（两份都是生成物，手改任一份即转红）
 * - L3 词汇表外的字符串一律不合法；空权限清单不放行任何 API
 */

import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'
import { hasPermissionForApi, isValidPermission } from '@/plugin/permission'
import {
  GENERATED_PERMISSION_API_MAP,
  GENERATED_VALID_PERMISSIONS,
} from '@/plugin/permission-vocabulary'

/** 从源码里抓出前端实际门住的 API 方法名 */
function requiredApiNames(file: string): string[] {
  const source = readFileSync(file, 'utf-8')
  const names = new Set<string>()
  for (const match of source.matchAll(/requirePermission\('([^']+)'\)/g)) {
    names.add(match[1])
  }
  return [...names].sort()
}

function permissionsForApi(api: string): string[] {
  return GENERATED_VALID_PERMISSIONS.filter((perm) =>
    (GENERATED_PERMISSION_API_MAP[perm] ?? []).includes(api),
  )
}

describe('权限词汇前端锁', () => {
  it('L1 context.ts 的每个 requirePermission 调用点都有唯一权限位门住', () => {
    const apiNames = requiredApiNames('src/plugin/context.ts')
    // 防「扫描空转 = 全绿」：前端上下文的门禁点数量有基线
    expect(apiNames.length).toBeGreaterThanOrEqual(20)

    for (const api of apiNames) {
      const owners = permissionsForApi(api)
      expect(owners, `API ${api} 在词汇表里没有任何权限门`).toHaveLength(1)
      const [perm] = owners
      expect(GENERATED_VALID_PERMISSIONS).toContain(perm)

      // 行为面：授予该权限才放行，未授予与授予他权限都拒绝
      expect(hasPermissionForApi([perm], api), `授予 ${perm} 后应允许 ${api}`).toBe(true)
      expect(hasPermissionForApi([], api), `空权限清单不得允许 ${api}`).toBe(false)
      const others = GENERATED_VALID_PERMISSIONS.filter((p) => p !== perm)
      expect(hasPermissionForApi(others, api), `他权限不得代为允许 ${api}`).toBe(false)
    }
  })

  it('L1b 注册页面贡献仍归 ui:sidebar（合并生成物时最易丢的一条）', () => {
    expect(hasPermissionForApi(['ui:sidebar'], 'ui.registerPage')).toBe(true)
    expect(hasPermissionForApi(['ui:toolbox'], 'ui.registerPage')).toBe(false)
    expect(permissionsForApi('ui.registerPage')).toEqual(['ui:sidebar'])
  })

  it('L2 前端生成物与打包 CLI 生成物是同一张表', () => {
    const cli = JSON.parse(
      readFileSync('packages/plugin-sdk-desktop/bin/permission-vocabulary.json', 'utf-8'),
    )
    expect([...GENERATED_VALID_PERMISSIONS].sort()).toEqual([...cli.permissions].sort())
    const tsMap = Object.fromEntries(
      Object.entries(GENERATED_PERMISSION_API_MAP).map(([k, v]) => [k, [...v]]),
    )
    expect(tsMap).toEqual(cli.apiMap)
  })

  it('L3 词汇表外的声明不合法，API 映射不认陌生方法', () => {
    expect(isValidPermission('terminal:input')).toBe(true)
    expect(isValidPermission('fileservice')).toBe(false)
    expect(isValidPermission('transfer')).toBe(false)
    // 本轮清零的生产装饰词汇（file-transfer 曾声明、宿主从不认）
    for (const dead of ['fileservice', 'transfer', 'bus']) {
      expect(isValidPermission(dead), `${dead} 不应留在桌面词汇表`).toBe(false)
    }
    expect(hasPermissionForApi(['storage'], 'storage.nonexistent')).toBe(false)
  })

  it('L4 前端权限面不再自带权限清单（只读生成物）', () => {
    const source = readFileSync('src/plugin/permission.ts', 'utf-8')
    expect(source).toContain('./permission-vocabulary')
    for (const perm of GENERATED_VALID_PERMISSIONS) {
      expect(source, `permission.ts 仍手抄权限 ${perm}`).not.toContain(`'${perm}'`)
    }
  })
})
