import { describe, it, expect } from 'vitest'
import { GENERATED_VALID_PERMISSIONS } from '@/plugin/permission.vocabulary'
import {
  getPermissionMeta,
  isHighRiskPermission,
  HIGH_RISK_PERMISSIONS,
} from '@/plugin/contributionKinds'

/**
 * 权限展示文案覆盖测试（审计票 03 前置子项）
 *
 * 审批弹层会把 manifest 声明的权限逐条展示给用户；词汇表里任何一条没有文案，
 * 弹层就会显示成「未知权限」——用户看不明权限含义却必须批准。因此断言：
 * - 词汇表全条目都有文案（title 不回退为权限串、desc 不回退为 unknown 文案）
 * - 高危清单与后果文案一一对应（高危有、非高危无）
 * - 未知权限走回退（不抛错、不显示空文案）
 */

/** 未注册权限的回退文案（作为「是否已覆盖」的判据基线） */
const UNKNOWN_DESC = getPermissionMeta('__not_a_permission__').desc

describe('权限展示文案覆盖', () => {
  it('词汇表每一条权限都有文案（不得回退为未知权限）', () => {
    // 词汇表非空：防「列表读空 → 下面的断言恒真」的退化实现
    expect(GENERATED_VALID_PERMISSIONS.length).toBeGreaterThan(0)

    const missing = GENERATED_VALID_PERMISSIONS.filter((perm) => {
      const meta = getPermissionMeta(perm)
      return meta.desc === UNKNOWN_DESC || meta.title === perm
    })
    expect(missing, `以下权限缺少 zh-CN 文案: ${missing.join(', ')}`).toEqual([])
  })

  it('高危位有后果文案，非高危位没有（清单与文案一一对应）', () => {
    expect(HIGH_RISK_PERMISSIONS.length).toBeGreaterThan(0)

    for (const perm of HIGH_RISK_PERMISSIONS) {
      expect(getPermissionMeta(perm).risk, `${perm} 必须有后果文案`).toBeTruthy()
      expect(isHighRiskPermission(perm), `${perm} 必须命中高危判定`).toBe(true)
    }

    const leaked = GENERATED_VALID_PERMISSIONS.filter(
      (perm) => !HIGH_RISK_PERMISSIONS.includes(perm) && getPermissionMeta(perm).risk,
    )
    expect(leaked, `非高危位不得带后果文案: ${leaked.join(', ')}`).toEqual([])
    expect(isHighRiskPermission('storage')).toBe(false)
  })

  it('未知权限回退为原文 + 未知文案（不抛错、不留空）', () => {
    const meta = getPermissionMeta('com.example:unknown')
    expect(meta.title).toBe('com.example:unknown')
    expect(meta.desc).toBe(UNKNOWN_DESC)
    expect(meta.desc).not.toBe('')
    expect(isHighRiskPermission('com.example:unknown')).toBe(false)
  })
})
