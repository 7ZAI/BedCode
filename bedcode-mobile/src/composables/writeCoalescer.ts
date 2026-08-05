/**
 * xterm 写入合并器
 *
 * 把同帧内多次 terminal.write() 合并为一次，避免 WebGL 双缓冲下新旧帧叠加（重影）。
 * 等价于 xterm 6.0 DEC Mode 2026 (Synchronized Output) 同步输出语义。
 */

import type { Terminal } from '@xterm/xterm'

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

    terminal.write(combined)
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
