/**
 * Terminal Socket Composable
 *
 * 每会话终端 WS 直连（10 号票：移动端前端直连桌面端终端会话路由）。
 * 职责边界：连接生命周期 + 控制帧收发 + TB v2 二进制帧解析。
 * 状态机（订阅拼接/去重/重连语义）由 terminalBuffer store 驱动。
 *
 * 协议（桌面端 /ws/terminal/session/{id}，spec §5.3）：
 * - 首消息 {"type":"auth","token":JWT}（token 经 get_terminal_ws_info 获取，勿落前端存储）
 * - 订阅 {"type":"subscribe"}（无参快照订阅，连接创建即绑定会话）
 * - 输入 {"type":"input","data":Base64,"special_key":可选}
 * - 服务端控制帧：auth_ok / subscribe_ok{snapshot_seq,min_seq,history_count}
 *   / history_end{snapshot_seq} / session_stopped{session_id} / error{code,message}
 * - 输出为 TB v2 二进制帧：16B 头（magic "TB" + version=2 + flags + seq(8 LE) + len(4 LE)），
 *   seq = 帧内首事件 index，flags 高 7 位 = 事件数-1，帧末 seq = seq + count - 1
 */

import { getTerminalWsInfo } from '@/composables/useMobileCommands'
import {
  deriveWsSession,
  generateEphemeral,
  parseCryptoEcho,
  type EphemeralKeyPair,
  type WsSessionCrypto,
} from '@/services/linkCrypto'
import { getPinnedKey, isChannelEncryptionActive, useLinkEncryptionSettings } from './useLinkEncryption'

// ==================== Types ====================

/** TB v2 单帧解析结果 */
export interface TerminalSocketFrame {
  data: Uint8Array
  /** 帧内首事件 seq */
  seq: number
  /** 帧内事件数（flags 高 7 位 + 1） */
  eventCount: number
  /** 帧末 seq = seq + eventCount - 1（游标推进基准） */
  lastSeq: number
  isWaiting: boolean
}

/** 服务端控制帧（订阅元数据等） */
export interface SubscribeOkInfo {
  /** 订阅时刻队列最新序号（历史边界；历史帧 lastSeq ≤ 该值） */
  snapshotSeq: number
  /** 队列最早存续事件序号（环形淘汰后推进；> lastRenderedSeq+1 表示头部截断） */
  minSeq: number
  historyCount: number
}

/** 控制帧回调集 */
export interface TerminalSocketHandlers {
  /** auth_ok 后（连接已认证，可发 subscribe） */
  onAuthed: () => void
  /** subscribe_ok（快照元数据） */
  onSubscribed: (info: SubscribeOkInfo) => void
  /** history_end（历史段结束，此后为实时帧） */
  onHistoryEnd: (snapshotSeq: number) => void
  /** 二进制输出帧 */
  onFrame: (frame: TerminalSocketFrame) => void
  /** 服务端错误帧 */
  onError: (code: string, message: string) => void
  /** 会话停止通知 */
  onSessionStopped: (sessionId: string) => void
  /** 连接关闭（onclose，重连决策由调用方做） */
  onClose: () => void
}

// ==================== Constants ====================

// 帧头 16 字节：magic(2) + version(1) + flags(1) + seq(8 LE) + len(4 LE)
const FRAME_HEADER_LEN = 16
const FRAME_MAGIC = [0x54, 0x42] // "TB"
const FRAME_VERSION = 2
const FRAME_FLAG_WAITING = 0x01
const FRAME_FLAG_COUNT_SHIFT = 1
// 背压 ack 标志位（仅客户端→服务端方向使用；服务端→客户端帧的 flags 低 2 位
// 是 WAITING + 事件数编码，与服务端只认入站二进制帧作 ack 的解析互不冲突）
const FRAME_FLAG_ACK = 0x02

// ack 节流（对齐桌面端 useTerminalOutputStream）：累计待 ack 字节达阈值即
// 回发（对齐上游 WATERMARK 节奏，风暴批发）；空闲超时强制回发（低频输出
// 不滞留记账，服务端 unacked_bytes 不虚高）
const ACK_BYTES_THRESHOLD = 64 * 1024
const ACK_MAX_IDLE_MS = 250

// ==================== Frame Parsing ====================

/** 解析 TB v2 二进制帧；非法帧返回 null（打印错误日志，不中断流） */
export function parseFrame(buffer: ArrayBuffer): TerminalSocketFrame | null {
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

// ==================== Socket ====================

export interface TerminalSocket {
  /** 建立连接（幂等：已连接/连接中忽略）；sessionId 变化时先 stop 再 start */
  start(sessionId: string): void
  /** 发送订阅帧（连接未就绪时挂起，握手成功后自动发送） */
  subscribe(): void
  /** 发送输入帧（data 为原始字符串，内部 Base64 编码；specialKey 为按键组合名） */
  sendInput(data: string, specialKey?: string): void
  /** 停止（不再重连）；调用方负责后续清理 */
  stop(): void
  /** 断开当前连接并按退避重连（错误帧恢复路径） */
  reconnect(): void
  /** 当前是否已建立连接 */
  isOpen(): boolean
  /**
   * 渲染背压 ack（对齐桌面端 confirmWriteParsed）：写入解析完成后调用，
   * 按 64KB 阈值 + 250ms 空闲节流回发 TB v2 ACK 帧。仅本端为会话正统
   * 渲染端时调用（服务端也会丢弃非正统端的 ack，双保险）
   */
  ackRendered(): void
}

/**
 * 创建单会话终端 socket（每会话一个实例，store 持有）
 *
 * 重连退避：500ms → 8s 封顶。认证失败/会话不存在等错误由 onError 回调
 * 交由调用方决策（有限重试 vs 停止），socket 自身不自动重连错误帧场景
 */
export function createTerminalSocket(handlers: TerminalSocketHandlers): TerminalSocket {
  let ws: WebSocket | null = null
  let connecting = false
  let currentSession = ''
  let stopped = true
  let pendingSubscribe = false
  // 链路加密（issue 07）：握手前的临时密钥对与派生后的会话密码；
  // 重连/换会话即置空重建（序号随新连接归零）
  let wsEphemeral: EphemeralKeyPair | null = null
  let wsCrypto: WsSessionCrypto | null = null
  let reconnectAttempts = 0
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null

  // ==================== 渲染背压 ack 状态（spec 04-06，对齐桌面端） ====================
  // lastRenderedSeq：已交付帧末 seq（帧到达即更新，写入管线消费中）；
  // ackRendered() 由视图 onWriteParsed 触发，节流回发（阈值/空闲兜底）
  let lastRenderedSeq: number | null = null
  let ackedThroughSeq: number | null = null
  let pendingAckBytes = 0
  let lastAckSentAt = 0
  let ackIdleTimer: ReturnType<typeof setTimeout> | null = null

  // 重连退避（ms）：500 → 1000 → 2000 → 4000 → 8000 封顶
  const RECONNECT_BASE_MS = 500
  const RECONNECT_MAX_MS = 8000

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

  /** 构建背压 ack 帧（与服务端 control_frame::parse_ack_frame 布局一致） */
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

  /**
   * 写入解析完成 ack（视图 onWriteParsed 接线）：推进 ack 水位
   *
   * 语义：onWriteParsed 证明写入管线正在推进（有 write 被解析），此刻对
   * 已交付游标 last_rendered_seq 回发 ack（与桌面端一致的保守近似）。
   * 节流：64KB 阈值批发回发 + 250ms 空闲兜底，避免高频逐帧 ack。
   */
  function ackRendered() {
    if (stopped || !ws || ws.readyState !== WebSocket.OPEN) return
    if (lastRenderedSeq === null || ackedThroughSeq === lastRenderedSeq) return
    const now = Date.now()
    if (pendingAckBytes < ACK_BYTES_THRESHOLD && lastAckSentAt !== 0 && now - lastAckSentAt < ACK_MAX_IDLE_MS) {
      // 未到字节阈值也未到空闲兜底：挂起兜底计时器，后续批次或到点再回发
      if (!ackIdleTimer) {
        ackIdleTimer = setTimeout(() => {
          ackIdleTimer = null
          ackRendered()
        }, ACK_MAX_IDLE_MS)
      }
      return
    }
    if (ackIdleTimer) {
      clearTimeout(ackIdleTimer)
      ackIdleTimer = null
    }
    const ackFrame = new Uint8Array(buildAckFrame(currentSession, lastRenderedSeq))
    ws.send(wsCrypto ? wsCrypto.encryptBinary('ws-terminal', ackFrame) : ackFrame)
    ackedThroughSeq = lastRenderedSeq
    pendingAckBytes = 0
    lastAckSentAt = now
  }

  function scheduleReconnect() {
    if (stopped || reconnectTimer || connecting) return
    const delay = Math.min(RECONNECT_BASE_MS * 2 ** reconnectAttempts, RECONNECT_MAX_MS)
    reconnectAttempts += 1
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null
      start(currentSession)
    }, delay)
  }

  function handleControl(raw: string) {
    let msg: any
    try {
      msg = JSON.parse(raw)
    } catch {
      return
    }
    switch (msg?.type) {
      case 'auth_ok':
        consumePendingHandshake(msg)
        handlers.onAuthed()
        if (pendingSubscribe) {
          sendControlJson(JSON.stringify({ type: 'subscribe' }))
        }
        break
      case 'subscribe_ok':
        handlers.onSubscribed({
          snapshotSeq: msg.snapshot_seq ?? 0,
          minSeq: msg.min_seq ?? 0,
          historyCount: msg.history_count ?? 0,
        })
        break
      case 'history_end':
        handlers.onHistoryEnd(msg.snapshot_seq ?? 0)
        break
      case 'session_stopped':
        handlers.onSessionStopped(msg.session_id ?? '')
        break
      case 'error':
        handlers.onError(msg.code ?? 'UNKNOWN', msg.message ?? '')
        break
      default:
        console.warn('[useTerminalSocket] unknown control frame:', msg?.type)
    }
  }

  async function start(sessionId: string) {
    if (!sessionId) return
    if (!stopped && ws && currentSession === sessionId) return // 已在运行
    if (stopped || currentSession !== sessionId) {
      // 新会话或 stop 后重启：重置状态
      closeWs()
      currentSession = sessionId
      stopped = false
      pendingSubscribe = false
      reconnectAttempts = 0
      // 链路加密状态随连接重建（重连即新握手新序号）
      wsCrypto = null
      wsEphemeral = null
      // 重置渲染背压游标（新会话 seq 重新从历史快照开始）
      lastRenderedSeq = null
      ackedThroughSeq = null
      pendingAckBytes = 0
      lastAckSentAt = 0
      if (ackIdleTimer) {
        clearTimeout(ackIdleTimer)
        ackIdleTimer = null
      }
    }
    if (connecting) return
    connecting = true
    try {
      const info = await getTerminalWsInfo(sessionId)
      if (stopped) return // await 期间被 stop
      const socket = new WebSocket(info.url)
      socket.binaryType = 'arraybuffer'
      ws = socket

      socket.onopen = () => {
        reconnectAttempts = 0
        // 握手成功即认证（JWT 首消息；重连场景复用同一 JWT——桌面端仅验签+有效期）；
        // 已 pin 且开关开 → 附带加密协商提案（issue 07，auth_ok 回带服务端临时公钥）
        const pinnedKey = isChannelEncryptionActive('ws') ? getPinnedKey() : null
        if (pinnedKey) {
          wsEphemeral = generateEphemeral()
          socket.send(
            JSON.stringify({ type: 'auth', token: info.token, crypto: { v: 1, ek: wsEphemeral.pubB64 } }),
          )
        } else {
          socket.send(JSON.stringify({ type: 'auth', token: info.token }))
        }
        // 订阅在 auth_ok 后发送（pendingSubscribe 由 subscribe() 打开；
        // 重连场景 pendingSubscribe 仍为 true，自动恢复订阅）
      }
      socket.onmessage = (ev: MessageEvent) => {
        if (typeof ev.data === 'string') {
          let text = ev.data as string
          if (wsCrypto) {
            try {
              text = wsCrypto.decryptText('ws-terminal', text)
            } catch (e: any) {
              console.error('[useTerminalSocket] decrypt control frame failed:', e?.message || e)
              handlers.onError('LINK_CRYPTO_DECRYPT_FAILED', String(e?.message || e))
              socket.close(4003, 'decrypt failed')
              return
            }
          }
          handleControl(text)
          return
        }
        // 解密后可能被替换为 @noble 返回的缓冲（ArrayBufferLike），标注宽泛型
        let raw: Uint8Array<ArrayBufferLike> = new Uint8Array(ev.data as ArrayBuffer)
        if (wsCrypto) {
          try {
            raw = wsCrypto.decryptBinary('ws-terminal', raw)
          } catch (e: any) {
            console.error('[useTerminalSocket] decrypt binary frame failed:', e?.message || e)
            handlers.onError('LINK_CRYPTO_DECRYPT_FAILED', String(e?.message || e))
            socket.close(4003, 'decrypt failed')
            return
          }
        }
        const frame = parseFrame(raw.buffer as ArrayBuffer)
        if (!frame) {
          console.error('[useTerminalSocket] invalid binary frame received')
          return
        }
        // 渲染背压游标：帧末 seq 即已交付边界（与桌面端语义一致）
        lastRenderedSeq = Math.max(lastRenderedSeq ?? 0, frame.lastSeq)
        pendingAckBytes += frame.data.byteLength
        handlers.onFrame(frame)
      }
      socket.onerror = () => {
        // onclose 统一处理重连
      }
      socket.onclose = () => {
        ws = null
        wsCrypto = null
        wsEphemeral = null
        handlers.onClose()
        scheduleReconnect()
      }
    } catch (e) {
      console.warn('[useTerminalSocket] connect failed:', e)
      scheduleReconnect()
    } finally {
      connecting = false
    }
  }

  /**
   * 消费 auth_ok 的加密协商回执（issue 07）：
   * 我方已提案且服务端回带临时公钥 → 派生会话密码，此后帧全加密；
   * 提案被拒（无回执）→ strict 断连报错，非 strict 明文续跑 + warn。
   */
  function consumePendingHandshake(authOkMsg: Record<string, unknown>) {
    if (!wsEphemeral) return
    const ephemeral = wsEphemeral
    wsEphemeral = null // 私钥即用即弃（无论协商成败都不复用）
    const echo = parseCryptoEcho(authOkMsg as { crypto?: { v?: number; ek?: string } })
    const pinnedKey = getPinnedKey()
    if (echo && pinnedKey) {
      try {
        wsCrypto = deriveWsSession(ephemeral.priv, ephemeral.pubB64, echo.ek, pinnedKey)
        console.log('[useTerminalSocket] link encryption negotiated, frames encrypted')
        return
      } catch (e: any) {
        console.error('[useTerminalSocket] ws handshake derive failed:', e?.message || e)
        wsCrypto = null
      }
    }
    const { settings } = useLinkEncryptionSettings()
    if (settings.value.strictMode) {
      console.error('[useTerminalSocket] strict mode: server did not accept encryption, closing')
      handlers.onError('LINK_ENCRYPTION_DOWNGRADE', 'server did not accept encryption')
      ws?.close(4003, 'encryption downgrade')
    } else {
      console.warn('[useTerminalSocket] connection stays plaintext (downgrade tolerated)')
    }
  }

  /** 控制帧发送统一出口：加密会话中自动信封化 */
  function sendControlJson(json: string) {
    if (!ws || ws.readyState !== WebSocket.OPEN) return
    ws.send(wsCrypto ? wsCrypto.encryptText('ws-terminal', json) : json)
  }

  function subscribe() {
    pendingSubscribe = true
    if (ws && ws.readyState === WebSocket.OPEN) {
      sendControlJson(JSON.stringify({ type: 'subscribe' }))
    }
  }

  function sendInput(data: string, specialKey?: string) {
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      console.warn('[useTerminalSocket] sendInput: socket not open')
      return
    }
    sendControlJson(
      JSON.stringify({
        type: 'input',
        data: utf8ToBase64(data),
        special_key: specialKey ?? null,
      }),
    )
  }

  function stop() {
    stopped = true
    pendingSubscribe = false
    closeWs()
  }

  function reconnect() {
    if (stopped) return
    closeWs()
    scheduleReconnect()
  }

  function isOpen() {
    return !!ws && ws.readyState === WebSocket.OPEN
  }

  return { start, subscribe, sendInput, stop, reconnect, isOpen, ackRendered }
}

// ==================== Utility ====================

/** UTF-8 安全 Base64 编码（输入帧协议要求） */
export function utf8ToBase64(text: string): string {
  const bytes = new TextEncoder().encode(text)
  let binary = ''
  const chunk = 0x8000
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk))
  }
  return btoa(binary)
}
