/**
 * 桌面端本地 WS 二进制输出流 Composable
 *
 * 通过本地环回 WebSocket（/ws/terminal/local）以二进制帧直取 PTY 原始字节，
 * 替代"Tauri event（base64 JSON）+ invoke 历史拉取"的双通道模式。
 *
 * 连续性模型（服务端契约）：
 * - 游标 = 已渲染到的字节偏移，跨重连保留
 * - 服务端裁决 mode：incremental（从游标字节级裁剪续传）/ reset（清屏全量重播）
 * - 每帧携带 [startOffset, endOffset)，客户端校验 startOffset === cursor，
 *   不满足即违反不变量 → 报错并按 reset 重订阅（幂等，正常路径不触发）
 *
 * 生命周期（显式控制，terminal 就绪是订阅的前置条件）：
 * - start(sessionId)：断开旧连接并建立新连接（只握手，不订阅；游标重置）
 * - subscribe()：发送订阅消息（terminal 就绪后调用；WS 断线重连后自动重发）
 * - stop()：断开并停止重连（组件卸载 / 会话停止）
 *
 * 会话停止（SESSION_NOT_FOUND）时重试有限次数后停止，避免无限重连；
 * 会话重新启动后由消费者再次 start() + subscribe() 恢复。
 */

import { invoke } from '@tauri-apps/api/core'

/** 单帧解析结果 */
export interface OutputStreamFrame {
  data: Uint8Array
  startOffset: number
  endOffset: number
  isWaiting: boolean
}

/** 订阅裁决（服务端告知，消费者零猜测） */
export interface StreamSubscribeResult {
  mode: 'incremental' | 'reset'
  minOffset: number
  maxOffset: number
  historyCount: number
}

export interface TerminalStreamOptions {
  /** 连续性校验通过后回调（数据已与游标对齐，直接入写入管线） */
  onData: (frame: OutputStreamFrame) => void
  /** mode=reset 时在回放帧到达前调用（消费者应清屏，回放从 minOffset 起重播） */
  onReset: (result: StreamSubscribeResult) => void
  /** 环形保留区间头部被淘汰（min_offset > 0）时提示，用于"历史被截断"文案 */
  onTruncated?: (minOffset: number) => void
}

// 帧头 20 字节：magic(2) + version(1) + flags(1) + start_offset(8 LE) + end_offset(8 LE)
const FRAME_HEADER_LEN = 20
const FRAME_MAGIC = [0x54, 0x42] // "TB"
const FRAME_VERSION = 1
const FRAME_FLAG_WAITING = 0x01

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
  let cursor: number | null = null
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

  /** 构造订阅消息（复用 WS 文本协议，token 为空——本地通道免 JWT） */
  function buildSubscribe(sessionId: string, c: number | null) {
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
        payload: { action: { type: 'subscribe', start_seq: c } },
      },
    }
  }

  /** 解析二进制帧；非法帧返回 null（打印错误日志，不中断流） */
  function parseFrame(buffer: ArrayBuffer): OutputStreamFrame | null {
    if (buffer.byteLength < FRAME_HEADER_LEN) return null
    const view = new DataView(buffer)
    if (view.getUint8(0) !== FRAME_MAGIC[0] || view.getUint8(1) !== FRAME_MAGIC[1]) return null
    if (view.getUint8(2) !== FRAME_VERSION) return null
    const isWaiting = (view.getUint8(3) & FRAME_FLAG_WAITING) !== 0
    const startOffset = Number(view.getBigUint64(4, true))
    const endOffset = Number(view.getBigUint64(12, true))
    return {
      data: new Uint8Array(buffer, FRAME_HEADER_LEN),
      startOffset,
      endOffset,
      isWaiting,
    }
  }

  /** 交付帧：先做连续性校验，再推进游标并回调 */
  function deliverFrame(frame: OutputStreamFrame) {
    // 不变量：游标之后的首帧必须无缝衔接（字节级连续，无重无漏）
    if (cursor !== null && frame.startOffset !== cursor) {
      console.error(
        `[useTerminalOutputStream] continuity violation: frame.start=${frame.startOffset}, cursor=${cursor}. Re-subscribing from cursor`
      )
      forceResubscribe()
      return
    }
    cursor = frame.endOffset
    options.onData(frame)
  }

  /** 连续性不变量被破坏：保留当前游标，按增量语义重订阅（服务端裁决补缺口）。
   *
   *  关键：不能把 cursor 置 null——置 null 会让服务端裁决 mode=Reset，触发环形
   *  缓冲全量重播（长时间会话可达数万事件/数十 MB）。大历史重播期间 PTY 新输出
   *  继续产生新缺口（订阅通道背压丢事件）→ 反复全量重播 → 重订阅风暴自持循环
   *  （终端反复清屏滚动，永不停止）。保留游标 → 服务端返回 incremental，只补
   *  缺口之后的少量数据，游标逐轮推进，收敛后自然恢复。
   *  仅当服务端判定游标已失效（mode=Reset）时才清屏全量重播（handleControl 处理） */
  function forceResubscribe() {
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
      const result: StreamSubscribeResult = {
        mode: action.mode === 'incremental' ? 'incremental' : 'reset',
        minOffset: action.min_offset ?? 0,
        maxOffset: action.max_offset ?? 0,
        historyCount: action.history_count ?? 0,
      }
      subscribed = true
      if (result.mode === 'reset') {
        // 重置游标：回放帧从 minOffset 起（服务端保证帧内偏移起点）
        cursor = null
        options.onReset(result)
      }
      if (result.minOffset > 0) {
        options.onTruncated?.(result.minOffset)
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
            `[useTerminalOutputStream] session ${currentSession} not found after ${MAX_SESSION_MISSING_STRIKES} attempts, stopping`
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
      const socket = new WebSocket(`ws://127.0.0.1:${port}/ws/terminal/local?token=${encodeURIComponent(token)}`)
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
          socket.send(JSON.stringify(buildSubscribe(currentSession, cursor)))
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
            console.error('[useTerminalOutputStream] pending frame overflow, re-subscribing from cursor')
            forceResubscribe()
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

  /** 建立新连接（只握手不订阅）；游标重置——新会话坐标空间独立 */
  function start(sessionId: string) {
    if (!sessionId) return
    if (!stopped && ws && currentSession === sessionId) return // 已在运行
    closeWs()
    currentSession = sessionId
    cursor = null
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
      ws.send(JSON.stringify(buildSubscribe(currentSession, cursor)))
    }
  }

  /** 停止（组件卸载 / 会话停止）；不再重连 */
  function stop() {
    stopped = true
    pendingSubscribe = false
    closeWs()
    cursor = null
  }

  return { start, subscribe, stop }
}
