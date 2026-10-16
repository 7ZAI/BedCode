/**
 * 终端重连倒计时（M-02，2026-10-04 OCR）
 *
 * 纯函数：倒计时应从「排期到达时刻」起递减，而不是每秒重读同一个静态毫秒数。
 * Rust 只在每轮排期时发一次 `reconnect_scheduled`（retry_in_ms + 到达时刻由
 * store 记下），本函数把「剩余毫秒」换算成显示秒数：
 *
 * - 未排期 / 已恢复 / 已过期 → null（显示「正在重连…」而不是编造「0 秒后」）
 * - 未过期 → 向上取整到秒（1s 粒度足够，退避以秒计），至少显示 1
 */

/** 剩余毫秒：从到达时刻起减去已流逝时间；过期后钳到 0（不出现负数） */
export function reconnectRemainingMs(
  reconnectInMs: number,
  receivedAt: number,
  now: number,
): number {
  if (!Number.isFinite(reconnectInMs) || reconnectInMs <= 0) return 0
  return Math.max(0, reconnectInMs - Math.max(0, now - receivedAt))
}

/** 显示秒数：null = 不显示数字（未排期 / 已恢复 / 已过期），否则 ≥1 秒 */
export function reconnectDisplaySeconds(
  reconnectInMs: number | null | undefined,
  receivedAt: number | null | undefined,
  now: number,
): number | null {
  if (reconnectInMs == null || receivedAt == null) return null
  const remaining = reconnectRemainingMs(reconnectInMs, receivedAt, now)
  if (remaining <= 0) return null // 已过排期：不显示「0 秒后」，等下一轮排期
  return Math.max(1, Math.ceil(remaining / 1000))
}