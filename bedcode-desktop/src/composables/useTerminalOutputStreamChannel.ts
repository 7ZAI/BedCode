/**
 * 桌面端本地终端输出流 Composable（Channel 传输，与 WS 环回并行）
 *
 * 通过 Tauri Channel（`subscribe_terminal_channel` 命令）以 TB v2 二进制帧
 * 直取 PTY 原始字节。与 `useTerminalOutputStream`（WS 环回）协议/seq/快照
 * 语义完全一致，仅传输层不同：Channel 走 WebView 原生 IPC，大负载经
 * in-memory fetch 拉取，Rust 侧缓冲直到前端消费，天然无丢消息——规避
 * WebKitGTK WS 接收缓冲风暴溢出（opencode 滚动残渣 + Parsing error 根因）。
 *
 * 由 `VITE_TERMINAL_TRANSPORT`（"ws" | "channel"）选择；默认 "ws" 保持原
 * 行为。快照元数据经命令返回值同步获取（替代 WS subscribe_response 控制帧），
 * 帧在 invoke resolve 前先缓冲（pendingFrames），收到快照后按序排空——
 * 与 WS `subscribed`/`pendingFrames` 语义对齐。
 */

import { Channel, invoke } from '@tauri-apps/api/core'

/** TB v2 单帧解析结果（与 WS 路径字段一致） */
export interface OutputStreamFrame {
  data: Uint8Array
  /** 帧内首事件 seq */
  seq: number
  /** 帧内事件数（flags 高 7 位 + 1） */
  eventCount: number
  /** 帧末 seq = seq + eventCount - 1（游标推进基准） */
  lastSeq: number
  isWaiting: boolean
}

/** 快照订阅元数据（命令返回值，camelCase 与 Rust ChannelSubscribeResponse 对齐） */
export interface StreamSnapshot {
  minSeq: number
  snapshotSeq: number
  historyCount: number
  /** 本次订阅唯一 client_id（stop / 重订阅时据此精确取消） */
  clientId: string
}

export interface TerminalStreamOptions {
  /** 连续性校验/去重通过后的原始字节帧，直接入写入管线 */
  onData: (frame: OutputStreamFrame) => void
  /** 历史截断需清屏全量重播（已渲染区域被环形淘汰）时清屏；回放随后到达 */
  onReset: () => void
  /** 环形保留区间头部被淘汰（min_seq > 0）时提示 */
  onTruncated?: (minSeq: number) => void
}

// 帧头 16 字节：magic(2) + version(1) + flags(1) + seq(8 LE) + len(4 LE)
const FRAME_HEADER_LEN = 16
const FRAME_MAGIC = [0x54, 0x42] // "TB"
const FRAME_VERSION = 2
const FRAME_FLAG_WAITING = 0x01
const FRAME_FLAG_COUNT_SHIFT = 1

// ack 节流：累计待 ack 字节达阈值即回发（与 WS 路径 ACK_BYTES_THRESHOLD 一致）
const ACK_BYTES_THRESHOLD = 64 * 1024
// ack 空闲兜底：距上次回发超此时长仍推进则强制回发
const ACK_MAX_IDLE_MS = 250

// 订阅确认前缓冲回放帧的上限（防御性，与 WS 路径一致）
const MAX_PENDING_FRAME_BYTES = 8 * 1024 * 1024

// gap 重订阅冷却（与 WS 路径 GAP_RESUBSCRIBE_COOLDOWN_MS 一致）
const GAP_RESUBSCRIBE_COOLDOWN_MS = 3000

export function useTerminalOutputStreamChannel(options: TerminalStreamOptions) {
  let currentSession = ''
  let stopped = true
  let lastRenderedSeq: number | null = null
  let channel: Channel<ArrayBuffer> | null = null
  let subscribing = false
  // 当前订阅的唯一 client_id（Rust 侧分配；stop / 重订阅时精确取消）
  let clientId: string | null = null
  // invoke resolve 前缓冲帧（快照元数据未到，不能按 seq 去重/截断判定）
  let subscribed = false
  let pendingFrames: OutputStreamFrame[] = []
  let pendingBytes = 0

  // seq gap 处理：任何缺口立即重订阅补回（与 WS 路径同语义，带冷却防风暴）
  let lastGapResubscribeAt = 0

  // 背压 ack 记账（与 WS 路径同语义，经 terminal_channel_ack 命令回发）
  let ackedThroughSeq: number | null = null
  let pendingAckBytes = 0
  let lastAckSentAt = 0
  let ackIdleTimer: ReturnType<typeof setTimeout> | null = null

  /** 解析 TB v2 二进制消息内全部帧（与 WS 路径 parseFrames 逐字节一致） */
  function parseFrames(buffer: ArrayBuffer): OutputStreamFrame[] {
    const frames: OutputStreamFrame[] = []
    const view = new DataView(buffer)
    let offset = 0
    while (offset + FRAME_HEADER_LEN <= buffer.byteLength) {
      if (view.getUint8(offset) !== FRAME_MAGIC[0] || view.getUint8(offset + 1) !== FRAME_MAGIC[1]) break
      if (view.getUint8(offset + 2) !== FRAME_VERSION) break
      const flags = view.getUint8(offset + 3)
      const isWaiting = (flags & FRAME_FLAG_WAITING) !== 0
      const eventCount = (flags >> FRAME_FLAG_COUNT_SHIFT) + 1
      const seq = Number(view.getBigUint64(offset + 4, true))
      const len = view.getUint32(offset + 12, true)
      if (offset + FRAME_HEADER_LEN + len > buffer.byteLength) break
      frames.push({
        data: new Uint8Array(buffer, offset + FRAME_HEADER_LEN, len),
        seq,
        eventCount,
        lastSeq: seq + eventCount - 1,
        isWaiting,
      })
      offset += FRAME_HEADER_LEN + len
    }
    return frames
  }

  /** 交付帧：重播去重 → 连续性校验（seq 缺口）→ 推进游标（与 WS 路径一致） */
  function deliverFrame(frame: OutputStreamFrame) {
    if (lastRenderedSeq !== null && frame.lastSeq <= lastRenderedSeq) return
    if (lastRenderedSeq !== null && frame.seq > lastRenderedSeq + 1) {
      // 缺口 = 字节永久缺失：重订阅让服务端全量重播补回；冷却期内缺口帧
      // 不渲染不推进游标，避免把残缺字节写进终端
      const now = Date.now()
      if (now - lastGapResubscribeAt >= GAP_RESUBSCRIBE_COOLDOWN_MS) {
        lastGapResubscribeAt = now
        console.warn(
          `[useTerminalOutputStreamChannel] seq gap, re-subscribing (frame.start=${frame.seq}, last_rendered=${lastRenderedSeq})`,
        )
        resubscribe()
        return
      }
      return
    }
    lastRenderedSeq = frame.lastSeq
    pendingAckBytes += frame.data.byteLength
    options.onData(frame)
  }

  /** 快照重订阅：保留 last_rendered_seq，重播时按 seq 去重跳过已渲染部分。
   *
   *  先精确取消旧订阅（避免固定 client_id 覆盖语义误删新订阅），再建新订阅 */
  function resubscribe() {
    subscribed = false
    lastGapResubscribeAt = Date.now()
    unsubscribeCurrent()
    dropChannel()
    void doSubscribe()
  }

  /** 精确取消当前订阅（best-effort，不阻塞后续建立新订阅） */
  function unsubscribeCurrent() {
    if (clientId && currentSession) {
      invoke('unsubscribe_terminal_channel', { sessionId: currentSession, clientId }).catch(
        () => {},
      )
      clientId = null
    }
  }

  /** 写入解析完成回调（TerminalPreview onWriteParsed 接线）：经命令回发 ack */
  function confirmWriteParsed() {
    if (stopped || lastRenderedSeq === null || ackedThroughSeq === lastRenderedSeq) return
    const now = Date.now()
    if (pendingAckBytes < ACK_BYTES_THRESHOLD && lastAckSentAt !== 0 && now - lastAckSentAt < ACK_MAX_IDLE_MS) {
      if (!ackIdleTimer) {
        ackIdleTimer = setTimeout(() => {
          ackIdleTimer = null
          confirmWriteParsed()
        }, ACK_MAX_IDLE_MS)
      }
      return
    }
    if (ackIdleTimer) {
      clearTimeout(ackIdleTimer)
      ackIdleTimer = null
    }
    invoke('terminal_channel_ack', { sessionId: currentSession, ackedSeq: lastRenderedSeq }).catch((e) => {
      console.warn('[useTerminalOutputStreamChannel] ack failed:', e)
    })
    ackedThroughSeq = lastRenderedSeq
    pendingAckBytes = 0
    lastAckSentAt = now
  }

  /** 丢弃当前 Channel 引用（Rust 侧据 client_id 覆盖/subscribe 关闭后自动清理） */
  function dropChannel() {
    channel = null
  }

  /** 建立 Channel 订阅：创建 Channel → invoke → 收到快照元数据后排空缓冲帧 */
  async function doSubscribe() {
    if (stopped || subscribing || !currentSession) return
    subscribing = true
    const sessionAtStart = currentSession
    try {
      const ch = new Channel<ArrayBuffer>()
      ch.onmessage = (msg) => {
        const frames = parseFrames(msg)
        if (frames.length === 0) {
          console.error('[useTerminalOutputStreamChannel] invalid binary frame received')
          return
        }
        for (const frame of frames) {
          if (!subscribed) {
            // 快照元数据未到：缓冲，确认后按序写入
            pendingFrames.push(frame)
            pendingBytes += frame.data.byteLength
            if (pendingBytes > MAX_PENDING_FRAME_BYTES) {
              console.error('[useTerminalOutputStreamChannel] pending frame overflow, re-subscribing')
              resubscribe()
              return
            }
            continue
          }
          deliverFrame(frame)
        }
      }
      channel = ch
      const snapshot = await invoke<StreamSnapshot>('subscribe_terminal_channel', {
        sessionId: sessionAtStart,
        channel: ch,
      })
      // await 期间可能被 stop / 切会话
      if (stopped || currentSession !== sessionAtStart) return
      clientId = snapshot.clientId
      // 历史头部被环形淘汰：已渲染区域不可恢复 → 清屏全量重播
      if (lastRenderedSeq !== null && snapshot.minSeq > lastRenderedSeq + 1) {
        console.warn(
          `[useTerminalOutputStreamChannel] history truncated: min_seq=${snapshot.minSeq} > last_rendered=${lastRenderedSeq}+1, full replay`,
        )
        lastRenderedSeq = null
        options.onReset()
      }
      if (snapshot.minSeq > 0) {
        options.onTruncated?.(snapshot.minSeq)
      }
      // 排空订阅确认前缓冲的回放帧（按到达顺序写入，保持连续）
      subscribed = true
      const frames = pendingFrames
      pendingFrames = []
      pendingBytes = 0
      for (const frame of frames) {
        deliverFrame(frame)
      }
    } catch (e) {
      console.warn('[useTerminalOutputStreamChannel] subscribe failed:', e)
    } finally {
      subscribing = false
    }
  }

  /** 建立新会话上下文（只准备不订阅）；seq 游标重置 */
  function start(sessionId: string) {
    if (!sessionId) return
    if (!stopped && currentSession === sessionId && (channel || subscribing)) return
    unsubscribeCurrent()
    dropChannel()
    currentSession = sessionId
    lastRenderedSeq = null
    ackedThroughSeq = null
    pendingAckBytes = 0
    lastAckSentAt = 0
    lastGapResubscribeAt = 0
    stopped = false
    subscribed = false
    pendingFrames = []
    pendingBytes = 0
  }

  /** 发送订阅（terminal 就绪后调用）：实际建立 Channel 流（幂等：已订阅不重复） */
  function subscribe() {
    if (stopped || subscribing || subscribed) return
    void doSubscribe()
  }

  /** 停止（组件卸载 / 会话停止） */
  function stop() {
    stopped = true
    subscribed = false
    unsubscribeCurrent()
    dropChannel()
    if (ackIdleTimer) {
      clearTimeout(ackIdleTimer)
      ackIdleTimer = null
    }
    lastRenderedSeq = null
    ackedThroughSeq = null
    pendingAckBytes = 0
    pendingFrames = []
    pendingBytes = 0
  }

  return { start, subscribe, stop, confirmWriteParsed }
}
