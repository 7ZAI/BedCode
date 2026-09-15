/**
 * 连接错误分类
 *
 * DevicesView 在 startConnection / connectFromScanResult 两处对连接错误做
 * 相同的字符串分类（timeout / refused / unreachable → 对应 toast key）。
 * 抽为纯函数以便单测锁定分类契约，避免两处 if/else 漂移。
 */

/** 错误分类结果：timeout / refused / unreachable / other */
export type ConnectionErrorKind = 'timeout' | 'refused' | 'unreachable' | 'other'

/**
 * 按错误消息分类连接错误（兼容中文「超时」；大小写敏感，与原 DevicesView 行为一致）。
 * 分类顺序与 DevicesView 原有 if/else 完全一致：
 * 1. timeout / 超时 → timeout
 * 2. refused / rejected → refused
 * 3. unreachable / network → unreachable
 * 4. 其余 → other
 */
export function classifyConnectionError(errorMsg: string): ConnectionErrorKind {
  const msg = String(errorMsg)
  if (msg.includes('timeout') || msg.includes('超时')) return 'timeout'
  if (msg.includes('refused') || msg.includes('rejected')) return 'refused'
  if (msg.includes('unreachable') || msg.includes('network')) return 'unreachable'
  return 'other'
}
