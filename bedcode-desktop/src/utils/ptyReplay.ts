/**
 * PTY 输出回放去重
 *
 * 终端窗口打开时，同一会话存在两条数据通道，共享 Rust 端单调递增的 index：
 * 1. 实时流（pty-output-{sessionId} Tauri 事件）
 * 2. 历史回放（get_session_output_history）
 *
 * 若会话在历史快照返回前持续输出，实时事件会先于回放到达。两条通道必须共用
 * 同一个"已写入终端的水位"（watermark），否则重叠事件会被写入终端两次，
 * 产生重复行（如全屏 TUI 的图标重复显示）。
 */

export interface SeqEvent {
  index: number
}

/**
 * 从历史事件中筛出尚未写入终端的事件（index > watermark），并返回推进后的水位。
 *
 * 调用方必须保证 watermark 已包含实时流写入的最大 index（实时路径在实际写入
 * 终端时推进水位），否则与之重叠的历史事件会被再次写入。
 *
 * @param events 历史事件，需按 index 升序排列（Rust 端按队列顺序返回）
 */
export function pendingReplayEvents<T extends SeqEvent>(
  events: T[],
  watermark: number,
): { events: T[]; nextWatermark: number } {
  let w = watermark
  const pending: T[] = []
  for (const ev of events) {
    if (ev.index > w) {
      pending.push(ev)
      w = ev.index
    }
  }
  return { events: pending, nextWatermark: w }
}

/** 推进水位（单调不减，避免乱序事件导致倒退） */
export function advanceWatermark(watermark: number, index: number): number {
  return Math.max(watermark, index)
}
