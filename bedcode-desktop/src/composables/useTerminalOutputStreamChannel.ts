/**
 * 桌面端本地终端输出流 Composable（Tauri Channel 传输，唯一路径）
 *
 * 通过 Tauri Channel（`subscribe_terminal_channel` 命令）以 TB v3 二进制帧
 * 直取 PTY 原始字节。WS 环回链路（/ws/terminal/local）已整体下线，桌面本地
 * 终端输出只走本路径：Channel 走 WebView 原生 IPC，大负载经 in-memory fetch
 * 拉取，Rust 侧缓冲直到前端消费，天然无丢消息——规避 WebKitGTK WS 接收缓冲
 * 风暴溢出（opencode 滚动残渣 + Parsing error 根因）。
 *
 * 快照元数据经命令返回值同步获取（替代 WS subscribe_response 控制帧），帧在
 * invoke resolve 前先缓冲（pendingFrames），收到快照后按序排空。
 *
 * TB v3 字节语义（`.scratch/pty-byte-history/spec.md`）：连续性以累计字节偏移
 * 表达；帧头 start_offset(8 LE) + len(4 LE)，end_offset = start_offset + len；
 * 游标 = 已渲染帧末 endOffset；跨帧裁剪（overlap = cursor - startOffset）根治
 * 重播重复渲染。
 */

import { Channel, invoke } from '@tauri-apps/api/core'

/** TB v3 单帧解析结果（与 WS 路径字段一致） */
export interface OutputStreamFrame {
  data: Uint8Array
  /** 帧内首字节的会话内累计偏移 */
  startOffset: number
  /** 帧内末字节偏移 = startOffset + len（游标推进基准） */
  endOffset: number
  isWaiting: boolean
}

/** 快照订阅元数据（命令返回值，camelCase 与 Rust ChannelSubscribeResponse 对齐） */
export interface StreamSnapshot {
  minOffset: number
  snapshotOffset: number
  historyBytes: number
  /** 本次订阅唯一 client_id（stop / 重订阅时据此精确取消） */
  clientId: string
}

export interface TerminalStreamOptions {
  /** 连续性校验/去重通过后的原始字节帧，直接入写入管线 */
  onData: (frame: OutputStreamFrame) => void
  /** 历史截断需清屏全量重播（已渲染区域被环形淘汰）时清屏；回放随后到达 */
  onReset: () => void
  /** 环形保留区间头部被淘汰（min_offset > 0）时提示 */
  onTruncated?: (minOffset: number) => void
}

// 帧头 16 字节：magic(2) + version(1) + flags(1) + start_offset(8 LE) + len(4 LE)
const FRAME_HEADER_LEN = 16
const FRAME_MAGIC = [0x54, 0x42] // "TB"
const FRAME_VERSION = 3
const FRAME_FLAG_WAITING = 0x01

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
  let lastRenderedOffset: number | null = null
  let channel: Channel<ArrayBuffer> | null = null
  let subscribing = false
  // 当前订阅的唯一 client_id（Rust 侧分配；stop / 重订阅时精确取消）
  let clientId: string | null = null
  // invoke resolve 前缓冲帧（快照元数据未到，不能按 offset 去重/截断判定）
  let subscribed = false
  let pendingFrames: OutputStreamFrame[] = []
  let pendingBytes = 0

  // offset gap 处理：任何缺口立即重订阅补回（与 WS 路径同语义，带冷却防风暴）
  let lastGapResubscribeAt = 0

  // 背压 ack 记账（与 WS 路径同语义，经 terminal_channel_ack 命令回发）
  let ackedThroughOffset: number | null = null
  let pendingAckBytes = 0
  let lastAckSentAt = 0
  let ackIdleTimer: ReturnType<typeof setTimeout> | null = null

  /** 解析 TB v3 二进制消息内全部帧（与 WS 路径 parseFrames 逐字节一致） */
  function parseFrames(buffer: ArrayBuffer): OutputStreamFrame[] {
    const frames: OutputStreamFrame[] = []
    const view = new DataView(buffer)
    let offset = 0
    while (offset + FRAME_HEADER_LEN <= buffer.byteLength) {
      if (view.getUint8(offset) !== FRAME_MAGIC[0] || view.getUint8(offset + 1) !== FRAME_MAGIC[1]) break
      if (view.getUint8(offset + 2) !== FRAME_VERSION) break
      const flags = view.getUint8(offset + 3)
      const isWaiting = (flags & FRAME_FLAG_WAITING) !== 0
      const startOffset = Number(view.getBigUint64(offset + 4, true))
      const len = view.getUint32(offset + 12, true)
      if (offset + FRAME_HEADER_LEN + len > buffer.byteLength) break
      frames.push({
        data: new Uint8Array(buffer, offset + FRAME_HEADER_LEN, len),
        startOffset,
        endOffset: startOffset + len,
        isWaiting,
      })
      offset += FRAME_HEADER_LEN + len
    }
    return frames
  }

  /**
   * 交付帧：重播去重 → 跨帧裁剪 → 连续性校验（offset 缺口）→ 推进游标
   *
   * 跨帧裁剪（TB v3 根治重复渲染）：重订阅全量重播的首帧可能跨过已渲染游标
   * （startOffset < cursor < endOffset），按帧内字节区间精确裁掉前半段：
   * `overlap = cursor - startOffset`，渲染 `data[overlap..]`——游标恒为帧边界，
   * 切片起点恒合法，无整帧重渲染
   */
  function deliverFrame(frame: OutputStreamFrame) {
    // 去重：整帧已渲染（endOffset ≤ 游标）跳过
    if (lastRenderedOffset !== null && frame.endOffset <= lastRenderedOffset) return
    // 连续性缺口：帧首越过游标（字节永久缺失）→ 重订阅全量重播补回；
    // 冷却期内缺口帧不渲染不推进游标，避免把残缺字节写进终端
    if (lastRenderedOffset !== null && frame.startOffset > lastRenderedOffset) {
      const now = Date.now()
      if (now - lastGapResubscribeAt >= GAP_RESUBSCRIBE_COOLDOWN_MS) {
        lastGapResubscribeAt = now
        console.warn(
          `[useTerminalOutputStreamChannel] offset gap, re-subscribing (frame.start=${frame.startOffset}, last_rendered=${lastRenderedOffset})`,
        )
        resubscribe()
        return
      }
      return
    }
    // 跨帧裁剪：帧覆盖已渲染游标（重播首帧跨游标）→ 裁掉前半段，零重复
    const cursor = lastRenderedOffset ?? frame.startOffset
    if (cursor > frame.startOffset) {
      const overlap = cursor - frame.startOffset
      frame = { ...frame, data: frame.data.subarray(overlap) }
    }
    lastRenderedOffset = frame.endOffset
    pendingAckBytes += frame.data.byteLength
    options.onData(frame)
  }

  /** 快照重订阅：保留 last_rendered_offset，重播按字节区间去重 + 跨帧裁剪。
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
    if (stopped || lastRenderedOffset === null || ackedThroughOffset === lastRenderedOffset) return
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
    invoke('terminal_channel_ack', { sessionId: currentSession, ackedOffset: lastRenderedOffset }).catch((e) => {
      console.warn('[useTerminalOutputStreamChannel] ack failed:', e)
    })
    ackedThroughOffset = lastRenderedOffset
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
      if (lastRenderedOffset !== null && snapshot.minOffset > lastRenderedOffset) {
        console.warn(
          `[useTerminalOutputStreamChannel] history truncated: min_offset=${snapshot.minOffset} > last_rendered=${lastRenderedOffset}, full replay`,
        )
        lastRenderedOffset = null
        options.onReset()
      }
      if (snapshot.minOffset > 0) {
        options.onTruncated?.(snapshot.minOffset)
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

  /** 建立新会话上下文（只准备不订阅）；字节游标重置 */
  function start(sessionId: string) {
    if (!sessionId) return
    if (!stopped && currentSession === sessionId && (channel || subscribing)) return
    unsubscribeCurrent()
    dropChannel()
    currentSession = sessionId
    lastRenderedOffset = null
    ackedThroughOffset = null
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
    lastRenderedOffset = null
    ackedThroughOffset = null
    pendingAckBytes = 0
    pendingFrames = []
    pendingBytes = 0
  }

  return { start, subscribe, stop, confirmWriteParsed }
}
