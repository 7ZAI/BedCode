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
  let reconnectAttempts = 0
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null

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
        handlers.onAuthed()
        if (pendingSubscribe) {
          ws?.send(JSON.stringify({ type: 'subscribe' }))
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
        // 握手成功即认证（JWT 首消息；重连场景复用同一 JWT——桌面端仅验签+有效期）
        socket.send(JSON.stringify({ type: 'auth', token: info.token }))
        // 订阅在 auth_ok 后发送（pendingSubscribe 由 subscribe() 打开；
        // 重连场景 pendingSubscribe 仍为 true，自动恢复订阅）
      }
      socket.onmessage = (ev: MessageEvent) => {
        if (typeof ev.data === 'string') {
          handleControl(ev.data)
          return
        }
        const frame = parseFrame(ev.data as ArrayBuffer)
        if (!frame) {
          console.error('[useTerminalSocket] invalid binary frame received')
          return
        }
        handlers.onFrame(frame)
      }
      socket.onerror = () => {
        // onclose 统一处理重连
      }
      socket.onclose = () => {
        ws = null
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

  function subscribe() {
    pendingSubscribe = true
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ type: 'subscribe' }))
    }
  }

  function sendInput(data: string, specialKey?: string) {
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      console.warn('[useTerminalSocket] sendInput: socket not open')
      return
    }
    ws.send(
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

  return { start, subscribe, sendInput, stop, reconnect, isOpen }
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
