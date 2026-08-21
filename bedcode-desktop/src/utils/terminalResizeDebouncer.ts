/**
 * 终端 resize 分层防抖（对齐 VS Code TerminalResizeDebouncer 语义）
 *
 * 为什么分层：水平 resize 触发整屏 reflow（昂贵），拖窗时逐帧执行浪费严重，
 * 值得用 100ms 防抖合并；垂直 resize 是行数变化，用户对即时性敏感，必须
 * 立即生效。flush() 提供「最终尺寸必达」保证（防抖窗口内的最后一次尺寸
 * 可被立即应用，不必等计时器到期）。
 *
 * 两个对齐 VS Code 的补充分支：
 * - 小缓冲阈值（StartDebouncingThreshold=200）：缓冲行数低于阈值时不做防抖，
 *   立即应用 —— 新终端/输出少的 shell reflow 便宜，防抖纯属浪费，且避免
 *   「终端已显示但网格滞后 100ms」的感知；
 * - 0/非法尺寸保护：窗口最小化 / display:none 时 ResizeObserver 报 0 尺寸，
 *   直接忽略（对齐 VS Code layout() 的 width<=0/height<=0 直接 return），
 *   避免把 PTY resize 成 1×1 打乱 shell、恢复时再拉回的无谓 reflow。
 *
 * 纯逻辑模块（Seam A）：零 DOM 依赖，仅计时器 + 上次尺寸记录；
 * 实际 fit/重绘/PTY 同步由调用方经 onApply 接线。
 */

const enum Constants {
  /**
   * The _normal_ buffer length threshold at which point resizing starts being debounced.
   */
  StartDebouncingThreshold = 200,
}

export interface TerminalResizeDebouncerOptions {
  /** 实际执行 fit + 重绘 + PTY 同步（调用方接线） */
  onApply: () => void
  /** 水平防抖延迟，默认 100ms（= VS Code DebounceResizeXDelay） */
  horizontalDelayMs?: number
  /**
   * 返回 xterm 当前 normal buffer 行数（小缓冲免防抖判定的输入）。
   * 返回 null/undefined（terminal 未就绪/已销毁）时保守按防抖处理，
   * 等下一次尺寸喂入时重估。
   */
  getBufferLength?: () => number | null
}

/** resize 分层防抖器：高度变化立即应用，仅宽度变化防抖合并 */
export class TerminalResizeDebouncer {
  private readonly onApply: () => void
  private readonly horizontalDelayMs: number
  private readonly getBufferLength: (() => number | null) | undefined
  private lastWidth: number | null = null
  private lastHeight: number | null = null
  private timer: ReturnType<typeof setTimeout> | null = null
  private disposed = false

  constructor(options: TerminalResizeDebouncerOptions) {
    this.onApply = options.onApply
    this.horizontalDelayMs = options.horizontalDelayMs ?? 100
    this.getBufferLength = options.getBufferLength
  }

  /**
   * 喂入容器 CSS 尺寸：
   * - 非法尺寸（≤0 / NaN，最小化或 display:none）→ 忽略，不记录不触发
   * - 小缓冲（< StartDebouncingThreshold）→ 立即 onApply（含纯宽度变化）
   * - 高度变化（含宽高同时变化、首次喂入）→ 立即 onApply，并取消挂起的水平防抖
   * - 仅宽度变化 → 重置防抖计时器（窗口内最后一次生效）
   * - 等值喂入（subpixel 抖动）→ 不触发、不重置计时器
   */
  resize(width: number, height: number): void {
    if (this.disposed) return
    if (!isFinite(width) || !isFinite(height) || width <= 0 || height <= 0) {
      return
    }
    if (width === this.lastWidth && height === this.lastHeight) return
    // 注意：先算高度变化再记录（heightChanged 必须基于旧值判定）
    const heightChanged = this.lastHeight === null || height !== this.lastHeight
    this.lastWidth = width
    this.lastHeight = height

    // 小缓冲立即应用：新终端 reflow 便宜，逐帧即时生效胜过防抖合并。
    // bufferLength 不可知（terminal 未就绪）时保守走常规分层路径。
    // 位于高度变化分支之前：小缓冲时连纯宽度变化也立即应用
    const bufferLength = this.getBufferLength?.()
    if (
      bufferLength !== undefined &&
      bufferLength !== null &&
      bufferLength < Constants.StartDebouncingThreshold
    ) {
      this.clearTimer()
      this.onApply()
      return
    }

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