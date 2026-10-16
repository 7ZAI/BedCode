/**
 * 终端滚动域（TerminalPreview 拆分产物）
 *
 * 滚动状态追踪（"是否在底部"）、rAF 合并的滚动到底、清屏、刷新格式。
 * onScroll 监听由 initTerminal 创建后经 setScrollDisposable 注入（保持创建
 * 时点与 xterm 生命周期一致），卸载时 disposeScroll 统一清理。
 */
import type { TerminalKernelContext } from './terminalKernel'
import type { IDisposable } from '@xterm/xterm'

export function useTerminalScroll(ctx: TerminalKernelContext) {
  // rAF 节流：同一帧内多次 scrollToBottom 调用只执行一次
  let pendingScrollRaf = 0
  // xterm onScroll 取消监听（IDisposable 接口）
  let scrollDisposable: IDisposable | null = null

  /** 滚动到底（rAF 合并：同一帧内多次输出只滚动一次） */
  function scrollToBottom() {
    if (!pendingScrollRaf) {
      pendingScrollRaf = requestAnimationFrame(() => {
        pendingScrollRaf = 0
        ctx.terminalRef.value?.scrollToBottom()
      })
    }
  }

  /** 用户点击"回到底部"按钮：重置滚动状态并滚到底 */
  function scrollToBottomManual() {
    ctx.isUserScrolling.value = false
    ctx.terminalRef.value?.scrollToBottom()
  }

  function clearTerminal() {
    ctx.terminalRef.value?.clear()
  }

  /** 刷新格式（等价用户点击刷新）：全量重绘 + 重算网格（渲染器 cell 尺寸漂移修正） */
  function refreshTerminal() {
    const terminal = ctx.terminalRef.value
    const session = ctx.getSession()
    if (!terminal || !session) return
    terminal.refresh(0, terminal.rows - 1)
    ctx.callbacks.syncTerminalSize()
  }

  /** initTerminal 创建 onScroll 监听后注入，卸载时 disposeScroll 统一清理 */
  function setScrollDisposable(disposable: IDisposable | null) {
    scrollDisposable = disposable
  }

  function disposeScroll() {
    if (scrollDisposable) {
      scrollDisposable.dispose()
      scrollDisposable = null
    }
    if (pendingScrollRaf) {
      cancelAnimationFrame(pendingScrollRaf)
      pendingScrollRaf = 0
    }
  }

  return {
    isUserScrolling: ctx.isUserScrolling,
    scrollToBottom,
    scrollToBottomManual,
    clearTerminal,
    refreshTerminal,
    setScrollDisposable,
    disposeScroll,
  }
}
