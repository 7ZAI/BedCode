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
 * 背压水位上报（`onParsed`）：每块经 xterm 解析**完成**（write 回调）后上报字节数，
 * 是 `terminal-session.ack-rendered` 的唯一数据源——上游（桌面插件）按此放行下一批，
 * 故禁止改成「入队即计数」（那会让背压退化为无界 WebView 队列）。
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

import { logger } from '../host'
import type { Terminal } from '@xterm/xterm'

/** rAF 合并开关（默认 true）：同帧多次 write 合并为一次渲染提交，消除 TUI
 * 滚动残影（根因见文件头）；置 false 回退逐事件直写，供排查「渲染挂起/黑屏/
 * 帧滞留」时做 A/B 对比（合并依赖 rAF 回调，后台时由兜底定时器兜底） */
const ENABLE_RAF_COALESCE = true

/** 单次 write 上限：超过则拆块（与桌面端一致） */
const MAX_WRITE_CHUNK = 64 * 1024

// ==================== 链路调试统计（渲染管线字节对账，2s 节流） ====================

/** 统计打点间隔（ms）：输出风暴期不逐次刷日志 */
const STATS_INTERVAL_MS = 2000

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
  /**
   * 解析完成回调（**分块粒度**，字节数 = 该块长度）
   *
   * 背压水位（`terminal-session.ack-rendered`）的唯一推进点：必须是「xterm 解析
   * 完成」而不是「收到字节」，否则 WebView 解析跟不上时上游照旧全速灌入（真实
   * 缓冲退化为 WebView 消息队列，背压失效）。世代失效（dispose 清屏/重锚）后
   * 在飞分块的完成回调不再上报——那块字节属于旧世代。
   */
  onParsed?: (bytes: number) => void
}

export function createWriteCoalescer(
  terminal: Terminal,
  options: WriteCoalescerOptions = {},
): WriteCoalescer {
  // rAF 合并关闭时每个事件直接写入（调试/对比路径）
  const rafCoalesce = options.enableRafCoalesce ?? ENABLE_RAF_COALESCE
  const onParsed = options.onParsed
  let pending: Uint8Array[] = []
  let totalBytes = 0
  let flushRaf = 0
  let flushTimer: ReturnType<typeof setTimeout> | null = null
  // 重入守卫（对齐桌面端 flushWriteQueue）：flush 在让出点 await 期间，后续
  // write 触发的新 flush 直接返回——当前 flush 的 while 轮次会继续消费新入队
  // 数据，避免双写与数据滞留
  let flushing = false
  // 写入世代：`dispose()`（清屏/重锚路径）后旧世代的**在飞**分块写必须中止——
  // 否则 `terminal.clear()` 之后旧内容仍会继续写入 xterm（清屏白做、与新一代
  // 重放交错）。每次 dispose 递增，让出点醒来时比对，失配即停
  let epoch = 0

  // 链路调试统计（字节对账）：入队/合并 flush/兜底 flush/拆块/让出，
  // 与 terminalBuffer bytesRendered 对账可验证渲染管线无滞留丢字节
  let statsWrites = 0
  let statsWriteBytes = 0
  let statsFlushes = 0
  let statsFlushBytes = 0
  let statsMaxPending = 0
  let statsImmediateFlush = 0
  let statsFallbackFlush = 0
  let statsSplits = 0
  let statsYields = 0
  let statsLastLogAt = 0

  /** 周期打点（2s 节流）：渲染管线消费口径统计 */
  function maybeLogStats() {
    const now = Date.now()
    if (now - statsLastLogAt < STATS_INTERVAL_MS) return
    statsLastLogAt = now
    logger.debug(
      `[writeCoalescer] stats: writes=${statsWrites} writeBytes=${statsWriteBytes} ` +
        `flushes=${statsFlushes} flushBytes=${statsFlushBytes} maxPending=${statsMaxPending} ` +
        `immediate=${statsImmediateFlush} fallback=${statsFallbackFlush} splits=${statsSplits} yields=${statsYields}`,
    )
  }

  /**
   * 写出一块并上报解析完成（背压水位推进点）。
   *
   * 传 xterm 的 write 回调而不是在上层假设同步完成：xterm 的解析是异步的
   * （WriteBuffer 在积压时会暂停 parser，靠 timeouts/idle 恢复），只有回调
   * 触发才代表该块真的进了终端解析器。世代失效（dispose）后回调不再上报。
   */
  function writeChunk(chunk: Uint8Array, chunkEpoch: number) {
    terminal.write(chunk, () => {
      if (chunkEpoch !== epoch) return
      onParsed?.(chunk.byteLength)
    })
  }

  /** 写入单块字节（仅 rAF 合并关闭的调试直写路径使用；合并路径走 writeInChunks） */
  function writeBytes(data: Uint8Array) {
    // terminal 可能已 dispose（页面切换/会话关闭）：与合并路径 flush 的守卫一致
    if (!terminal.element) return
    const writeEpoch = epoch
    for (let i = 0; i < data.length; i += MAX_WRITE_CHUNK) {
      writeChunk(data.subarray(i, i + MAX_WRITE_CHUNK), writeEpoch)
    }
  }

  /** 分片写入：按 MAX_WRITE_CHUNK 拆块写，每累积 WRITE_YIELD_THRESHOLD 让出
   * 主线程一次（宏任务），使触摸/渲染能在输出风暴期间插入，防 UI 冻结
   * （对齐桌面端 TerminalPreview.writeInChunks；阈值按弱 CPU 减半）
   *
   * `writeEpoch` = 本次 flush 启动时的世代：让出点醒来后世代已变（dispose
   * 清屏/重锚）即中止剩余分块——旧世代内容不得写进已清空的屏幕 */
  async function writeInChunks(buf: Uint8Array, writeEpoch: number) {
    let written = 0
    statsSplits++
    for (let i = 0; i < buf.length; i += MAX_WRITE_CHUNK) {
      // 世代已作废（dispose 清屏/重锚）、或 terminal 已 dispose（页面切换/
      // 会话关闭）：让出点期间两种失效都要拦
      if (writeEpoch !== epoch) return
      if (!terminal.element) return
      const chunk = buf.subarray(i, i + MAX_WRITE_CHUNK)
      writeChunk(chunk, writeEpoch)
      written += MAX_WRITE_CHUNK
      if (written >= WRITE_YIELD_THRESHOLD) {
        written = 0
        statsYields++
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
    const flushEpoch = epoch
    try {
      let combined: Uint8Array | null
      // while 轮次：flush 在让出点 await 期间新入队的数据也由本次消费，
      // 直到 pending 清空（无滞留、无双写）。小数据路径全同步（flushPendingSync
      // 无 await），仅大数据分块路径在让出点 await——与测试/桌面端语义一致。
      // `flushEpoch !== epoch` = 期间发生 dispose（清屏/重锚）：本次 flush 立即收尾
      while (flushEpoch === epoch && (combined = flushPendingSync()) !== null) {
        statsFlushes++
        statsFlushBytes += combined.byteLength
        if (combined.byteLength <= MAX_WRITE_CHUNK) {
          writeChunk(combined, flushEpoch)
        } else {
          await writeInChunks(combined, flushEpoch)
        }
      }
    } finally {
      flushing = false
      // 本 flush 因世代作废（dispose）提前收尾时，让出期间新入队的数据不能滞留：
      // 期间 `write()` 调 scheduleFlush 被 flushing 守卫挡掉（不重复调度），
      // 收尾后必须补一次调度，否则新世代首批数据要等到下一次 write 才上屏
      if (pending.length > 0) scheduleFlush()
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
        // rAF 暂停（后台/最小化）触发的兜底 flush：出现频次反映前台渲染状态
        statsFallbackFlush++
        flush()
      }, FALLBACK_FLUSH_MS)
    }
  }

  function write(data: Uint8Array) {
    if (data.length === 0) return

    // 链路调试对账：入队计数 + 待写积压高水位
    statsWrites++
    statsWriteBytes += data.byteLength

    if (!rafCoalesce) {
      // rAF 合并关闭（调试开关）：每个事件直接写入，不经合并管线
      writeBytes(data)
      return
    }

    pending.push(data)
    totalBytes += data.byteLength
    statsMaxPending = Math.max(statsMaxPending, totalBytes)

    if (totalBytes >= MAX_COALESCED_BYTES) {
      // 累积过大，立即 flush 避免 rAF 延迟影响响应（移动端特殊处理；
      // flush 内部按 WRITE_YIELD_THRESHOLD 让出，不阻塞主线程太久）
      statsImmediateFlush++
      if (flushRaf) {
        cancelAnimationFrame(flushRaf)
        flushRaf = 0
      }
      void flush()
    } else {
      scheduleFlush()
    }
    maybeLogStats()
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
    // 链路调试（字节对账）：生命周期终点汇总——writeBytes 与 flushBytes 差值
    // 即 dispose 时被丢弃的未写入字节（截断清屏路径为预期行为）
    logger.debug(
      `[writeCoalescer] dispose: writes=${statsWrites} writeBytes=${statsWriteBytes} ` +
        `flushes=${statsFlushes} flushBytes=${statsFlushBytes} maxPending=${statsMaxPending} ` +
        `immediate=${statsImmediateFlush} fallback=${statsFallbackFlush} splits=${statsSplits} yields=${statsYields}`,
    )
    pending = []
    totalBytes = 0
    // 世代递增：中止在飞分块写的剩余块（清屏后旧内容不得再上屏）；
    // in-flight flush 的让出点醒来后由 writeEpoch 失配拦截，其 finally 会补调度
    // dispose 后新入队的数据。此处不置永久禁用标志——测试断言 dispose 后
    // coalescer 可复用，且会话切换场景会重建实例
    epoch += 1
  }

  return Object.assign(write, { dispose }) as WriteCoalescer
}
