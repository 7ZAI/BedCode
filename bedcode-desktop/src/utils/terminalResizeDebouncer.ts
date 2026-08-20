/**
 * 终端 resize 分层防抖（对齐 VS Code TerminalResizeDebouncer 语义）
 *
 * 为什么分层：水平 resize 触发整屏 reflow（昂贵），拖窗时逐帧执行浪费严重，
 * 值得用 100ms 防抖合并；垂直 resize 是行数变化，用户对即时性敏感，必须
 * 立即生效。flush() 提供「最终尺寸必达」保证（防抖窗口内的最后一次尺寸
 * 可被立即应用，不必等计时器到期）。
 *
 * 纯逻辑模块（Seam A）：零 DOM 依赖，仅计时器 + 上次尺寸记录；
 * 实际 fit/重绘/PTY 同步由调用方经 onApply 接线。
 */

export interface TerminalResizeDebouncerOptions {
  /** 实际执行 fit + 重绘 + PTY 同步（调用方接线） */
  onApply: () => void
  /** 水平防抖延迟，默认 100ms（= VS Code DebounceResizeXDelay） */
  horizontalDelayMs?: number
}

/** resize 分层防抖器：高度变化立即应用，仅宽度变化防抖合并 */
export class TerminalResizeDebouncer {
  private readonly onApply: () => void
  private readonly horizontalDelayMs: number
  private lastWidth: number | null = null
  private lastHeight: number | null = null
  private timer: ReturnType<typeof setTimeout> | null = null
  private disposed = false

  constructor(options: TerminalResizeDebouncerOptions) {
    this.onApply = options.onApply
    this.horizontalDelayMs = options.horizontalDelayMs ?? 100
  }

  /**
   * 喂入容器 CSS 尺寸：
   * - 高度变化（含宽高同时变化、首次喂入）→ 立即 onApply，并取消挂起的水平防抖
   * - 仅宽度变化 → 重置防抖计时器（窗口内最后一次生效）
   * - 等值喂入（subpixel 抖动）→ 不触发、不重置计时器
   */
  resize(width: number, height: number): void {
    if (this.disposed) return
    if (width === this.lastWidth && height === this.lastHeight) return
    const heightChanged = this.lastHeight === null || height !== this.lastHeight
    this.lastWidth = width
    this.lastHeight = height
    if (heightChanged) {
      // 高度优先：立即应用；挂起的水平防抖已被本次应用覆盖，取消
      this.clearTimer()
      this.onApply()
      return
    }
    // 仅宽度变化：防抖合并，计时器重置（最后一次尺寸生效）
    this.clearTimer()
    this.timer = setTimeout(() => {
      this.timer = null
      this.onApply()
    }, this.horizontalDelayMs)
  }

  /** 立即应用挂起的防抖尺寸并取消计时器（最终尺寸必达保证）；无挂起则为 no-op */
  flush(): void {
    if (this.disposed || this.timer === null) return
    this.clearTimer()
    this.onApply()
  }

  /** 清理计时器、不触发挂起应用（组件卸载用） */
  dispose(): void {
    this.disposed = true
    this.clearTimer()
  }

  private clearTimer(): void {
    if (this.timer !== null) {
      clearTimeout(this.timer)
      this.timer = null
    }
  }
}
