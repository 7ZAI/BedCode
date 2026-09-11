/**
 * 桌面端本地 WS 输出流 Composable（快照模型，07 迁移 + TB v3 字节化）
 *
 * 通过本地环回 WebSocket（/ws/terminal/local）以 TB v3 二进制帧直取 PTY 原始字节。
 *
 * 快照模型（spec §5.2/§5.3 + `.scratch/pty-byte-history/spec.md` 字节化）：
 * - 帧头 16B：magic "TB" + version=3 + flags + start_offset(8 LE) + len(4 LE)
 *   end_offset = start_offset + len 直接可导；高 7 位不再编码事件数
 * - 订阅响应（旧路由 wire 字段名不变，值承载字节语义）：min_seq = min_offset、
 *   max_seq = snapshot_offset、history_count = history_bytes
 * - 历史段 = [min_offset .. snapshot_offset] → history_end → 实时段；本地通道
 *   零缓冲直通，每事件一帧，字节区间严格连续铺满
 *
 * 恢复模型（字节游标）：
 * - last_rendered_offset（替代 last_rendered_seq）：已渲染到的帧末 endOffset，跨重连保留
 * - 重订阅（缺口/断线）＝快照重订阅：重播时按字节区间去重 + 跨帧裁剪
 * - 截断判定：min_offset > last_rendered_offset 说明已渲染区域被环形淘汰 → 清屏全量重播
 * - 连续性内幕：offset 缺口（frame.start_offset > last_rendered_offset）→ 快照重订阅；
 *   直通模式无合并，正常流恒连续，缺口即丢帧
 * - 跨帧裁剪：overlap = cursor - frame.start_offset，渲染 data[overlap..]
 *   （根治「重播帧跨游标 → 整帧重渲染」的重复输出缺陷）
 *
 * 生命周期（显式控制，terminal 就绪是订阅的前置条件）：
 * - start(sessionId)：断开旧连接并建立新连接（只握手，不订阅；字节游标重置）
 * - subscribe()：发送订阅消息（terminal 就绪后调用；WS 断线重连后自动重发）
 * - stop()：断开并停止重连（组件卸载 / 会话停止）
 *
 * 会话停止（SESSION_NOT_FOUND）时重试有限次数后停止，避免无限重连；
 * 会话重新启动后由消费者再次 start() + subscribe() 恢复。
 */

import { invoke } from '@tauri-apps/api/core'
import { logger } from '@/utils/frontendLogger'

/** TB v3 单帧解析结果 */
export interface OutputStreamFrame {
  data: Uint8Array
  /** 帧内首字节的会话内累计偏移 */
  startOffset: number
  /** 帧内末字节偏移 = startOffset + len（游标推进基准） */
  endOffset: number
  isWaiting: boolean
}

/** 快照订阅元数据（旧路由 wire min_seq/max_seq/history_count 承载字节语义值） */
export interface StreamSnapshot {
  /** 队列最早存续字节位置（环形淘汰后推进；> 游标表示历史头部被截断） */
  minOffset: number
  /** 订阅时刻累计字节数（历史边界） */
  snapshotOffset: number
  /** 驻留历史总字节数 */
  historyBytes: number
}

export interface TerminalStreamOptions {
  /** 连续性校验/去重通过后的原始字节帧，直接入写入管线 */
  onData: (frame: OutputStreamFrame) => void
  /** 历史截断需清屏全量重播（已渲染区域被环形淘汰）时清屏；回放随后到达 */
  onReset: () => void
  /** 环形保留区间头部被淘汰（min_offset > 0）时提示，用于"历史被截断"文案 */
  onTruncated?: (minOffset: number) => void
}

// 帧头 16 字节：magic(2) + version(1) + flags(1) + start_offset(8 LE) + len(4 LE)
const FRAME_HEADER_LEN = 16
const FRAME_MAGIC = [0x54, 0x42] // "TB"
const FRAME_VERSION = 3
const FRAME_FLAG_WAITING = 0x01
// 背压 ack 标志位（仅客户端→服务端方向使用；服务端→客户端帧的 flags bit0
// 是 WAITING 位，与服务端只认入站二进制帧作 ack 的解析互不冲突）
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
  let lastRenderedOffset: number | null = null
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
  // offset gap 处理：任何缺口立即触发快照重订阅补回缺失字节（缺失帧字节无法从
  // 实时流恢复，跳过渲染会把残缺序列写进 buffer → parser 报错 + 字面残渣）。
  // 冷却防风暴：WebKitGTK WS 缓冲在输出风暴期可能丢弃整消息（单帧缺口），
  // 若每次缺口都重连会连环重订阅；冷却期内缺口帧不渲染、不推进游标（保持
  // lastRenderedOffset），冷却结束重订阅后由全量重播一次性补回全部缺失字节。
  let lastGapResubscribeAt = 0
  const GAP_RESUBSCRIBE_COOLDOWN_MS = 3000

  // ==================== 背压 ack（渲染解析反馈环，spec 04-06） ====================
  // 写入解析完成（TerminalPreview onWriteParsed）后回发 ack 帧携已渲染到的
  // last_rendered_offset；服务端据此暂停/恢复 PTY 读取，渲染速度反向钳制源头流速。
  // 节流：字节阈值（风暴批发）+ 空闲兜底（低频输出也最终 ack），避免逐帧刷屏
  let ackedThroughOffset: number | null = null
  let pendingAckBytes = 0
  let lastAckSentAt = 0
  let ackIdleTimer: ReturnType<typeof setTimeout> | null = null

  /** 构造背压 ack 帧（TB v3 头 + ACK 标志位 + acked_offset + session_id 负载） */
  function buildAckFrame(sessionId: string, ackedOffset: number): ArrayBuffer {
    const sessionBytes = new TextEncoder().encode(sessionId)
    const buf = new ArrayBuffer(FRAME_HEADER_LEN + sessionBytes.byteLength)
    const view = new DataView(buf)
    view.setUint8(0, FRAME_MAGIC[0])
    view.setUint8(1, FRAME_MAGIC[1])
    view.setUint8(2, FRAME_VERSION)
    view.setUint8(3, FRAME_FLAG_ACK)
    view.setBigUint64(4, BigInt(ackedOffset), true)
    view.setUint32(12, sessionBytes.byteLength, true)
    new Uint8Array(buf, FRAME_HEADER_LEN).set(sessionBytes)
    return buf
  }

  /** 写入解析完成回调（TerminalPreview onWriteParsed 接线）：推进 ack 水位
   *
   * 语义：onWriteParsed 证明写入管线正在推进（有 write 被解析），此刻对
   * 已交付游标 last_rendered_offset 回发 ack。注意该游标是「已交付」边界，可能
   * 略超前于实际解析完成（writeQueue 中待写帧）——本地环回下这是可接受的
   * 保守近似（低估的余量 = writeQueue 本身，正是要钳制的目标）；真机数据
   * 若有偏差再收紧为逐帧确认 */
  function confirmWriteParsed() {
    if (stopped || !ws || ws.readyState !== WebSocket.OPEN) return
    if (lastRenderedOffset === null || ackedThroughOffset === lastRenderedOffset) return
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
    ws.send(buildAckFrame(currentSession, lastRenderedOffset))
    ackedThroughOffset = lastRenderedOffset
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

  /** 解析 TB v3 二进制消息内全部帧。一个 WS message 允许串联多个 TB v3 帧
   * （服务端有界合并策略会产生多帧串联），逐个解析避免只取首帧丢尾帧。
   * 非法帧（魔数/版本/长度越界）截断解析，不中断流；返回已解析帧列表。
   */
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
   * 切片起点恒合法，不再整帧重渲染
   */
  function deliverFrame(frame: OutputStreamFrame) {
    // 快照重订阅/断线重连后的重播去重：已渲染区间整帧跳过
    if (lastRenderedOffset !== null && frame.endOffset <= lastRenderedOffset) return
    // 连续性内幕：首帧必须无缝衔接（直通模式字节区间严格连续铺满）
    if (lastRenderedOffset !== null && frame.startOffset > lastRenderedOffset) {
      // 缺口 = 字节永久缺失（服务端无丢弃日志，疑似 WebKitGTK WS 缓冲风暴溢出
      // 丢整消息）。正确做法：重订阅让服务端全量重播，跳过已渲染部分后缺失
      // 字节自然补回——而不是跳过缺失继续渲染（会把残缺序列写进 buffer 变成
      // 屏幕残渣）。带冷却：冷却期内缺口帧不渲染不推进游标，等待重播补回
      const now = Date.now()
      if (now - lastGapResubscribeAt >= GAP_RESUBSCRIBE_COOLDOWN_MS) {
        lastGapResubscribeAt = now
        logger.warn(
          `[useTerminalOutputStream] offset gap, re-subscribing for snapshot (frame.start=${frame.startOffset}, last_rendered=${lastRenderedOffset})`,
        )
        resubscribe()
        return
      }
      // 冷却期内：跳过缺口帧（不渲染、不推进游标）——后续帧同样跳过，
      // 冷却结束由全量重播一次性补回；避免把残缺字节写进终端
      return
    }
    // 跨帧裁剪：帧覆盖已渲染游标（重播首帧跨游标）→ 裁掉前半段，零重复
    const cursor = lastRenderedOffset ?? frame.startOffset
    if (cursor > frame.startOffset) {
      const overlap = cursor - frame.startOffset
      frame = { ...frame, data: frame.data.subarray(overlap) }
    }
    lastRenderedOffset = frame.endOffset
    // 背压记账：交付字节累计（onData 消费后由 confirmWriteParsed 在写解析完成时回发 ack）
    pendingAckBytes += frame.data.byteLength
    options.onData(frame)
  }

  /** 快照重订阅：保留 last_rendered_offset，重播时按字节区间去重 + 跨帧裁剪。
   *
   *  快照协议恒全量重播 [min_offset .. snapshot_offset]，前端按帧跳过
   *  已渲染区间（end_offset ≤ last_rendered_offset）的部分——无重复无遗漏
   *  （重播帧与已渲染帧内容一致），跨帧首段由 overlap 字节裁剪。
   *  仅当 min_offset > last_rendered_offset（已渲染区被环形淘汰）才清屏全量重播 */
  function resubscribe() {
    subscribed = false
    // 重订阅本身是完整自愈：冷却从此刻重新起算（避免重播补回期间又被后续
    // 缺口连环触发重订阅风暴）
    lastGapResubscribeAt = Date.now()
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
      // 旧路由 wire 字段名不变，值承载字节语义：min_seq = min_offset、
      // max_seq = snapshot_offset、history_count = history_bytes
      const snapshot: StreamSnapshot = {
        minOffset: action.min_seq ?? 0,
        snapshotOffset: action.max_seq ?? 0,
        historyBytes: action.history_count ?? 0,
      }
      subscribed = true
      // 历史头部被环形淘汰：已渲染区域不可恢复 → 清屏全量重播
      if (lastRenderedOffset !== null && snapshot.minOffset > lastRenderedOffset) {
        logger.warn(
          `[useTerminalOutputStream] history truncated: min_offset=${snapshot.minOffset} > last_rendered=${lastRenderedOffset}, full replay`,
        )
        lastRenderedOffset = null
        options.onReset()
      }
      if (snapshot.minOffset > 0) {
        options.onTruncated?.(snapshot.minOffset)
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
      logger.error('[useTerminalOutputStream] server error:', code, message)
      if (code === 'SESSION_NOT_FOUND') {
        // 会话启动中或已停止：有限重试后停止，等待消费者恢复
        sessionMissingStrikes += 1
        if (sessionMissingStrikes >= MAX_SESSION_MISSING_STRIKES) {
          logger.warn(
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
        // 一个 WS message 可能串联多个 TB v3 帧（服务端合并策略），
        // 逐帧解析交付；空/全非法返回空数组时下方按无帧处理
        const frames = parseFrames(ev.data as ArrayBuffer)
        if (frames.length === 0) {
          logger.error('[useTerminalOutputStream] invalid binary frame received')
          return
        }
        for (const frame of frames) {
          if (!subscribed) {
            // 订阅确认前到达的回放帧：缓冲，确认后按序写入
            pendingFrames.push(frame)
            pendingBytes += frame.data.byteLength
            if (pendingBytes > MAX_PENDING_FRAME_BYTES) {
              logger.error('[useTerminalOutputStream] pending frame overflow, re-subscribing')
              resubscribe()
            }
            continue
          }
          deliverFrame(frame)
        }
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
      logger.warn('[useTerminalOutputStream] connect failed:', e)
      scheduleReconnect()
    } finally {
      connecting = false
    }
  }

  /** 建立新连接（只握手不订阅）；字节游标重置——新会话坐标空间独立 */
  function start(sessionId: string) {
    if (!sessionId) return
    if (!stopped && ws && currentSession === sessionId) return // 已在运行
    closeWs()
    currentSession = sessionId
    lastRenderedOffset = null
    // 新会话坐标空间独立：ack 水位一并重置
    ackedThroughOffset = null
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
    lastRenderedOffset = null
    ackedThroughOffset = null
    pendingAckBytes = 0
  }

  return { start, subscribe, stop, confirmWriteParsed }
}
