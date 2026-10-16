/**
 * taskReason 纯函数测试（ADR 0030 收口，2026-09-28）
 *
 * 锁三条：
 * ① reason 未知码时**永不渲染原文**（曾 `|| task.reason` 直出 wire detail）；
 * ② wire 枚举 PascalCase（UserRejected/Timeout/…）与历史 kebab 小写双命中；
 * ③ 空 reason / 未知码 → 状态兜底文案。
 */
import { describe, expect, it } from 'vitest'
import { rejectReasonText, reasonText, historyReason } from '../utils/taskReason'

/** mock 翻译器：返回 `[key]` 便于断言命中了哪个 i18n key */
const t = (key: string) => `[${key}]`

describe('rejectReasonText', () => {
  it('wire PascalCase 枚举命中对应 i18n', () => {
    expect(rejectReasonText('UserRejected', t)).toBe('[transfer.error.rejectedByUser]')
    expect(rejectReasonText('Timeout', t)).toBe('[transfer.error.noResponse]')
    expect(rejectReasonText('PolicyDenied', t)).toBe('[transfer.error.policyDenied]')
    expect(rejectReasonText('DuplicateName', t)).toBe('[transfer.error.duplicateName]')
  })

  it('历史 kebab 小写兼容', () => {
    expect(rejectReasonText('user-rejected', t)).toBe('[transfer.error.rejectedByUser]')
    expect(rejectReasonText('timeout', t)).toBe('[transfer.error.noResponse]')
  })

  it('未知码 / 空返回空串（调用方落兜底，不渲染原文）', () => {
    expect(rejectReasonText('peer went away mid-transfer', t)).toBe('')
    expect(rejectReasonText(null, t)).toBe('')
    expect(rejectReasonText(undefined, t)).toBe('')
    expect(rejectReasonText('', t)).toBe('')
  })
})

describe('reasonText', () => {
  it('rejected + 未知码 → 状态兜底，不渲染原文', () => {
    // 变异守卫：把兜底改成 `|| reason`（直出原文）即红
    expect(reasonText('rejected', 'UserRejected', t)).toBe('[transfer.error.rejectedByUser]')
    expect(reasonText('rejected', 'PeerBusy', t)).toBe('[transfer.task.state.rejected]')
  })

  it('failed + 未知码 → failed 兜底，不渲染原文（防泄漏主锁）', () => {
    expect(reasonText('failed', 'disk io error while writing', t)).toBe(
      '[transfer.task.state.failed]',
    )
    expect(reasonText('failed', null, t)).toBe('[transfer.task.state.failed]')
  })

  it('其它状态 → 空串', () => {
    expect(reasonText('transferring', null, t)).toBe('')
    expect(reasonText('completed', null, t)).toBe('')
    expect(reasonText('cancelled', 'whatever', t)).toBe('')
  })
})

describe('historyReason', () => {
  it('failed + 未知码 → failed 兜底，不渲染原文（防泄漏主锁）', () => {
    expect(historyReason('failed', 'peer disconnected (code 7)', t)).toBe(
      '[transfer.task.state.failed]',
    )
  })

  it('rejected 已知码 → 具体文案；未知码 → 状态兜底', () => {
    expect(historyReason('rejected', 'Timeout', t)).toBe('[transfer.error.noResponse]')
    expect(historyReason('rejected', 'WeirdReason', t)).toBe('[transfer.task.state.rejected]')
  })

  it('非失败/拒绝状态 → 空串', () => {
    expect(historyReason('completed', 'x', t)).toBe('')
    expect(historyReason('cancelled', 'x', t)).toBe('')
  })
})
