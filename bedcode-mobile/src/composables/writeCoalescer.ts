/**
 * xterm 写入合并器
 *
 * 把同帧内多次 terminal.write() 合并为一次，避免 WebGL 双缓冲下新旧帧叠加（重影）。
 * 写入前用 DEC Mode 2026 (Synchronized Output) 序列包裹，让 xterm 缓存所有变化到
 * 下一帧统一渲染，消除 WebGL 渲染器逐块绘制产生的视觉撕裂/重影（xterm 6.0+ 支持）。
 */

import type { Terminal } from '@xterm/xterm'

// DEC Mode 2026 同步输出序列：包裹一次写入，渲染器收到 ESU 前不刷新屏幕
const SYNC_OUTPUT_START = new TextEncoder().encode('\x1b[?2026h')
const SYNC_OUTPUT_END = new TextEncoder().encode('\x1b[?2026l')

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

/** 累积字节超过此值时立即 flush，防止极端大块数据下 rAF 延迟影响响应 */
const MAX_COALESCED_BYTES = 256 * 1024

/** 写入合并器：调用即入队，rAF 时统一 flush */
export interface WriteCoalescer {
  (data: Uint8Array): void
  /** 取消挂起的 rAF 并清空待写入缓冲 */
  dispose(): void
}

export function createWriteCoalescer(terminal: Terminal): WriteCoalescer {
  let pending: Uint8Array[] = []
  let totalBytes = 0
  let rafId = 0

  function flush() {
    rafId = 0
    if (pending.length === 0) return
    // terminal 可能已 dispose（页面切换/会话关闭）
    if (!terminal.element) {
      pending = []
      totalBytes = 0
      return
    }

    let combined: Uint8Array
    if (pending.length === 1) {
      combined = pending[0]
    } else {
      combined = new Uint8Array(totalBytes)
      let offset = 0
      for (const chunk of pending) {
        combined.set(chunk, offset)
        offset += chunk.byteLength
      }
    }
    pending = []
    totalBytes = 0

    terminal.write(wrapSyncOutput(combined))
  }

  function write(data: Uint8Array) {
    pending.push(data)
    totalBytes += data.byteLength

    if (totalBytes >= MAX_COALESCED_BYTES) {
      // 累积过大，立即 flush 避免 rAF 延迟影响响应
      if (rafId) {
        cancelAnimationFrame(rafId)
        rafId = 0
      }
      flush()
    } else if (!rafId) {
      rafId = requestAnimationFrame(flush)
    }
  }

  function dispose() {
    if (rafId) {
      cancelAnimationFrame(rafId)
      rafId = 0
    }
    pending = []
    totalBytes = 0
  }

  return Object.assign(write, { dispose }) as WriteCoalescer
}
