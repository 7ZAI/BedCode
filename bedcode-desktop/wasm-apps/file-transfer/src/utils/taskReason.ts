/**
 * 传输终态原因文案（ADR 0030 收口，2026-09-28）
 *
 * 历史缺陷：`useTasks.mapWireTask` 曾把 wire `detail` 并进 `reason`（detail 优先），
 * TaskPanel 对未知 reason 码兜底直出原文 → wire 技术串会上用户面（审计发现）。
 * 收口两条：
 * ① `reason` 只承载 `rejectReason`（wire 枚举，见 transfer_store.rs
 *    `terminal_status_of`：rejected 的 reason 原样透传，如 `UserRejected`）；
 * ② 未知码一律落 i18n 状态兜底，**原文永不渲染**。
 *
 * 映射表大小写双列：wire 为 PascalCase（`UserRejected` / `Timeout` 等），旧前端
 * 曾用 kebab 小写（`user-rejected`…）——保留兼容，避免两处口径漂移。
 */
import type { TaskStateName } from '../types'

/** 翻译器（注入方用插件 context.i18n.t，自动加插件 ID 前缀） */
export type Translator = (key: string, params?: Record<string, unknown>) => string

/** wire 拒绝原因枚举 → 友好文案 key（kebab 小写为历史兼容，真源 = PascalCase） */
const REJECT_REASON_KEYS: Record<string, string> = {
  'user-rejected': 'transfer.error.rejectedByUser',
  UserRejected: 'transfer.error.rejectedByUser',
  timeout: 'transfer.error.noResponse',
  Timeout: 'transfer.error.noResponse',
  'policy-denied': 'transfer.error.policyDenied',
  PolicyDenied: 'transfer.error.policyDenied',
  'duplicate-name': 'transfer.error.duplicateName',
  DuplicateName: 'transfer.error.duplicateName',
}

/** 终态原因 → 友好文案；未知码 / 空 → ''（调用方落状态兜底，**不渲染原文**） */
export function rejectReasonText(reason: string | null | undefined, t: Translator): string {
  if (!reason) return ''
  const key = REJECT_REASON_KEYS[reason]
  return key ? t(key) : ''
}

/**
 * 活跃任务终态原因文案（rejected / failed）。
 * 未知码 → 状态兜底（`transfer.task.state.rejected` / `...failed`），原文永不渲染。
 */
export function reasonText(state: TaskStateName, reason: string | null | undefined, t: Translator): string {
  const mapped = rejectReasonText(reason, t)
  if (mapped) return mapped
  if (state === 'rejected') return t('transfer.task.state.rejected')
  if (state === 'failed') return t('transfer.task.state.failed')
  return ''
}

/**
 * 历史条目原因文案（仅失败/拒绝时显示）。
 * 未知码 → 状态兜底（与 reasonText 同规则，不渲染原文）。
 */
export function historyReason(
  state: TaskStateName,
  reason: string | null | undefined,
  t: Translator,
): string {
  const mapped = rejectReasonText(reason, t)
  if (mapped) return mapped
  if (state === 'failed' || state === 'rejected') {
    return t(`transfer.task.state.${state}`)
  }
  return ''
}
