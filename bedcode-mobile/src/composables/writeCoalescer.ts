/**
 * xterm 写入管线（对齐桌面端 TerminalPreview.vue）
 *
 * 默认直写：每个输出事件直接 terminal.write()（rAF 合并关闭），
 * 避免依赖 rAF 回调的合并路径在页面后台/动画挂起时延迟写入（黑屏/帧滞留）。
 *
 * rAF 合并（调试开关 ENABLE_RAF_COALESCE=true）：把同帧内多次 write() 合并为
 * 一次，降低高频输出时每事件一次渲染的开销——排查渲染挂起/黑屏/帧滞留时
 * 可开启做 A/B 对比。为什么不用 queueMicrotask：Tauri 事件每个都是独立
 * macrotask，微任务会在每个事件后立即 flush，无法跨事件合并；rAF 才能把
 * 同帧事件合为一次 write。合并路径带 100ms 兜底定时器：窗口最小化/后台时
 * rAF 暂停，定时器保证队列最终被清空。
 *
 * DEC 2026 同步输出不再由应用侧包裹：xterm.js 6.0 已内置该协议（解析 BSU/ESU
 * 序列并按帧统一提交渲染），应用侧包裹反而会与 TUI 应用自身的 2026 序列嵌套。
 *
 * 单次 write 上限 MAX_WRITE_CHUNK：超过则拆块，让 xterm parser 在块间让出
 * 主线程，避免单帧解析超大字符串导致 UI 卡顿。
 *
 * 移动端特殊处理：累积阈值 MAX_COALESCED_BYTES —— 极端大块数据下手机 CPU 更弱、
 * rAF 延迟更敏感，超过阈值立即 flush 而非等下一帧。
 */

import type { Terminal } from '@xterm/xterm'

// rAF 合并调试开关（默认关闭）：直写路径下每个输出事件直接 terminal.write，
// 不经 rAF 合并/兜底定时器——排查「渲染挂起/黑屏/帧滞留」时可置 true 恢复
// 合并路径做 A/B 对比（合并依赖 rAF 回调，页面后台/动画挂起时可能延迟写入）
const ENABLE_RAF_COALESCE = false

/** 单次 write 上限：超过则拆块（与桌面端一致） */
const MAX_WRITE_CHUNK = 64 * 1024

/** 累积阈值：超过立即 flush（移动端特殊处理） */
const MAX_COALESCED_BYTES = 256 * 1024

/** rAF 暂停（最小化/后台）时的兜底 flush 延迟 */
const FALLBACK_FLUSH_MS = 100

/** 写入合并器：调用即入队，rAF 时统一 flush */
export interface WriteCoalescer {
  (data: Uint8Array): void
  /** 取消挂起的 rAF/定时器并清空待写入缓冲 */
  dispose(): void
}

/** 创建选项 */
export interface WriteCoalescerOptions {
  /** rAF 合并调试开关（默认 ENABLE_RAF_COALESCE）；测试可显式开启以覆盖合并路径 */
  enableRafCoalesce?: boolean
}

export function createWriteCoalescer(
  terminal: Terminal,
  options: WriteCoalescerOptions = {},
): WriteCoalescer {
  // rAF 合并关闭时每个事件直接写入（调试/对比路径）
  const rafCoalesce = options.enableRafCoalesce ?? ENABLE_RAF_COALESCE
  let pending: Uint8Array[] = []
  let totalBytes = 0
  let flushRaf = 0
  let flushTimer: ReturnType<typeof setTimeout> | null = null

  /** 写入单块字节（拆块，rAF 合并与直写共用） */
  function writeBytes(data: Uint8Array) {
    // terminal 可能已 dispose（页面切换/会话关闭）：与合并路径 flush 的守卫一致
    if (!terminal.element) return
    for (let i = 0; i < data.length; i += MAX_WRITE_CHUNK) {
      terminal.write(data.subarray(i, i + MAX_WRITE_CHUNK))
    }
  }

  function flush() {
    flushRaf = 0
    if (flushTimer) {
      clearTimeout(flushTimer)
      flushTimer = null
    }
    if (pending.length === 0) return
    // terminal 可能已 dispose（页面切换/会话关闭）
    if (!terminal.element) {
      pending = []
      totalBytes = 0
      return
    }

    const chunks = pending
    const bytes = totalBytes
    pending = []
    totalBytes = 0

    // 合并同帧所有事件为单块字节，一次 write
    const combined = new Uint8Array(bytes)
    let offset = 0
    for (const chunk of chunks) {
      combined.set(chunk, offset)
      offset += chunk.byteLength
    }

    writeBytes(combined)
  }

  function scheduleFlush() {
    if (flushRaf) return
    flushRaf = requestAnimationFrame(flush)
    if (!flushTimer) {
      flushTimer = setTimeout(() => {
        flushTimer = null
        if (flushRaf) {
          cancelAnimationFrame(flushRaf)
          flushRaf = 0
        }
        flush()
      }, FALLBACK_FLUSH_MS)
    }
  }

  function write(data: Uint8Array) {
    if (data.length === 0) return

    if (!rafCoalesce) {
      // rAF 合并关闭（调试开关）：每个事件直接写入，不经合并管线
      writeBytes(data)
      return
    }

    pending.push(data)
    totalBytes += data.byteLength

    if (totalBytes >= MAX_COALESCED_BYTES) {
      // 累积过大，立即 flush 避免 rAF 延迟影响响应（移动端特殊处理）
      if (flushRaf) {
        cancelAnimationFrame(flushRaf)
        flushRaf = 0
      }
      flush()
    } else {
      scheduleFlush()
    }
  }

  function dispose() {
    if (flushRaf) {
      cancelAnimationFrame(flushRaf)
      flushRaf = 0
    }
    if (flushTimer) {
      clearTimeout(flushTimer)
      flushTimer = null
    }
    pending = []
    totalBytes = 0
  }

  return Object.assign(write, { dispose }) as WriteCoalescer
}
