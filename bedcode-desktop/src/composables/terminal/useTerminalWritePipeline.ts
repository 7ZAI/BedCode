/**
 * 终端写入管线 + 本地输出流接线（TerminalPreview 拆分产物）
 *
 * 职责：
 * - 写入管线：实时输出合并为单次 write（DEC 2026 同步输出包裹），渲染器缓存
 *   所有变更到下一帧统一绘制，避免逐块绘制的撕裂/重影；大块分片 + 水线让出
 *   主线程（风暴期间允许渲染/输入插入，防 UI 冻结）
 * - 回放静止补刷：订阅/重订阅后的历史回放与渲染器冷启动时序竞态，可能留下
 *   未被后续帧覆盖的中间态（字符残缺/错位）；连续 REPLAY_IDLE_MS 无新数据
 *   时自动补一次全量重绘
 * - 本地 WS 输出流接线：固定走 Tauri Channel（原生 IPC，规避 WebKitGTK WS 缓冲
 *   溢出丢消息）；WS 环回链路（/ws/terminal/local）已整体下线
 *
 * 依赖：共享内核 ctx（terminalRef / isUserScrolling / scrollToBottom 回调）。
 */
import type { TerminalKernelContext } from './terminalKernel'
import { useTerminalOutputStreamChannel } from '@/composables/useTerminalOutputStreamChannel'
import { useToast } from '@/composables/useToast'
import { useI18n } from 'vue-i18n'
import { logger } from '@/utils/frontendLogger'

// 回放静止补刷：订阅后置补刷标记，每次有数据入队重置静止计时器；回放完毕连续
// REPLAY_IDLE_MS 无新数据 → 自动补一次全量重绘（等价用户点击刷新，glyph 缓存
// 全部命中，无副作用；实时持续输出时计时器不断重置不会触发）
const REPLAY_IDLE_MS = 250

// 单次 write 上限：超过则拆块，让 xterm parser 在块间让出主线程，
// 避免单帧解析超大字符串导致 UI 卡顿
const MAX_WRITE_CHUNK = 64 * 1024

// 写入水线阈值（对齐 VS Code high-water 思想）：单次 flush 累计写入达到该值即
// 让出主线程一次（宏任务），使渲染/输入能在历史回放或输出风暴期间插入，避免
// UI 冻结。仅控制"写节奏"，不限制总量——积压数据仍在 writeQueue，下个 while
// 轮次继续写。
const WRITE_YIELD_THRESHOLD = 256 * 1024

export function useTerminalWritePipeline(ctx: TerminalKernelContext) {
  const { t } = useI18n()
  const toast = useToast()

  let writeQueue: Uint8Array[] = []
  let writeQueueBytes = 0
  let flushRaf = 0
  let flushTimer: ReturnType<typeof setTimeout> | null = null
  let replayRefreshTimer: ReturnType<typeof setTimeout> | null = null
  let pendingReplayRefresh = false
  let flushing = false
  // 历史截断提示标记：min_seq > 0 说明会话开头输出已不可恢复，
  // 仅首次提示一次，后续重连/重订阅触发时仅后台日志记录
  let historyTruncatedNotified = false

  async function flushWriteQueue() {
    // 避免重入：已有 flush 在跑（其 while 轮次会消费新入队数据），直接返回
    if (flushing) return
    flushing = true
    // 清掉 rAF / timer 标记（本次 flush 接管调度）
    flushRaf = 0
    if (flushTimer) {
      clearTimeout(flushTimer)
      flushTimer = null
    }
    try {
      while (writeQueue.length > 0) {
        const terminal = ctx.terminalRef.value
        if (!terminal) {
          // 终端未就绪：丢弃（数据已进全局缓存，可从历史恢复）
          writeQueue = []
          writeQueueBytes = 0
          return
        }
        const chunks = writeQueue
        const totalBytes = writeQueueBytes
        writeQueue = []
        writeQueueBytes = 0

        // 合并同帧所有事件为单块字节，一次 write
        const combined = new Uint8Array(totalBytes)
        let offset = 0
        for (const chunk of chunks) {
          combined.set(chunk, offset)
          offset += chunk.byteLength
        }

        if (totalBytes <= MAX_WRITE_CHUNK) {
          terminal.write(combined)
        } else {
          await writeInChunks(combined)
        }
      }
    } finally {
      flushing = false
    }
  }

  /** 分片写入：按 MAX_WRITE_CHUNK 拆块写，每累积 WRITE_YIELD_THRESHOLD 让出主线程一次 */
  async function writeInChunks(buf: Uint8Array) {
    // 防御：调用方 flushWriteQueue 已保证 terminal 非空，此处收窄类型（并发 agent 新增函数）
    const terminal = ctx.terminalRef.value
    if (!terminal) return
    let written = 0
    for (let i = 0; i < buf.length; i += MAX_WRITE_CHUNK) {
      terminal.write(buf.subarray(i, i + MAX_WRITE_CHUNK))
      written += MAX_WRITE_CHUNK
      if (written >= WRITE_YIELD_THRESHOLD) {
        written = 0
        // 宏任务让出：风暴期间允许渲染/输入插入，防 UI 冻结
        await new Promise((r) => setTimeout(r, 0))
      }
    }
  }

  /** 回放静止补刷：置补刷标记并重置静止计时器（数据继续到达时反复重置） */
  function armReplayRefresh() {
    pendingReplayRefresh = true
    if (replayRefreshTimer) clearTimeout(replayRefreshTimer)
    replayRefreshTimer = setTimeout(() => {
      replayRefreshTimer = null
      if (!pendingReplayRefresh) return
      pendingReplayRefresh = false
      // xterm 已销毁（element 已脱离 DOM）则不再重绘
      const terminal = ctx.terminalRef.value
      if (terminal && terminal.element?.isConnected) {
        terminal.refresh(0, terminal.rows - 1)
      }
    }, REPLAY_IDLE_MS)
  }

  /** 入队输出：合并到下一渲染帧统一写入 */
  function enqueueOutput(data: Uint8Array) {
    if (data.length === 0) return
    writeQueue.push(data)
    writeQueueBytes += data.byteLength
    // 数据继续到达 = 回放/输出仍在进行：重置静止补刷计时器（仅在补刷待命期）
    if (pendingReplayRefresh) {
      armReplayRefresh()
    }
    if (flushRaf) return
    // rAF 合并同帧事件；100ms 兜底：窗口最小化（rAF 暂停）时也能及时清空队列
    flushRaf = requestAnimationFrame(flushWriteQueue)
    if (!flushTimer) {
      flushTimer = setTimeout(() => {
        flushTimer = null
        if (flushRaf) {
          cancelAnimationFrame(flushRaf)
          flushRaf = 0
        }
        void flushWriteQueue()
      }, 100)
    }
  }

  // 本地 WS 输出流（快照模型）:
  // - onData 帧已通过 seq 连续性校验/重播去重，直接写入（无去重/无补序）
  // - onReset 时清屏：快照重订阅遇到历史截断（已渲染区被环形淘汰）后全量重播
  // - onTruncated（min_seq > 0，参数即 min_seq）：会话开头输出已不可恢复
  const terminalStreamOptions = {
    onData: ({ data }: { data: Uint8Array }) => {
      enqueueOutput(data)
      if (!ctx.isUserScrolling.value) {
        ctx.callbacks.scrollToBottom()
      }
    },
    onReset: () => {
      const terminal = ctx.terminalRef.value
      if (terminal) {
        terminal.clear()
        // 清屏后即将全量重播：重播结束静止时补刷一次（防重播中间态残留）
        armReplayRefresh()
      }
    },
    onTruncated: (minOffset: number) => {
      if (historyTruncatedNotified) {
        // 已提示过：仅后台日志记录，不再弹 toast 打扰用户
        logger.warn(
          `[TerminalPreview] 终端历史已被环形缓冲截断（已提示过，仅记录）：min_offset=${minOffset}`,
        )
        return
      }
      historyTruncatedNotified = true
      logger.warn(
        `[TerminalPreview] 终端历史已被环形缓冲截断：min_offset=${minOffset}，会话开头输出不可用`,
      )
      toast.warning(t('desktop.terminal.historyTruncated'))
    },
  }

  const terminalStream = useTerminalOutputStreamChannel(terminalStreamOptions)

  /** 新会话开始时重置历史截断提示标记，允许再次提示 */
  function resetTruncatedNotified() {
    historyTruncatedNotified = false
  }

  /** 组件卸载清理：取消挂起调度并清空队列（未 flush 的数据仍存于服务端环形，重开窗口可恢复） */
  function disposePipeline() {
    if (flushRaf) {
      cancelAnimationFrame(flushRaf)
      flushRaf = 0
    }
    if (flushTimer) {
      clearTimeout(flushTimer)
      flushTimer = null
    }
    writeQueue.length = 0
    writeQueueBytes = 0
    if (replayRefreshTimer) {
      clearTimeout(replayRefreshTimer)
      replayRefreshTimer = null
      pendingReplayRefresh = false
    }
  }

  return {
    enqueueOutput,
    armReplayRefresh,
    terminalStream,
    resetTruncatedNotified,
    disposePipeline,
  }
}
