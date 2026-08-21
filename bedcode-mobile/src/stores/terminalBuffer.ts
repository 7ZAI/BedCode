/**
 * Terminal Buffer Store
 *
 * 移动端终端输出订阅状态（10 号票重写）— 每会话前端直连桌面端
 * /ws/terminal/session/{id}，输出为 TB v2 二进制帧 + JSON 控制帧。
 *
 * 状态机（spec §6.3）：
 *   IDLE → connecting(auth) → auth_ok → subscribe → subscribe_ok{snapshot_seq}
 *     → HISTORY（帧 lastSeq ≤ snapshot_seq 写 xterm + 入历史缓存；
 *       > snapshot_seq 入实时缓冲）→ history_end → FLUSH → LIVE（直写 + 入缓存）
 *   seq 缺口（> lastRenderedSeq+1）→ 重发 subscribe（快照重播跳过 ≤ lastRenderedSeq）
 *   重连（WS 断开）→ 回到 connecting；快照重播跳过 ≤ lastRenderedSeq；
 *   minSeq > lastRenderedSeq+1（环形淘汰）→ 清屏 + onTruncated + 锚定重播
 *   页面重进（xterm 已销毁）→ 历史缓存立即回放 → 服务端帧按 seq 跳过（不双写）
 */

import { defineStore } from 'pinia'
import { reactive, ref } from 'vue'
import { emit } from '@tauri-apps/api/event'
import {
  createTerminalSocket,
  type TerminalSocket,
  type TerminalSocketFrame,
  type SubscribeOkInfo,
} from '@/composables/useTerminalSocket'

// ==================== Types ====================

export type { TerminalSocketFrame as OutputFrame } from '@/composables/useTerminalSocket'

/** 订阅确认信息（subscribe_ok 帧的元数据，供调用方判断快照边界） */
export interface SubscribeResultInfo {
  /** 订阅时刻队列最新序号（历史边界） */
  snapshotSeq: number
  /** 队列最早存续事件序号（环形淘汰后推进） */
  minSeq: number
  historyCount: number
}

/** 实时输出回调 — TerminalView 注册 */
export interface RealtimeHandler {
  onOutput: (data: Uint8Array, frame: TerminalSocketFrame) => void
  /** 清屏（已渲染区域被环形淘汰需全量重播时） */
  onClear?: () => void
  /** 历史头部被淘汰提示（min_seq > 0 时触发一次） */
  onTruncated?: (minSeq: number) => void
}

/** 单会话订阅状态 */
export interface SessionBuffer {
  /** 连接/订阅阶段：idle / connecting / auth / history / live */
  phase: 'idle' | 'connecting' | 'auth' | 'history' | 'live'
  /** 已订阅后端（phase ∈ history/live；供视图判断） */
  subscribed: boolean
  /** 连接建立中（供视图判断防重） */
  subscribing: boolean
  /** 已渲染到的帧末 seq（快照重播去重基准；null = 未渲染过） */
  lastRenderedSeq: number | null
  /** 本次订阅历史边界（subscribe_ok.snapshot_seq） */
  snapshotSeq: number
  /** 队列最早存续序号（subscribe_ok.min_seq） */
  minSeq: number
  /** 会话是否已停止 */
  sessionStopped: boolean
  /** 历史缓存（页面重进回放源；16MB 上限 LRU 淘汰） */
  historyCache: TerminalSocketFrame[]
  /** 历史缓存总字节数 */
  historyBytes: number
  /** history_end 前到达的实时帧（按 seq 入队，history_end 后按序写入） */
  liveBuffer: TerminalSocketFrame[]
  /** subscribe_ok 前到达的帧（防御缓冲；服务端同 actor 顺序下不应出现） */
  pending: TerminalSocketFrame[]
  /** 缓冲帧总字节数（防御性上限） */
  pendingBytes: number
  /** 截断提示已展示（每会话一次） */
  truncatedNotified: boolean
}

// ==================== Store ====================

export const useTerminalBufferStore = defineStore('terminalBuffer', () => {
  // ==================== State ====================

  /** sessionId → 订阅状态 */
  const buffers = reactive(new Map<string, SessionBuffer>())

  /** sessionId → 实时回调（TerminalView 注册的） */
  const realtimeHandlers = reactive(new Map<string, RealtimeHandler>())

  /** sessionId → 终端 socket（普通 Map：socket 对象无需响应式代理） */
  const sockets = new Map<string, TerminalSocket>()

  /** 历史缓存字节上限（LRU 淘汰） */
  const MAX_HISTORY_CACHE_BYTES = 16 * 1024 * 1024
  /** subscribe_ok 前缓冲帧上限（防御性） */
  const MAX_PENDING_FRAME_BYTES = 8 * 1024 * 1024
  /** 会话不存在（启动中/已停止）时的重试上限，超出后停止等待外部恢复 */
  const MAX_SESSION_MISSING_STRIKES = 3
  /** terminal_output_activity 通知节流（ms）：插件 TerminalOutput 触发频率上限 */
  const ACTIVITY_THROTTLE_MS = 200

  /** sessionId → 会话不存在连续重试计数 */
  const sessionMissingStrikes = new Map<string, number>()
  /** sessionId → 上次输出活动通知时间戳 */
  const lastActivityAt = new Map<string, number>()

  /** 预加载已就绪的会话（会话页 prepareSession 成功后标记，终端页挂载时消费一次） */
  const preparedSessionId = ref<string | null>(null)

  /** 标记预加载就绪：终端页挂载后可跳过 forceReplay，直接渲染已缓冲回放 */
  function markPrepared(sessionId: string) {
    preparedSessionId.value = sessionId
  }

  /** 消费预加载标记（一次性）：返回就绪会话 ID 并复位 */
  function consumePrepared(): string | null {
    const id = preparedSessionId.value
    preparedSessionId.value = null
    return id
  }

  /** 使预加载标记失效：会话状态被重置后，已缓冲回放帧不可信 */
  function invalidatePrepared(sessionId: string) {
    if (preparedSessionId.value === sessionId) preparedSessionId.value = null
  }

  // ==================== Socket Lifecycle ====================

  /** 每会话 socket 的 handlers 工厂（闭包持有 buffer） */
  function createSocketForSession(sessionId: string): TerminalSocket {
    const socket = createTerminalSocket({
      onAuthed: () => {
        // 认证成功：订阅帧由 socket 内部在 auth_ok 后自动发送（pendingSubscribe）
      },
      onSubscribed: (info: SubscribeOkInfo) => {
        const buffer = buffers.get(sessionId)
        if (!buffer) return
        buffer.subscribing = false
        buffer.snapshotSeq = info.snapshotSeq
        buffer.minSeq = info.minSeq

        // 截断检测：已渲染区域被环形淘汰（服务端最早存续 seq 已越过游标）→
        // 清屏 + 提示 + 锚定到服务端可提供的最早 seq（避免重播首帧触发缺口循环）
        if (buffer.lastRenderedSeq !== null && info.minSeq > buffer.lastRenderedSeq + 1) {
          const handler = realtimeHandlers.get(sessionId)
          handler?.onClear?.()
          if (!buffer.truncatedNotified) {
            buffer.truncatedNotified = true
            handler?.onTruncated?.(info.minSeq)
          }
          buffer.lastRenderedSeq = info.minSeq - 1
        }

        buffer.phase = 'history'
        buffer.subscribed = true
        // 排空订阅确认前缓冲的帧（防御路径；正常协议序下为空）
        const pending = buffer.pending
        buffer.pending = []
        buffer.pendingBytes = 0
        for (const frame of pending) {
          deliverFrame(sessionId, frame)
        }
      },
      onHistoryEnd: (_snapshotSeq: number) => {
        const buffer = buffers.get(sessionId)
        if (!buffer) return
        buffer.phase = 'live'
        // FLUSH：实时缓冲按到达顺序写入（均 > snapshot_seq，与历史段无缝衔接）
        const live = buffer.liveBuffer
        buffer.liveBuffer = []
        for (const frame of live) {
          deliverFrame(sessionId, frame)
        }
      },
      onFrame: (frame: TerminalSocketFrame) => {
        deliverFrame(sessionId, frame)
      },
      onError: (code: string, message: string) => {
        const buffer = buffers.get(sessionId)
        console.warn(`[terminalBuffer] session ${sessionId} error: ${code} - ${message}`)
        if (code === 'SESSION_NOT_FOUND') {
          // 会话启动中或已停止：有限重试后停止，等待消费者恢复
          const strikes = (sessionMissingStrikes.get(sessionId) ?? 0) + 1
          sessionMissingStrikes.set(sessionId, strikes)
          if (strikes >= MAX_SESSION_MISSING_STRIKES) {
            console.warn(
              `[terminalBuffer] session ${sessionId} not found after ${MAX_SESSION_MISSING_STRIKES} attempts, stopping`,
            )
            sockets.get(sessionId)?.stop()
            if (buffer) {
              buffer.phase = 'idle'
              buffer.subscribed = false
              buffer.subscribing = false
            }
            return
          }
        }
        // 其他错误：断开重连（退避）
        sockets.get(sessionId)?.reconnect()
      },
      onSessionStopped: (stoppedSessionId: string) => {
        const buffer = buffers.get(sessionId)
        if (!buffer) return
        buffer.sessionStopped = true
        buffer.phase = 'idle'
        buffer.subscribed = false
        buffer.subscribing = false
        // 会话停止：不再自动重连，等外部（会话恢复运行）重新订阅
        sockets.get(sessionId)?.stop()
        void stoppedSessionId
      },
      onClose: () => {
        const buffer = buffers.get(sessionId)
        if (!buffer) return
        // 连接断开：实时缓冲/防御缓冲失效；lastRenderedSeq 保留（重连快照重播跳过）
        buffer.liveBuffer = []
        buffer.pending = []
        buffer.pendingBytes = 0
        buffer.subscribing = false
        // 会话不存在重试计数在每次连接建立时不清零（连续失败才停止）；
        // 这里也不动 phase——重连成功后重新走 auth → subscribe 流程
      },
    })
    return socket
  }

  /**
   * 交付帧（状态机核心）：去重（跳过 ≤ lastRenderedSeq）→ 缺口检测（重发订阅）
   * → 按 phase 分流（历史段写 xterm + 入缓存；实时段缓冲待 history_end 后 FLUSH）
   */
  function deliverFrame(sessionId: string, frame: TerminalSocketFrame) {
    const buffer = buffers.get(sessionId)
    if (!buffer || buffer.sessionStopped) return

    // 快照重播/重连后的去重：已渲染部分整帧跳过（重播帧与已渲染帧字节一致）
    if (buffer.lastRenderedSeq !== null && frame.lastSeq <= buffer.lastRenderedSeq) return

    // 连续性缺口：帧首 seq 越过游标（缺帧）→ 重新订阅拿快照（跳过 ≤ lastRenderedSeq）
    if (buffer.lastRenderedSeq !== null && frame.seq > buffer.lastRenderedSeq + 1) {
      console.error(
        `[terminalBuffer] seq gap: frame.start=${frame.seq}, last_rendered=${buffer.lastRenderedSeq}. Re-subscribing for snapshot`,
      )
      sockets.get(sessionId)?.subscribe()
      return
    }

    // 历史段内实时帧（订阅后新产出，未在历史快照内）：缓冲至 history_end 统一 FLUSH
    if (buffer.phase === 'history' && frame.lastSeq > buffer.snapshotSeq) {
      buffer.liveBuffer.push(frame)
      return
    }

    writeFrame(sessionId, frame)
  }

  /** 写入路径：推进游标 + 入历史缓存（LRU）+ 通知插件 + 回调视图 */
  function writeFrame(sessionId: string, frame: TerminalSocketFrame) {
    const buffer = buffers.get(sessionId)
    if (!buffer) return

    buffer.historyCache.push(frame)
    buffer.historyBytes += frame.data.byteLength
    // LRU 淘汰：超出上限从头丢弃（缓存仅用于页面重进回放，丢最旧不影响实时）
    while (buffer.historyBytes > MAX_HISTORY_CACHE_BYTES && buffer.historyCache.length > 0) {
      const oldest = buffer.historyCache.shift()
      if (oldest) buffer.historyBytes -= oldest.data.byteLength
    }

    buffer.lastRenderedSeq = frame.lastSeq

    // 插件 TerminalOutput 通知（09 迁移后由前端在此触发，仅传 session_id；节流）
    const now = Date.now()
    const last = lastActivityAt.get(sessionId) ?? 0
    if (now - last >= ACTIVITY_THROTTLE_MS) {
      lastActivityAt.set(sessionId, now)
      emit('terminal_output_activity', { session_id: sessionId }).catch(() => {})
    }

    realtimeHandlers.get(sessionId)?.onOutput(frame.data, frame)
  }

  /** 确保会话有订阅状态，不存在则创建 */
  function ensureBuffer(sessionId: string): SessionBuffer {
    let buffer = buffers.get(sessionId)
    if (!buffer) {
      buffer = {
        phase: 'idle',
        subscribed: false,
        subscribing: false,
        lastRenderedSeq: null,
        snapshotSeq: 0,
        minSeq: 0,
        sessionStopped: false,
        historyCache: [],
        historyBytes: 0,
        liveBuffer: [],
        pending: [],
        pendingBytes: 0,
        truncatedNotified: false,
      }
      buffers.set(sessionId, buffer)
    }
    return buffer
  }

  // ==================== Subscription ====================

  /**
   * 订阅会话（统一入口，幂等）— 页面进入 / 重连恢复 / 自愈全部收敛于此。
   * 已连接且已订阅（phase history/live）时直接返回当前快照元数据
   *
   * @returns 订阅确认信息；连接失败时返回 null
   */
  async function subscribeSession(sessionId: string): Promise<SubscribeResultInfo | null> {
    const buffer = ensureBuffer(sessionId)
    if (buffer.sessionStopped) return null

    // 已订阅：直接返回当前快照元数据（页面存活重复调用）
    if (buffer.subscribed) {
      return {
        snapshotSeq: buffer.snapshotSeq,
        minSeq: buffer.minSeq,
        historyCount: buffer.historyCache.length,
      }
    }

    let socket = sockets.get(sessionId)
    if (!socket) {
      socket = createSocketForSession(sessionId)
      sockets.set(sessionId, socket)
    }

    // 连接已建立但未订阅（重连后待恢复）：直接发订阅帧
    if (socket.isOpen()) {
      buffer.subscribing = true
      buffer.phase = 'auth'
      socket.subscribe()
      return null // 订阅确认经 socket 回调异步到达
    }

    buffer.subscribing = true
    buffer.phase = 'connecting'
    socket.start(sessionId)
    socket.subscribe() // 挂起：auth_ok 后自动发送
    return null
  }

  /** 会话不存在重试计数复位（订阅路径每次显式调用时清零） */
  function resetMissingStrikes(sessionId: string) {
    sessionMissingStrikes.delete(sessionId)
  }

  /** 强制全量重播：页面重进时 xterm 为全新实例，历史缓存已由
   *  registerRealtimeHandler 回放并推进 lastRenderedSeq；此处仅重置
   *  实时缓冲并在连接存活时重发订阅（服务端快照重播按 lastRenderedSeq
   *  跳过已渲染部分，不双写） */
  function forceReplay(sessionId: string) {
    const buffer = ensureBuffer(sessionId)
    buffer.liveBuffer = []
    buffer.pending = []
    buffer.pendingBytes = 0
    sessionMissingStrikes.delete(sessionId)
    const socket = sockets.get(sessionId)
    if (socket?.isOpen()) {
      buffer.subscribing = true
      buffer.phase = 'auth'
      socket.subscribe()
    }
  }

  /** 获取会话订阅状态 */
  function getBuffer(sessionId: string): SessionBuffer | undefined {
    return buffers.get(sessionId)
  }

  /** 标记已订阅后端（兼容旧 API；socket 回调已维护，仅防御） */
  function markSubscribed(sessionId: string) {
    const buffer = ensureBuffer(sessionId)
    buffer.subscribed = true
  }

  /** 渲染背压 ack：转发到会话 socket（视图 onWriteParsed 门控后调用） */
  function ackRendered(sessionId: string) {
    sockets.get(sessionId)?.ackRendered()
  }

  /** 标记未订阅（取消订阅时）：关 socket + 清实时/防御缓冲，保留游标与历史缓存 */
  function markUnsubscribed(sessionId: string) {
    invalidatePrepared(sessionId)
    sockets.get(sessionId)?.stop()
    const buffer = buffers.get(sessionId)
    if (buffer) {
      buffer.subscribed = false
      buffer.phase = 'idle'
      buffer.subscribing = false
      buffer.liveBuffer = []
      buffer.pending = []
      buffer.pendingBytes = 0
    }
  }

  /** 标记所有 buffer 未订阅（连接断开时；socket 自动重连恢复） */
  function markAllUnsubscribed() {
    for (const buffer of buffers.values()) {
      buffer.subscribed = false
      buffer.subscribing = false
      buffer.liveBuffer = []
      buffer.pending = []
      buffer.pendingBytes = 0
    }
  }

  /**
   * 标记会话停止：订阅状态失效 + 关闭 socket。
   * 会话重启后 seq 空间从 0 重建（新 SessionOutputManager），
   * lastRenderedSeq 必须重置（旧流序号在新流中无意义）
   */
  function markSessionStopped(sessionId: string) {
    invalidatePrepared(sessionId)
    const buffer = buffers.get(sessionId)
    if (buffer) {
      buffer.sessionStopped = true
      buffer.subscribed = false
      buffer.phase = 'idle'
      buffer.subscribing = false
      buffer.lastRenderedSeq = null
      buffer.liveBuffer = []
      buffer.pending = []
      buffer.pendingBytes = 0
    }
    sockets.get(sessionId)?.stop()
  }

  /** 标记会话恢复运行：复位 sessionStopped，等待订阅路径重建连接 */
  function markSessionRunning(sessionId: string) {
    const buffer = buffers.get(sessionId)
    if (buffer) {
      buffer.sessionStopped = false
      buffer.subscribed = false
      buffer.phase = 'idle'
      buffer.subscribing = false
      buffer.lastRenderedSeq = null
      buffer.liveBuffer = []
      buffer.pending = []
      buffer.pendingBytes = 0
    }
  }

  /** 清理单个会话订阅状态 */
  function clearBuffer(sessionId: string) {
    invalidatePrepared(sessionId)
    sockets.get(sessionId)?.stop()
    sockets.delete(sessionId)
    buffers.delete(sessionId)
    realtimeHandlers.delete(sessionId)
    sessionMissingStrikes.delete(sessionId)
    lastActivityAt.delete(sessionId)
  }

  /** 清理所有订阅状态 */
  function clearAllBuffers() {
    preparedSessionId.value = null
    for (const socket of sockets.values()) socket.stop()
    sockets.clear()
    buffers.clear()
    realtimeHandlers.clear()
    sessionMissingStrikes.clear()
    lastActivityAt.clear()
  }

  // ==================== Realtime Handler ====================

  /** 注册实时输出回调（TerminalView onMounted 时调用） */
  function registerRealtimeHandler(sessionId: string, handler: RealtimeHandler) {
    realtimeHandlers.set(sessionId, handler)
    // 注册时机 = xterm 全新实例（页面挂载）：历史缓存无条件立即回放，
    // 随后服务端帧按 seq 跳过（不双写）。覆盖两个场景：
    // - 页面重进：缓存即全部可见历史，回放后 lastRenderedSeq 推进到缓存末帧
    // - 预加载：会话页订阅期间无 handler，帧已入缓存，挂载时一次回放
    const buffer = buffers.get(sessionId)
    if (buffer && buffer.historyCache.length > 0) {
      for (const frame of buffer.historyCache) {
        handler.onOutput(frame.data, frame)
      }
      const last = buffer.historyCache[buffer.historyCache.length - 1]
      buffer.lastRenderedSeq = last.lastSeq
    }
  }

  /** 注销实时输出回调（TerminalView onUnmounted 时调用） */
  function unregisterRealtimeHandler(sessionId: string) {
    realtimeHandlers.delete(sessionId)
  }

  // ==================== Input ====================

  /** 发送终端输入（经终端 WS input 帧；替代旧 HTTP 输入路径） */
  function sendInput(sessionId: string, data: string, specialKey?: string): boolean {
    const buffer = buffers.get(sessionId)
    if (!buffer || !buffer.subscribed) {
      console.warn(`[terminalBuffer] sendInput: session ${sessionId} not subscribed`)
      return false
    }
    const socket = sockets.get(sessionId)
    if (!socket?.isOpen()) {
      console.warn(`[terminalBuffer] sendInput: session ${sessionId} socket not open`)
      return false
    }
    socket.sendInput(data, specialKey)
    return true
  }

  // ==================== Legacy Compatibility ====================

  /**
   * 旧全局 ws_output 监听启动（兼容 useMobileConnection 调用点）。
   * 10 号票后输出经每会话终端 socket 直连，无全局事件监听——no-op
   */
  function startGlobalListener(): Promise<void> {
    return Promise.resolve()
  }

  return {
    buffers,
    realtimeHandlers,
    ensureBuffer,
    getBuffer,
    markSubscribed,
    markPrepared,
    consumePrepared,
    markUnsubscribed,
    markAllUnsubscribed,
    markSessionStopped,
    markSessionRunning,
    forceReplay,
    ackRendered,
    clearBuffer,
    clearAllBuffers,
    registerRealtimeHandler,
    unregisterRealtimeHandler,
    startGlobalListener,
    subscribeSession,
    resetMissingStrikes,
    sendInput,
  }
})

// 供 composable 复用
export { createTerminalSocket }
