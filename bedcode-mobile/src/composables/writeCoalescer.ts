/**
 * xterm 写入管线（对齐桌面端 TerminalPreview.vue）
 *
 * 默认 rAF 合并：同一帧内到达的多次 write() 合并为一次 terminal.write。
 *
 * 为什么必须合并（移动端 TUI 滚动残影根因，取证实验 2026-09-07）：
 * TUI 应用（opencode 等）一次滚轮重绘被拆成多个 WS 消息，若逐事件直写，
 * xterm 按 write 边界多次提交渲染——同一逻辑屏幕更新的多个批次在不同帧
 * 渲染，canvas 位图残留旧帧像素：滚动停止后不消失、refresh(0, rows-1)
 * 只能清主体剩边缘残字、transform 往返无效（排除合成器滞留）；
 * 开启 rAF 合并后残影 100% 消除。桌面端始终合并，移动端直写在 TUI 高频
 * 重绘下必现残影，故移动端默认同样合并。
 *
 * 为什么不用 queueMicrotask：Tauri 事件每个都是独立 macrotask，微任务会在
 * 每个事件后立即 flush，无法跨事件合并；rAF 才能把同帧事件合为一次 write。
 *
 * 兜底：合并路径带 100ms 定时器（FALLBACK_FLUSH_MS）——窗口最小化/后台时
 * rAF 暂停，定时器保证队列最终被清空，不产生写入延迟/黑屏。
 *
 * 调试：置 ENABLE_RAF_COALESCE=false 可回退逐事件直写做 A/B 对比
 * （排查渲染挂起/黑屏/帧滞留时可反向验证合并路径影响）。
 *
 * DEC 2026 同步输出不再由应用侧包裹：xterm.js 6.0 已内置该协议（解析 BSU/ESU
 * 序列并按帧统一提交渲染），应用侧包裹反而会与 TUI 应用自身的 2026 序列嵌套。
 *
 * 单次 write 上限 MAX_WRITE_CHUNK：超过则拆块（writeInChunks），让 xterm
 * parser 在块间让出主线程，避免单帧解析超大字符串导致 UI 卡顿。
 *
 * 主线程让出 WRITE_YIELD_THRESHOLD：分片写入每累积该量即退让一次宏任务
 * （对齐桌面端 TerminalPreview.writeInChunks）——输出风暴期间允许触摸/渲染
 * 插入，防 UI 冻结。移动端 CPU 更弱，阈值取桌面端（256KB）一半，让出更频繁；
 * 只控制写节奏，不限制总量（flush 内 while 轮次继续消费新入队数据）。
 *
 * 移动端特殊处理：累积阈值 MAX_COALESCED_BYTES —— 极端大块数据下手机 CPU 更弱、
 * rAF 延迟更敏感，超过阈值立即 flush 而非等下一帧（与让出机制不冲突：
 * flush 内部依旧按 WRITE_YIELD_THRESHOLD 让出）。
 */

import type { Terminal } from '@xterm/xterm'

/** rAF 合并开关（默认 true）：同帧多次 write 合并为一次渲染提交，消除 TUI
 * 滚动残影（根因见文件头）；置 false 回退逐事件直写，供排查「渲染挂起/黑屏/
 * 帧滞留」时做 A/B 对比（合并依赖 rAF 回调，后台时由兜底定时器兜底） */
const ENABLE_RAF_COALESCE = true

/** 单次 write 上限：超过则拆块（与桌面端一致） */
const MAX_WRITE_CHUNK = 64 * 1024

/** 累积阈值（512KB）：超过立即 flush（移动端特殊处理）。抬到 512KB 的理由：
 * TUI 滚动重绘脉冲（一次逻辑屏幕更新）在 16ms 帧内可数百 KB，阈值过低会
 * 在更新中途截断合并 → 一次屏幕更新拆成两次渲染提交，重蹈批次交错残影。
 * 512KB 足够容纳单帧内全部滚动重绘数据；瞬时内存峰 ≈ 1MB（pending + 合并
 * 缓冲），移动端可接受 */
const MAX_COALESCED_BYTES = 512 * 1024

/** 主线程让出阈值：分片写入每累积该量让出一次宏任务（桌面端 256KB 的一半）。
 * 移动端 CPU 更弱，256KB 连续 parse 在低端机可致数百 ms 冻结（掉触摸/卡滚动）；
 * 128KB 在风暴下约每 2 个 64KB 块让出一次，渲染/输入可插入。对写作节奏影响：
 * 每次让出约 1 帧（setTimeout 0），512KB 合并缓冲最多 4 次让出，可接受 */
const WRITE_YIELD_THRESHOLD = 128 * 1024

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
  // 重入守卫（对齐桌面端 flushWriteQueue）：flush 在让出点 await 期间，后续
  // write 触发的新 flush 直接返回——当前 flush 的 while 轮次会继续消费新入队
  // 数据，避免双写与数据滞留
  let flushing = false

  /** 写入单块字节（仅 rAF 合并关闭的调试直写路径使用；合并路径走 writeInChunks） */
  function writeBytes(data: Uint8Array) {
    // terminal 可能已 dispose（页面切换/会话关闭）：与合并路径 flush 的守卫一致
    if (!terminal.element) return
    for (let i = 0; i < data.length; i += MAX_WRITE_CHUNK) {
      terminal.write(data.subarray(i, i + MAX_WRITE_CHUNK))
    }
  }

  /** 分片写入：按 MAX_WRITE_CHUNK 拆块写，每累积 WRITE_YIELD_THRESHOLD 让出
   * 主线程一次（宏任务），使触摸/渲染能在输出风暴期间插入，防 UI 冻结
   * （对齐桌面端 TerminalPreview.writeInChunks；阈值按弱 CPU 减半） */
  async function writeInChunks(buf: Uint8Array) {
    let written = 0
    for (let i = 0; i < buf.length; i += MAX_WRITE_CHUNK) {
      // terminal 可能已 dispose（页面切换/会话关闭）：让出点期间也可能销毁
      if (!terminal.element) return
      terminal.write(buf.subarray(i, i + MAX_WRITE_CHUNK))
      written += MAX_WRITE_CHUNK
      if (written >= WRITE_YIELD_THRESHOLD) {
        written = 0
        await new Promise((resolve) => setTimeout(resolve, 0))
      }
    }
  }

  /** 清空并合并当前 pending，返回合并缓冲（调用方决定直写或分块）；pending 空
   * 或 terminal 已 dispose 时返回 null（dispose 情形清空计数）。同步函数：不做
   * 任何 await——小数据路径必须零微任务让出，否则 flushing 复位滞后会拦截
   * 随后的 scheduleFlush（对齐桌面端 flushWriteQueue 同步 while 语义） */
  function flushPendingSync(): Uint8Array | null {
    if (!terminal.element) {
      // terminal 可能已 dispose（页面切换/会话关闭）
      pending = []
      totalBytes = 0
      return null
    }
    if (pending.length === 0) return null

    const chunks = pending
    const bytes = totalBytes
    pending = []
    totalBytes = 0

    // 合并同帧所有事件为单块字节
    const combined = new Uint8Array(bytes)
    let offset = 0
    for (const chunk of chunks) {
      combined.set(chunk, offset)
      offset += chunk.byteLength
    }
    return combined
  }

  async function flush() {
    flushRaf = 0
    if (flushTimer) {
      clearTimeout(flushTimer)
      flushTimer = null
    }
    if (pending.length === 0) return
    if (flushing) return
    flushing = true
    try {
      let combined: Uint8Array | null
      // while 轮次：flush 在让出点 await 期间新入队的数据也由本次消费，
      // 直到 pending 清空（无滞留、无双写）。小数据路径全同步（flushPendingSync
      // 无 await），仅大数据分块路径在让出点 await——与测试/桌面端语义一致
      while ((combined = flushPendingSync()) !== null) {
        if (combined.byteLength <= MAX_WRITE_CHUNK) {
          terminal.write(combined)
        } else {
          await writeInChunks(combined)
        }
      }
    } finally {
      flushing = false
    }
  }

  function scheduleFlush() {
    // 已有 flush 在让出点挂起：新数据直接入 pending，由该 flush 的 while 轮次
    // 消费——重复调度会注册多余的 rAF/兜底定时器（残留 timer）
    if (flushing) return
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
      // 累积过大，立即 flush 避免 rAF 延迟影响响应（移动端特殊处理；
      // flush 内部按 WRITE_YIELD_THRESHOLD 让出，不阻塞主线程太久）
      if (flushRaf) {
        cancelAnimationFrame(flushRaf)
        flushRaf = 0
      }
      void flush()
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
    // in-flight flush 的让出点醒来后由 terminal.element 守卫拦截（dispose 后
    // 通常伴随 terminal 销毁/换终端的 element 失效）；此处不置永久禁用标志——
    // 测试断言 dispose 后 coalescer 可复用，且会话切换场景会重建实例
  }

  return Object.assign(write, { dispose }) as WriteCoalescer
}
