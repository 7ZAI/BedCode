/**
 * xterm 写入管线（对齐桌面端 TerminalPreview.vue）
 *
 * 把同帧内多次 terminal.write() 合并为一次，避免 WebGL 双缓冲下新旧帧叠加（重影）。
 * 三层保障：
 * - DEC Mode 2026 同步输出：写入前用 BSU/ESU 序列包裹，xterm 缓存所有变化到
 *   下一帧统一渲染（xterm 6.0+ 支持）
 * - rAF 合并：同一渲染帧内的多个输出事件合并为一次 write。为什么不用
 *   queueMicrotask：Tauri 事件每个都是独立 macrotask，微任务会在每个事件后立即
 *   flush，无法跨事件合并；rAF 才能把同帧事件合为一次 write
 * - 兜底定时器：窗口最小化/后台时 rAF 暂停，100ms 定时器保证队列最终被清空
 *
 * 单次 write 上限 MAX_WRITE_CHUNK：超过则拆块，让 xterm parser 在块间让出
 * 主线程，避免单帧解析超大字符串导致 UI 卡顿。
 *
 * 移动端特殊处理：累积阈值 MAX_COALESCED_BYTES —— 极端大块数据下手机 CPU 更弱、
 * rAF 延迟更敏感，超过阈值立即 flush 而非等下一帧。
 */

import type { Terminal } from '@xterm/xterm'

// DEC Mode 2026 同步输出序列：包裹一次写入，渲染器收到 ESU 前不刷新屏幕
const SYNC_OUTPUT_START = new TextEncoder().encode('\x1b[?2026h')
const SYNC_OUTPUT_END = new TextEncoder().encode('\x1b[?2026l')

/** 单次 write 上限：超过则拆块（与桌面端一致） */
const MAX_WRITE_CHUNK = 64 * 1024

/** 累积阈值：超过立即 flush（移动端特殊处理） */
const MAX_COALESCED_BYTES = 256 * 1024

/** rAF 暂停（最小化/后台）时的兜底 flush 延迟 */
const FALLBACK_FLUSH_MS = 100

/**
 * 用 DEC Mode 2026 同步输出序列包裹数据，让 xterm 缓存所有变化到下一帧统一绘制
 */
export function wrapSyncOutput(data: Uint8Array): Uint8Array {
  const wrapped = new Uint8Array(SYNC_OUTPUT_START.length + data.byteLength + SYNC_OUTPUT_END.length)
  wrapped.set(SYNC_OUTPUT_START, 0)
  wrapped.set(data, SYNC_OUTPUT_START.length)
  wrapped.set(SYNC_OUTPUT_END, SYNC_OUTPUT_START.length + data.byteLength)
  return wrapped
}

/** 写入合并器：调用即入队，rAF 时统一 flush */
export interface WriteCoalescer {
  (data: Uint8Array): void
  /** 取消挂起的 rAF/定时器并清空待写入缓冲 */
  dispose(): void
}

export function createWriteCoalescer(terminal: Terminal): WriteCoalescer {
  let pending: Uint8Array[] = []
  let totalBytes = 0
  let flushRaf = 0
  let flushTimer: ReturnType<typeof setTimeout> | null = null

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

    if (combined.byteLength <= MAX_WRITE_CHUNK) {
      terminal.write(wrapSyncOutput(combined))
      return
    }
    // 大块拆分为多次 write（2026 包裹整体）：渲染器仍缓存变更到帧末统一绘制；
    // subarray 零拷贝切片，避免大块复制
    terminal.write(SYNC_OUTPUT_START)
    for (let i = 0; i < combined.length; i += MAX_WRITE_CHUNK) {
      terminal.write(combined.subarray(i, i + MAX_WRITE_CHUNK))
    }
    terminal.write(SYNC_OUTPUT_END)
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
