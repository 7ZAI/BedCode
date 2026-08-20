/**
 * 桌面端本地 WS 输出流 Composable（快照模型，07 迁移）
 *
 * 通过本地环回 WebSocket（/ws/terminal/local）以 TB v2 二进制帧直取 PTY 原始字节。
 *
 * 快照模型（spec §5.2/§5.3，服务端契约）：
 * - 帧头 16B：magic "TB" + version=2 + flags + seq(8 LE) + len(4 LE)
 *   seq = 帧内首事件 index；flags bit0 = is_waiting，高 7 位 = 事件数 - 1
 *   （帧末 seq = seq + 事件数 - 1，消费端据此做 seq 级连续性校验与去重）
 * - 订阅响应 {min_seq, snapshot_seq(=wire max_seq), history_count}：快照元数据
 * - 历史段 = [min_seq .. snapshot_seq] → history_end → 实时段；本地通道零缓冲直通，
 *   每事件一帧，seq 严格 +1 连续
 *
 * 恢复模型（07 从字节游标迁移）：
 * - last_rendered_seq（替代字节 cursor）：已渲染到的帧末 seq，跨重连保留
 * - 重订阅（缺口/断线）＝快照重订阅：重播时跳过 ≤ last_rendered_seq 的帧
 * - 截断判定：min_seq > last_rendered_seq + 1 说明已渲染区域被环形淘汰 → 清屏全量重播
 * - 连续性内幕：seq 缺口（frame.seq > last_rendered_seq + 1）→ 快照重订阅；
 *   直通模式无合并，正常流 frame.seq 恒 = last_rendered_seq + 1，缺口即丢帧
 *
 * 生命周期（显式控制，terminal 就绪是订阅的前置条件）：
 * - start(sessionId)：断开旧连接并建立新连接（只握手，不订阅；seq 游标重置）
 * - subscribe()：发送订阅消息（terminal 就绪后调用；WS 断线重连后自动重发）
 * - stop()：断开并停止重连（组件卸载 / 会话停止）
 *
 * 会话停止（SESSION_NOT_FOUND）时重试有限次数后停止，避免无限重连；
 * 会话重新启动后由消费者再次 start() + subscribe() 恢复。
 */

import { invoke } from '@tauri-apps/api/core'

/** TB v2 单帧解析结果 */
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

/** 快照订阅元数据（服务端 SubscribeResponse 的 min_seq/max_seq/history_count） */
export interface StreamSnapshot {
  /** 队列最早存续事件序号（环形淘汰后推进；> 0 表示历史头部被截断） */
  minSeq: number
  /** 订阅时刻队列最新序号（历史边界） */
  snapshotSeq: number
  historyCount: number
}

export interface TerminalStreamOptions {
  /** 连续性校验/去重通过后的原始字节帧，直接入写入管线 */
  onData: (frame: OutputStreamFrame) => void
  /** 历史截断需清屏全量重播（已渲染区域被环形淘汰）时清屏；回放随后到达 */
  onReset: () => void
  /** 环形保留区间头部被淘汰（min_seq > 0）时提示，用于"历史被截断"文案 */
  onTruncated?: (minSeq: number) => void
}

// 帧头 16 字节：magic(2) + version(1) + flags(1) + seq(8 LE) + len(4 LE)
const FRAME_HEADER_LEN = 16
const FRAME_MAGIC = [0x54, 0x42] // "TB"
const FRAME_VERSION = 2
const FRAME_FLAG_WAITING = 0x01
const FRAME_FLAG_COUNT_SHIFT = 1
// 背压 ack 标志位（仅客户端→服务端方向使用；服务端→客户端帧的 flags 低 2 位
// 是 WAITING + 事件数编码，与服务端只认入站二进制帧作 ack 的解析互不冲突）
const FRAME_FLAG_ACK = 0x02
// ack 节流：累计待 ack 字节达阈值即回发（对齐上游 WATERMARK 节奏，风暴批发）
const ACK_BYTES_THRESHOLD = 64 * 1024
// ack 空闲兜底：距上次回发超此时长仍推进则强制回发（低频输出不滞留记账）
const ACK_MAX_IDLE_MS = 250

// 重连退避（ms）：500 → 1000 → 2000 → 4000 → 8000 封顶
const RECONNECT_BASE_MS = 500
const RECONNECT_MAX_MS = 8000

// 订阅确认前缓冲回放帧的上限（防御性；服务端环形容量远小于此）
const MAX_PENDING_FRAME_BYTES = 8 * 1024 * 1024

// 会话不存在（启动中/已停止）时的重试上限，超出后停止等待外部恢复
const MAX_SESSION_MISSING_STRIKES = 3

export function useTerminalOutputStream(options: TerminalStreamOptions) {
  let ws: WebSocket | null = null
  let connecting = false
  let currentSession = ''
  let lastRenderedSeq: number | null = null
  let stopped = true
  let pendingSubscribe = false
  let reconnectAttempts = 0
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null
  let sessionMissingStrikes = 0
  // subscribe_response 到达前缓冲回放帧：服务端 SubscribeResult 与历史帧经不同
  // actor 消息路径发送，顺序无保证，帧可能先于控制消息到达
  let pendingFrames: OutputStreamFrame[] = []
  let pendingBytes = 0
  let subscribed = false

  // ==================== 背压 ack（渲染解析反馈环，spec 04-06） ====================
  // 写入解析完成（TerminalPreview onWriteParsed）后回发 ack 帧携已渲染到的
  // last_rendered_seq；服务端据此暂停/恢复 PTY 读取，渲染速度反向钳制源头流速。
  // 节流：字节阈值（风暴批发）+ 空闲兜底（低频输出也最终 ack），避免逐帧刷屏
  let ackedThroughSeq: number | null = null
  let pendingAckBytes = 0
  let lastAckSentAt = 0
  let ackIdleTimer: ReturnType<typeof setTimeout> | null = null

  /** 构造背压 ack 帧（复用 TB v2 帧头 + ACK 标志位 + acked_seq + session_id 负载） */
  function buildAckFrame(sessionId: string, ackedSeq: number): ArrayBuffer {
    const sessionBytes = new TextEncoder().encode(sessionId)
    const buf = new ArrayBuffer(FRAME_HEADER_LEN + sessionBytes.byteLength)
    const view = new DataView(buf)
    view.setUint8(0, FRAME_MAGIC[0])
    view.setUint8(1, FRAME_MAGIC[1])
    view.setUint8(2, FRAME_VERSION)
    view.setUint8(3, FRAME_FLAG_ACK)
    view.setBigUint64(4, BigInt(ackedSeq), true)
    view.setUint32(12, sessionBytes.byteLength, true)
    new Uint8Array(buf, FRAME_HEADER_LEN).set(sessionBytes)
    return buf
  }

  /** 写入解析完成回调（TerminalPreview onWriteParsed 接线）：推进 ack 水位
   *
   * 语义：onWriteParsed 证明写入管线正在推进（有 write 被解析），此刻对
   * 已交付游标 last_rendered_seq 回发 ack。注意该游标是「已交付」边界，可能
   * 略超前于实际解析完成（writeQueue 中待写帧）——本地环回下这是可接受的
   * 保守近似（低估的余量 = writeQueue 本身，正是要钳制的目标）；真机数据
   * 若有偏差再收紧为逐帧确认 */
  function confirmWriteParsed() {
    if (stopped || !ws || ws.readyState !== WebSocket.OPEN) return
    if (lastRenderedSeq === null || ackedThroughSeq === lastRenderedSeq) return
    const now = Date.now()
    if (pendingAckBytes < ACK_BYTES_THRESHOLD && lastAckSentAt !== 0 && now - lastAckSentAt < ACK_MAX_IDLE_MS) {
      // 未到字节阈值也未到空闲兜底：挂起兜底计时器，后续批次或到点再回发
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
    ws.send(buildAckFrame(currentSession, lastRenderedSeq))
    ackedThroughSeq = lastRenderedSeq
    pendingAckBytes = 0
    lastAckSentAt = now
  }

  function closeWs() {
    if (ws) {
      ws.onopen = null
      ws.onmessage = null
      ws.onclose = null
      ws.onerror = null
      ws.close()
      ws = null
    }
    if (reconnectTimer) {
      clearTimeout(reconnectTimer)
      reconnectTimer = null
    }
  }

  function scheduleReconnect() {
    if (stopped || reconnectTimer || connecting) return
    const delay = Math.min(RECONNECT_BASE_MS * 2 ** reconnectAttempts, RECONNECT_MAX_MS)
    reconnectAttempts += 1
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null
      connect()
    }, delay)
  }

  /** 构造订阅消息（复用 WS 文本协议，token 为空——本地通道免 JWT；无参快照订阅） */
  function buildSubscribe(sessionId: string) {
    return {
      type: 'terminal',
      payload: {
        message_id: crypto.randomUUID(),
        expect_response: true,
        timestamp: Date.now(),
        session_id: sessionId,
        token: '',
        // 相邻标记格式（服务端 Message tag="type" + content="payload"）：
        // TerminalPayload{action} 内层再包一层内部标记的 TerminalAction
        payload: { action: { type: 'subscribe' } },
      },
    }
  }

  /** 解析 TB v2 二进制帧；非法帧返回 null（打印错误日志，不中断流） */
  function parseFrame(buffer: ArrayBuffer): OutputStreamFrame | null {
    if (buffer.byteLength < FRAME_HEADER_LEN) return null
    const view = new DataView(buffer)
    if (view.getUint8(0) !== FRAME_MAGIC[0] || view.getUint8(1) !== FRAME_MAGIC[1]) return null
    if (view.getUint8(2) !== FRAME_VERSION) return null
    const flags = view.getUint8(3)
    const isWaiting = (flags & FRAME_FLAG_WAITING) !== 0
    const eventCount = (flags >> FRAME_FLAG_COUNT_SHIFT) + 1
    const seq = Number(view.getBigUint64(4, true))
    const len = view.getUint32(12, true)
    if (FRAME_HEADER_LEN + len > buffer.byteLength) return null
    return {
      data: new Uint8Array(buffer, FRAME_HEADER_LEN, len),
      seq,
      eventCount,
      lastSeq: seq + eventCount - 1,
      isWaiting,
    }
  }

  /** 交付帧：重播去重（跳过 ≤ last_rendered_seq）→ 连续性校验（seq 缺口）→ 推进游标 */
  function deliverFrame(frame: OutputStreamFrame) {
    // 快照重订阅/断线重连后的重播去重：已渲染部分整帧跳过
    // （重播帧与已渲染帧字节完全一致，跳过不破坏终端状态）
    if (lastRenderedSeq !== null && frame.lastSeq <= lastRenderedSeq) return
    // 连续性内幕：首帧必须无缝衔接（直通模式帧末 + 1 = 下帧首 seq）
    if (lastRenderedSeq !== null && frame.seq > lastRenderedSeq + 1) {
      console.error(
        `[useTerminalOutputStream] seq gap: frame.start=${frame.seq}, last_rendered=${lastRenderedSeq}. Re-subscribing for snapshot`,
      )
      resubscribe()
      return
    }
    lastRenderedSeq = frame.lastSeq
    // 背压记账：交付字节累计（onData 消费后由 confirmWriteParsed 在写解析完成时回发 ack）
    pendingAckBytes += frame.data.byteLength
    options.onData(frame)
  }

  /** 快照重订阅：保留 last_rendered_seq，重播时跳过已渲染部分。
   *
   *  07 从增量重订阅（保字节游标续传）迁移：服务端快照协议恒全量重播
   *  [min_seq .. snapshot_seq]，前端按帧跳过 ≤ last_rendered_seq 的部分——
   *  无重复无遗漏（重播帧与已渲染帧内容一致），也免去服务端裁决三态。
   *  仅当 min_seq > last_rendered_seq + 1（已渲染区被环形淘汰）才清屏全量重播 */
  function resubscribe() {
    subscribed = false
    closeWs()
    reconnectAttempts = 0
    connect()
  }

  function handleControl(raw: string) {
    let msg: any
    try {
      msg = JSON.parse(raw)
    } catch {
      return
    }
    // 服务端消息为相邻标记格式：{"type":"terminal","payload":{...,"payload":{"action":{"type":...}}}}
    const action = msg?.payload?.payload?.action
    if (msg?.type === 'terminal' && action?.type === 'subscribe_response') {
      const snapshot: StreamSnapshot = {
        minSeq: action.min_seq ?? 0,
        snapshotSeq: action.max_seq ?? 0, // wire max_seq = 05 快照协议的 snapshot_seq
        historyCount: action.history_count ?? 0,
      }
      subscribed = true
      // 历史头部被环形淘汰：已渲染区域不可恢复 → 清屏全量重播
      if (lastRenderedSeq !== null && snapshot.minSeq > lastRenderedSeq + 1) {
        console.warn(
          `[useTerminalOutputStream] history truncated: min_seq=${snapshot.minSeq} > last_rendered=${lastRenderedSeq}+1, full replay`,
        )
        lastRenderedSeq = null
        options.onReset()
      }
      if (snapshot.minSeq > 0) {
        options.onTruncated?.(snapshot.minSeq)
      }
      // 排空订阅确认前缓冲的回放帧（按到达顺序写入，保持连续）
      const frames = pendingFrames
      pendingFrames = []
      pendingBytes = 0
      for (const frame of frames) {
        deliverFrame(frame)
      }
    } else if (msg?.type === 'error') {
      // 错误消息同样为相邻标记格式，code/message 在 payload 内
      const code = msg?.payload?.code
      const message = msg?.payload?.message
      console.error('[useTerminalOutputStream] server error:', code, message)
      if (code === 'SESSION_NOT_FOUND') {
        // 会话启动中或已停止：有限重试后停止，等待消费者恢复
        sessionMissingStrikes += 1
        if (sessionMissingStrikes >= MAX_SESSION_MISSING_STRIKES) {
          console.warn(
            `[useTerminalOutputStream] session ${currentSession} not found after ${MAX_SESSION_MISSING_STRIKES} attempts, stopping`,
          )
          stop()
          return
        }
      }
      // 关闭当前连接（服务端收到错误消息后通常保持连接），再退避重连——
      // 否则 connect() 会因 ws 非空而拒绝建立新连接
      closeWs()
      scheduleReconnect()
    }
  }

  async function connect() {
    if (stopped || ws || connecting) return
    connecting = true
    try {
      // 并行获取服务器端口与本地通道短期令牌
      const [status, token] = await Promise.all([
        invoke<{ port: number }>('get_server_status'),
        invoke<string>('get_local_ws_token'),
      ])
      if (stopped) return // await 期间被 stop
      const port = status.port || 8765
      // 令牌为一次性（服务端消费后即失效），每次连接重新签发
      const socket = new WebSocket(
        `ws://127.0.0.1:${port}/ws/terminal/local?token=${encodeURIComponent(token)}`,
      )
      socket.binaryType = 'arraybuffer'
      ws = socket
      subscribed = false
      pendingFrames = []
      pendingBytes = 0

      socket.onopen = () => {
        reconnectAttempts = 0
        // 握手成功即订阅（start 后由 subscribe() 打开 pendingSubscribe；
        // 断线重连场景 pendingSubscribe 仍为 true，自动恢复订阅）
        if (pendingSubscribe) {
          socket.send(JSON.stringify(buildSubscribe(currentSession)))
        }
      }
      socket.onmessage = (ev: MessageEvent) => {
        if (typeof ev.data === 'string') {
          handleControl(ev.data)
          return
        }
        const frame = parseFrame(ev.data as ArrayBuffer)
        if (!frame) {
          console.error('[useTerminalOutputStream] invalid binary frame received')
          return
        }
        if (!subscribed) {
          // 订阅确认前到达的回放帧：缓冲，确认后按序写入
          pendingFrames.push(frame)
          pendingBytes += frame.data.byteLength
          if (pendingBytes > MAX_PENDING_FRAME_BYTES) {
            console.error('[useTerminalOutputStream] pending frame overflow, re-subscribing')
            resubscribe()
          }
          return
        }
        deliverFrame(frame)
      }
      socket.onerror = () => {
        // onclose 统一处理重连
      }
      socket.onclose = () => {
        ws = null
        subscribed = false
        scheduleReconnect()
      }
    } catch (e) {
      console.warn('[useTerminalOutputStream] connect failed:', e)
      scheduleReconnect()
    } finally {
      connecting = false
    }
  }

  /** 建立新连接（只握手不订阅）；seq 游标重置——新会话坐标空间独立 */
  function start(sessionId: string) {
    if (!sessionId) return
    if (!stopped && ws && currentSession === sessionId) return // 已在运行
    closeWs()
    currentSession = sessionId
    lastRenderedSeq = null
    // 新会话坐标空间独立：ack 水位一并重置
    ackedThroughSeq = null
    pendingAckBytes = 0
    lastAckSentAt = 0
    stopped = false
    pendingSubscribe = false
    reconnectAttempts = 0
    sessionMissingStrikes = 0
    connect()
  }

  /** 发送订阅（terminal 就绪后调用）；连接未就绪时挂起，握手完成后自动发送 */
  function subscribe() {
    pendingSubscribe = true
    sessionMissingStrikes = 0
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(buildSubscribe(currentSession)))
    }
  }

  /** 停止（组件卸载 / 会话停止）；不再重连 */
  function stop() {
    stopped = true
    pendingSubscribe = false
    if (ackIdleTimer) {
      clearTimeout(ackIdleTimer)
      ackIdleTimer = null
    }
    closeWs()
    lastRenderedSeq = null
    ackedThroughSeq = null
    pendingAckBytes = 0
  }

  return { start, subscribe, stop, confirmWriteParsed }
}
