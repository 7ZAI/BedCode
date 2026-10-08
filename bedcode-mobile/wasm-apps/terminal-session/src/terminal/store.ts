/**
 * Terminal Buffer Store（票 05：终端流新协议 + 本地字节计数）
 *
 * 移动端终端输出订阅（对齐桌面插件 `ws_terminal.rs` 新协议，spec §3.3）：
 * - 订阅由终端插件管理、前端触发（票 12）：进入终端页 → `terminalSubscribe`（fresh
 *   subscribe = 插件回放环窗口，历史与实时同一条流）；离开终端页 →
 *   `terminalUnsubscribe`（关闭连接，不得后台常拉）；意外断开 → 宿主自动
 *   退避重连并重新订阅（无续传语义；环淘汰由 `ring_resync` 如实告知）
 * - 输出帧 = **裸字节**（无 TB v3 16B 头、无 per-frame offset）：段2 页面级
 *   Channel 的原始字节消息，按到达序直接渲染；本地接收字节计数（
 *   `lastRenderedOffset`）**仅用于 ack 水位**（`terminalAckRendered` → Rust
 *   节流回发 `{"type":"ack","offset":<本地已渲染字节数>}`）
 * - `ring_resync` 是**唯一**重锚信号（终端插件发 `terminal-resync` 事件，票 12）：
 *   清屏 + 本地计数基准重置 + 一次性提示；重锚后到达的重播帧从新基准续
 * - 无缺口判定 / 无去重 / 无跨帧裁剪：字节连续性由 WS 帧序保证，缺口只经
 *   `ring_resync` 显性表达（缺口号不再误报）
 * - live 门控：收 `subscribed`（`terminal-state` phase=live）即进入 live；
 *   `subscribed` 之前的帧插件侧已丢弃（fresh subscribe 前旧流残留），
 *   前端无需缓冲拼接——「回放（历史）→ 实时」次序由 Channel FIFO 天然保证
 *
 * 状态机（对齐 Rust terminal_state 事件）：
 *   idle → connecting → auth → live
 *   （新协议无独立 history 阶段：收 `subscribed` 即 live）
 */

import { defineStore } from 'pinia'
import { reactive } from 'vue'
import { getMobileApi, type Disposable } from '@binblink/bedcode-plugin-sdk-mobile'
import { logger, events, invokeTerminal } from './host'

// ==================== Types ====================

/** Rust 侧链路状态快照（terminal-session.get-state 响应；与插件 Rust 域同形） */
export interface TerminalLinkState {
  sessionId: string
  phase: string
  /** 本地已收字节（统计；重锚后归零） */
  cursor: number
  /** 前端已渲染字节（本地计数，ack 帧 offset 值） */
  acked: number
  stopped: boolean
  /** 已收 subscribed（门控） */
  subscribed: boolean
}

/** 实时输出回调 — TerminalView 注册 */
export interface RealtimeHandler {
  onOutput: (data: Uint8Array) => void
  /** 历史写入背压信号（兼容保留；新协议无历史拼接，不调用） */
  writeParsed?: (data: Uint8Array) => Promise<void>
  /** 历史拼接完成（兼容保留；新协议回放随流，立即触发） */
  onReplayDone?: () => void
  /** 清屏（ring_resync / 重订阅重锚：在屏内容将被重播取代） */
  onClear?: () => void
  /** 历史头部被淘汰通知（仅后台日志，不提示用户） */
  onTruncated?: (offset: number) => void
}

/** 单会话订阅状态 */
export interface SessionBuffer {
  /** 连接/订阅阶段（terminal-state 事件同步）：idle/connecting/auth/live */
  phase: 'idle' | 'connecting' | 'auth' | 'live'
  /** 已订阅后端（phase=live；供视图判断） */
  subscribed: boolean
  /** 订阅建立中（Rust 连接握手未完成前） */
  subscribing: boolean
  /** 本地已渲染字节数（自上次重锚起；仅用于 ack 水位） */
  lastRenderedOffset: number | null
  /** 会话是否已停止 */
  sessionStopped: boolean
  /**
   * 链路断开且正在退避重试（`terminal-state` detail=reconnecting）
   *
   * **为什么要显式建模**：phase 在退避期间恒为 `connecting`，与「首次订阅中」
   * 无法区分；两者的用户含义完全相反（前者是连接挂了正在自愈，后者是
   * 正常建立中）。不区分的结果就是 ui-ux-pro-max ux 域标记 Severity High
   * 的反模式「No feedback」——终端静默冻结，用户无从判断该不该等。
   */
  reconnecting: boolean
  /**
   * 下次重连的等待毫秒数（detail=reconnect_scheduled 携带；null = 未排期）
   *
   * 退避封顶 30s，无倒计时时用户无法区分「在重连」与「已死」。
   */
  reconnectInMs: number | null
  /**
   * 最近一次 reconnect_scheduled 到达的 wall-clock 时间戳（ms）。
   *
   * **为什么需要它**：reconnectInMs 是一次性的等待时长，不是截止时刻。前端
   * 倒计时 = `reconnectInMs − (now − reconnectReceivedAt)`；只读静态值会导致
   * 显示秒数恒定不递减（2026-10-04 OCR M-02——原实现每秒重读同一毫秒数）。
   */
  reconnectReceivedAt: number | null
  /** 本次页面生命周期内是否渲染过内容（resync 清屏/提示门控） */
  hasRenderedContent: boolean
  /** 截断通知已派发（每会话一次；仅后台日志，防重复刷屏） */
  truncatedNotified: boolean
}

/** Rust 侧 phase 映射 */
const PHASE_MAP: Record<string, SessionBuffer['phase']> = {
  idle: 'idle',
  connecting: 'connecting',
  auth: 'auth',
  live: 'live',
}

/** 订阅阶段推进序（对账时只前进不回退：命令响应可能晚于更新的状态事件到达） */
const PHASE_RANK: Record<SessionBuffer['phase'], number> = {
  idle: 0,
  connecting: 1,
  auth: 2,
  live: 3,
}

// ==================== Store ====================

/** 终端状态事件名（票 12：事件源 = `com.bedcode.terminal-session` 插件 emit） */
const TERMINAL_STATE_EVENT = 'plugin:com.bedcode.terminal-session:terminal-state'
/** 重锚事件名（ring_resync / 重订阅回包） */
const TERMINAL_RESYNC_EVENT = 'plugin:com.bedcode.terminal-session:terminal-resync'

export const useTerminalBufferStore = defineStore('terminalBuffer', () => {
  // ==================== State ====================

  /** sessionId → 订阅状态 */
  const buffers = reactive(new Map<string, SessionBuffer>())

  /** sessionId → 实时回调（TerminalView 注册的） */
  const realtimeHandlers = reactive(new Map<string, RealtimeHandler>())

  let stateUnlisten: Disposable | null = null
  let resyncUnlisten: Disposable | null = null
  /**
   * 段2 推送通道（宿主 mobileApi.openTerminalStream 登记的页面级字节流，
   * 输出帧唯一出口）。
   *
   * 与页面进出严格配对：`markPageEntered` 登记（宿主内部创建 Channel +
   * terminal_page_subscribe），`markPageLeft` dispose（宿主内部作废回调 +
   * terminal_page_unsubscribe）。`pageTokens` 为页面代际号：登记是异步的，
   * 完成时若已换代/离开则立即注销本代通道
   */
  const pageStreams = new Map<string, { dispose(): void }>()
  /** sessionId → 页面代际号（异步登记完成前校验） */
  const pageTokens = new Map<string, number>()

  /** 已知订阅阶段（key = sessionId → phase）：onStateEvent 在 buffer 未创建时
   *  也记录——live 事件早于页面挂载（直接进页面路径）时若被丢弃，buffer 挂载后
   *  phase 停在 idle。ensureBuffer 继承该值 */
  const knownPhases = new Map<string, SessionBuffer['phase']>()

  // ==================== 链路调试统计（终端字节对账，5s 节流） ====================

  /** 会话级帧统计（非响应式，纯日志对账用，不进渲染路径） */
  interface FrameStats {
    /** 收到的 Channel 字节消息数 */
    framesReceived: number
    /** 收流字节（Channel 走 Raw 字节，与 bytesRendered 同口径） */
    bytesReceived: number
    /** 页面未挂载被丢弃的帧数（设计行为：Rust 缓存兜底，重进回补） */
    framesNoHandler: number
    /** 实际交给渲染管线的字节（精确值） */
    bytesRendered: number
    lastLogAt: number
    lastLogCursor: number
  }

  /** sessionId → 帧统计 */
  const frameStats = new Map<string, FrameStats>()

  /** 统计打点间隔（ms）：输出风暴期不逐帧刷日志 */
  const FRAME_STATS_INTERVAL_MS = 5000

  function statsFor(sessionId: string): FrameStats {
    let s = frameStats.get(sessionId)
    if (!s) {
      s = {
        framesReceived: 0,
        bytesReceived: 0,
        framesNoHandler: 0,
        bytesRendered: 0,
        lastLogAt: 0,
        lastLogCursor: -1,
      }
      frameStats.set(sessionId, s)
    }
    return s
  }

  /** 周期打点（5s 且游标有推进才打）：与终端插件收帧统计对账（票 12），
   * bytesReceived ≈ bytesRendered + noHandler 差值即无丢帧 */
  function maybeLogFrameStats(sessionId: string) {
    const s = frameStats.get(sessionId)
    if (!s) return
    const now = Date.now()
    if (now - s.lastLogAt < FRAME_STATS_INTERVAL_MS) return
    const cursor = buffers.get(sessionId)?.lastRenderedOffset ?? -1
    if (cursor === s.lastLogCursor && s.lastLogAt !== 0) return
    s.lastLogAt = now
    s.lastLogCursor = cursor
    logger.debug(
      `[terminalBuffer] frame stats (${sessionId}): frames=${s.framesReceived} ` +
        `bytesReceived=${s.bytesReceived} rendered=${s.bytesRendered} ` +
        `noHandler=${s.framesNoHandler} cursor=${cursor}`,
    )
  }

  // ==================== 事件监听（Rust → 前端） ====================

  /**
   * 惰性注册全局事件监听（终端状态 / 重锚，票 12 起事件源 = 插件 emit，
   * 事件名带插件命名空间前缀；payload 形状与退役前宿主事件逐字段一致）。
   *
   * 输出帧不在此列：段2 帧经页面级 Tauri Channel 投递（见 `pageChannels`），
   * 全局事件只承载低频的状态机迁移与重锚信号——两者解耦后互不影响
   */
  function ensureEventListeners() {
    if (stateUnlisten && resyncUnlisten) return
    if (!stateUnlisten) {
      stateUnlisten = events.on(TERMINAL_STATE_EVENT, (payload: any) => {
        if (!payload?.session_id) return
        onStateEvent(payload as { session_id: string; phase?: string; detail?: string })
      })
    }
    if (!resyncUnlisten) {
      resyncUnlisten = events.on(TERMINAL_RESYNC_EVENT, (payload: any) => {
        if (!payload?.session_id) return
        onResyncEvent(payload as { session_id: string; offset: number })
      })
    }
  }

  /**
   * 重锚事件（`ring_resync` / 重订阅回包，Rust 侧已重置计数与门控）：
   * 清屏 + 本地计数基准重置 + 一次性提示。重锚后到达的重播帧从新基准续。
   *
   * 无在屏内容（首次挂载空屏 / 空终端）时跳过清屏与提示（清屏无意义；
   * 有内容时**必须**清屏——重播与已在屏内容重叠即画面错乱）
   */
  function onResyncEvent(payload: { session_id: string; offset: number }) {
    const buffer = buffers.get(payload.session_id)
    if (!buffer || buffer.sessionStopped) return
    const handler = realtimeHandlers.get(payload.session_id)
    const hadContent = buffer.hasRenderedContent
    buffer.hasRenderedContent = false
    buffer.lastRenderedOffset = null
    knownPhases.set(payload.session_id, 'live')
    if (PHASE_RANK[buffer.phase] < PHASE_RANK.live) buffer.phase = 'live'
    buffer.subscribed = true
    if (!hadContent) return
    logger.warn(
      `[terminalBuffer] resync (${payload.session_id}): re-anchor at offset=${payload.offset}, clear + anchored replay`,
    )
    handler?.onClear?.()
    if (!buffer.truncatedNotified) {
      buffer.truncatedNotified = true
      handler?.onTruncated?.(payload.offset)
    }
  }

  // ==================== 段2 Channel 帧入口（裸字节） ====================

  /**
   * 段2 Channel 消息入口：**裸字节**（无帧头、无 per-frame offset）。
   * Rust 侧已按序转发（subscribed 之后的回放 + 实时同一条流），此处直接渲染，
   * 本地计数仅作 ack 水位推进
   */
  function onChannelMessage(sessionId: string, payload: ArrayBuffer | Uint8Array) {
    const bytes = payload instanceof Uint8Array ? payload : new Uint8Array(payload)
    deliverRawBytes(sessionId, bytes)
  }

  /**
   * 交付一段输出字节：本地计数推进 + 渲染。
   *
   * 无去重 / 无缺口判定 / 无跨帧裁剪（旧 TB v3 语义退役）：字节连续性由 WS
   * 帧序保证，缺口只经 `ring_resync` 显性表达——缺口号不再误报
   */
  function deliverRawBytes(sessionId: string, data: Uint8Array) {
    const buffer = buffers.get(sessionId)
    if (!buffer || buffer.sessionStopped) return
    const handler = realtimeHandlers.get(sessionId)
    const stats = statsFor(sessionId)
    stats.framesReceived++
    stats.bytesReceived += data.byteLength
    if (!handler) {
      // 页面未开：帧只进 Rust 侧门控（无通道不转发），此处兜底丢弃
      stats.framesNoHandler++
      maybeLogFrameStats(sessionId)
      return
    }
    // 本地渲染字节计数（仅用于 ack 水位；重锚后由 onResyncEvent 归零）
    buffer.lastRenderedOffset = (buffer.lastRenderedOffset ?? 0) + data.byteLength
    buffer.hasRenderedContent = true
    stats.bytesRendered += data.byteLength
    maybeLogFrameStats(sessionId)

    handler.onOutput(data)
  }

/** Rust 链路状态事件：同步 phase/subscribed/停止等 */
  function onStateEvent(payload: { session_id: string; phase?: string; detail?: string; retry_in_ms?: number }) {
    const statePhase = PHASE_MAP[payload.phase ?? ''] ?? null
    if (payload.detail === 'stopped' || payload.detail === 'session_missing' || payload.detail === 'unsubscribed') {
      knownPhases.delete(payload.session_id)
    }
    if (statePhase) knownPhases.set(payload.session_id, statePhase)
    const buffer = buffers.get(payload.session_id)
    if (!buffer) return
    const phase = PHASE_MAP[payload.phase ?? ''] ?? buffer.phase
    if (phase !== buffer.phase) {
      logger.debug(
        `[terminalBuffer] state (${payload.session_id}): phase ${buffer.phase} -> ${phase}` +
        `${payload.detail ? ` (${payload.detail})` : ''}`,
      )
    }
    buffer.phase = phase
    buffer.subscribed = phase === 'live'
    if (phase === 'live') {
      buffer.subscribing = false
      buffer.sessionStopped = false
      // 恢复 live → 断开提示与倒计时一并撤除（否则残留的倒计时会继续跑）
      buffer.reconnecting = false
      buffer.reconnectInMs = null
      buffer.reconnectReceivedAt = null
    }
    // 退避重试：Rust 在每次断线后发 reconnecting（进入退避）+ reconnect_scheduled
    // （本次等多久）。两者都置 reconnecting=true，只有后者带倒计时。
    if (payload.detail === 'reconnecting' || payload.detail === 'reconnect_scheduled') {
      buffer.reconnecting = true
      if (payload.detail === 'reconnect_scheduled' && typeof payload.retry_in_ms === 'number') {
        buffer.reconnectInMs = payload.retry_in_ms
        // 排期到达时刻 = 倒计时基准（只在此处重置：同一轮退避内的 reconnecting
        // 事件不得把已开始的倒计时拨回满值）
        buffer.reconnectReceivedAt = Date.now()
      } else {
        buffer.reconnectInMs = null
        buffer.reconnectReceivedAt = null
      }
    }
    if (payload.detail === 'stopped' || payload.detail === 'session_missing') {
      buffer.phase = 'idle'
      buffer.subscribed = false
      buffer.subscribing = false
      buffer.sessionStopped = true
      buffer.lastRenderedOffset = null
      // 链路已放弃（strikes 耗尽 / 会话停止）：Rust 不再发事件，不清除会让
      // 横幅永久卡「正在重连/30s 后重连」（2026-10-04 OCR M-03）
      buffer.reconnecting = false
      buffer.reconnectInMs = null
      buffer.reconnectReceivedAt = null
    }
    if (payload.detail === 'unsubscribed') {
      buffer.subscribed = false
      if (buffer.phase !== 'idle') buffer.phase = 'idle'
      buffer.subscribing = false
      // 主动取消订阅同样终止重连：横幅一并撤除（M-03 同款）
      buffer.reconnecting = false
      buffer.reconnectInMs = null
      buffer.reconnectReceivedAt = null
    }
  }

  // ==================== 辅助 ====================

  function ensureBuffer(sessionId: string): SessionBuffer {
    let buffer = buffers.get(sessionId)
    if (!buffer) {
      // 继承最近一次订阅阶段（页面挂载晚于订阅完成时，live 事件已由
      // onStateEvent 记录到此 map）
      const known = knownPhases.get(sessionId) ?? 'idle'
      buffer = {
        phase: known,
        subscribed: known === 'live',
        subscribing: false,
        lastRenderedOffset: null,
        sessionStopped: false,
        // 重连态**不**从 knownPhases 继承：继承的是 phase，而 phase=connecting
        // 分不清「首次订阅中」与「断线退避中」，必须由 reconnecting 事件单独置位
        reconnecting: false,
        reconnectInMs: null,
        reconnectReceivedAt: null,
        hasRenderedContent: false,
        truncatedNotified: false,
      }
      buffers.set(sessionId, buffer)
    }
    return buffer
  }

  // ==================== 渲染 ====================

  // ==================== Socket/订阅生命周期（Rust 驱动） ====================

  /**
   * 应用 Rust 链路状态快照（`terminal_get_state`）：收敛前端订阅信念。
   *
   * 为什么需要：Rust `terminal_subscribe` 是幂等的——链路已在运行时**不发任何
   * 状态事件**，前端仅靠事件会永久停在「未订阅」：输入被 `sendInput` 的
   * subscribed 门控拒绝。对账是唯一兜底。
   *
   * 只前进不回退：命令响应与状态事件分属两条通道，响应可能晚于更新的
   * 事件到达，回退会把已推进的 phase 冲掉。
   */
  function applyLinkState(sessionId: string, state: TerminalLinkState | null | undefined) {
    if (!state) return
    const incoming = PHASE_MAP[state.phase ?? ''] ?? null
    const buffer = buffers.get(sessionId)
    if (state.stopped) {
      knownPhases.delete(sessionId)
      if (buffer) {
        buffer.sessionStopped = true
        buffer.subscribed = false
        buffer.subscribing = false
        buffer.phase = 'idle'
      }
      return
    }
    if (!incoming) return
    if (buffer && PHASE_RANK[incoming] < PHASE_RANK[buffer.phase]) return
    knownPhases.set(sessionId, incoming)
    if (!buffer) return
    buffer.phase = incoming
    buffer.subscribed = incoming === 'live'
    if (buffer.subscribed) {
      buffer.subscribing = false
      buffer.sessionStopped = false
    }
  }

  /** 主动拉取 Rust 链路状态并对账（订阅/刷新路径的收敛兜底） */
  async function reconcileState(sessionId: string): Promise<void> {
    try {
      const state = (await invokeTerminal('terminal-session.get-state', { sessionId })) as TerminalLinkState
      if (!buffers.has(sessionId)) return
      applyLinkState(sessionId, state)
    } catch (e: any) {
      logger.warn(`[terminalBuffer] state reconcile failed for ${sessionId}:`, e?.message || e)
    }
  }

  /**
   * 订阅会话（统一入口；**fresh subscribe 语义**）：确保 Rust 链路订阅并触发
   * 环窗口回放。无论前端信念如何都 invoke（链路已在运行 → Rust 发 fresh
   * subscribe 帧 → 插件重播）；订阅确认经 terminal-state 事件/状态对账异步到达。
   *
   * 幂等性由调用方守卫：页面挂载 / 会话恢复运行触发；重试路径见
   * useTerminalSubscription（订阅成功即停）
   */
  async function subscribeSession(sessionId: string): Promise<null> {
    const buffer = ensureBuffer(sessionId)
    if (buffer.sessionStopped) return null
    try {
      ensureEventListeners()
    } catch (e) {
      logger.warn('[terminalBuffer] event listener init failed:', e)
    }

    buffer.subscribing = true
    buffer.phase = 'connecting'
    let invoked = true
    await invokeTerminal('terminal-session.subscribe', { sessionId }).catch((e) => {
      logger.warn(`[terminalBuffer] subscribe ${sessionId} failed:`, e)
      invoked = false
    })
    // 幂等订阅不发事件：调用成功后主动对账一次，保证信念必然收敛
    if (invoked) await reconcileState(sessionId)
    // 调用失败且未订阅：复位订阅中标志，交由调用方重试（成功路径保持
    // subscribing=true 直到状态事件/对账落定，避免重试路径误判失败）
    if (!invoked && !buffer.subscribed) buffer.subscribing = false
    return null
  }

  function resetMissingStrikes(sessionId: string) {
    // Rust 侧管理会话缺失重试；前端无需计数。保留签名兼容
    void sessionId
  }

  /** 进入终端页 = 全量重播：xterm 每次进入都是全新实例；本地计数基准归零
   *  （重播帧从新基准续，无需旧游标续传） */
  function resetCursor(sessionId: string) {
    const buffer = buffers.get(sessionId)
    if (buffer) buffer.lastRenderedOffset = null
  }

  function getBuffer(sessionId: string): SessionBuffer | undefined {
    return buffers.get(sessionId)
  }

  function markSubscribed(sessionId: string) {
    const buffer = ensureBuffer(sessionId)
    buffer.subscribed = true
  }

  /** 渲染背压 ack：本地已渲染字节数推进 Rust 侧 ack 水位（Rust 节流回发桌面端） */
  function ackRendered(sessionId: string) {
    const buffer = buffers.get(sessionId)
    if (!buffer || buffer.lastRenderedOffset === null) return
    invokeTerminal('terminal-session.ack-rendered', {
      sessionId,
      offset: buffer.lastRenderedOffset,
    }).catch((e) => {
      logger.warn(`[terminalBuffer] ack failed for ${sessionId}:`, e)
    })
  }

  /** 退出终端页：清理前端消费态 + 段2 注销 + **关闭链路**（离开页面即
   *  退订/关闭，不得后台常拉；重进时重订阅回放补齐） */
  function markPageLeft(sessionId: string) {
    // 先作废代际（在途帧不得写进已卸载的 xterm），再注销通道与链路。
    // 代际先删：异步登记完成的 openTerminalStream 见 token 不匹配即自行注销
    pageTokens.delete(sessionId)
    const stream = pageStreams.get(sessionId)
    if (stream) {
      pageStreams.delete(sessionId)
      stream.dispose()
    }
    // 段1 关闭连接：桌面环窗口保留输出，重进时重订阅回放补齐
    invokeTerminal('terminal-session.unsubscribe', { sessionId }).catch((e) => {
      logger.warn(`[terminalBuffer] unsubscribe ${sessionId} failed:`, e)
    })
  }

  /** 进入终端页：段2 登记（开启输出推送）。订阅本身由 subscribeSession 触发
   *  （fresh subscribe = 回放）；先登记通道再 invoke：invoke 在途时若有帧到达，
   *  也不会因缺回调而丢。重复进入（未配对 markPageLeft）时先作废上一代：
   *  其回调仍指向本会话，在途帧会写进已被丢弃的 xterm 实例 */
  function markPageEntered(sessionId: string) {
    const previous = pageStreams.get(sessionId)
    if (previous) {
      pageStreams.delete(sessionId)
      previous.dispose()
    }
    const token = (pageTokens.get(sessionId) ?? 0) + 1
    pageTokens.set(sessionId, token)
    void getMobileApi()
      .openTerminalStream(sessionId, (bytes) => {
        if (pageTokens.get(sessionId) !== token) return
        onChannelMessage(sessionId, bytes)
      })
      .then((stream) => {
        if (pageTokens.get(sessionId) === token) {
          pageStreams.set(sessionId, stream)
        } else {
          // 登记完成前页面已离开 / 已换代：立即注销本代通道（不留悬挂推送）
          stream.dispose()
        }
      })
      .catch((e) => {
        logger.warn(`[terminalBuffer] page subscribe ${sessionId} failed:`, e)
      })
  }

  /** 标记未订阅（手动取消）：关闭链路 + 清订阅信念，保留游标 */
  function markUnsubscribed(sessionId: string) {
    knownPhases.delete(sessionId)
    invokeTerminal('terminal-session.unsubscribe', { sessionId }).catch(() => {})
    const buffer = buffers.get(sessionId)
    if (buffer) {
      buffer.subscribed = false
      buffer.phase = 'idle'
      buffer.subscribing = false
    }
  }

  /** 全部未订阅（设备断开时；Rust 链路全部关闭，重连后由页面重新订阅） */
  function markAllUnsubscribed() {
    invokeTerminal('terminal-session.unsubscribe-all', {}).catch(() => {})
    knownPhases.clear()
    for (const buffer of buffers.values()) {
      buffer.subscribed = false
      buffer.subscribing = false
    }
  }

  /** 标记会话停止：关闭链路 + 游标重置（同 id 重启新流坐标空间）。
   *  hasRenderedContent 保留——xterm 仍显示最终输出，重启后重播前需清屏提示 */
  function markSessionStopped(sessionId: string) {
    knownPhases.delete(sessionId)
    const buffer = buffers.get(sessionId)
    if (buffer) {
      buffer.sessionStopped = true
      buffer.subscribed = false
      buffer.phase = 'idle'
      buffer.subscribing = false
      buffer.lastRenderedOffset = null
    }
    invokeTerminal('terminal-session.unsubscribe', { sessionId }).catch(() => {})
  }

  /** 标记会话恢复运行：复位停止信念。**不在此订阅**——后台会话不得常拉，
   *  订阅由页面驱动（TerminalView watch(isSessionActive) → subscribeWithRetry） */
  function markSessionRunning(sessionId: string) {
    const buffer = buffers.get(sessionId)
    if (buffer) {
      buffer.sessionStopped = false
    }
  }

  /** 清理单个会话（会话删除时）：Rust 链路一并清 */
  function clearBuffer(sessionId: string) {
    knownPhases.delete(sessionId)
    invokeTerminal('terminal-session.remove', { sessionId }).catch(() => {})
    buffers.delete(sessionId)
    realtimeHandlers.delete(sessionId)
    frameStats.delete(sessionId)
  }

  /** 清理所有订阅状态（连接断开/退出） */
  function clearAllBuffers() {
    knownPhases.clear()
    invokeTerminal('terminal-session.unsubscribe-all', {}).catch(() => {})
    buffers.clear()
    realtimeHandlers.clear()
    frameStats.clear()
    // 段2 推送通道随全部缓冲区一并作废（页面已不存在，宿主窄转发层的槽位由
    // terminal_unsubscribe_all / 会话删除清理）
    for (const stream of pageStreams.values()) stream.dispose()
    pageStreams.clear()
    pageTokens.clear()
    if (stateUnlisten) {
      stateUnlisten.dispose()
      stateUnlisten = null
    }
    if (resyncUnlisten) {
      resyncUnlisten.dispose()
      resyncUnlisten = null
    }
  }

  // ==================== Realtime Handler（TerminalView 挂载/卸载） ====================

  /** 注册实时输出回调（TerminalView onMounted）：登记页面通道与渲染入口。
   *  历史由 fresh subscribe 回放随流到达（无独立拼接）；onReplayDone 立即触发
   *  （兼容门控信号：live 门控以 `subscribed` 为准） */
  function registerRealtimeHandler(sessionId: string, handler: RealtimeHandler) {
    const buffer = ensureBuffer(sessionId)
    realtimeHandlers.set(sessionId, handler)
    // 新页面生命周期：无在屏内容（重锚提示/清屏门控复位）
    buffer.hasRenderedContent = false
    markPageEntered(sessionId)
    try {
      ensureEventListeners()
    } catch (e) {
      logger.warn('[terminalBuffer] event listener init failed:', e)
    }
    // 新协议无历史拼接：回放随订阅流直达，门控以 subscribed 为准；
    // 兼容信号立即放行（遮罩仍等 phase=live + fit）
    handler.onReplayDone?.()
  }

  /** 注销实时输出回调（TerminalView onUnmounted）：停止消费 + 关闭链路 */
  function unregisterRealtimeHandler(sessionId: string) {
    realtimeHandlers.delete(sessionId)
    markPageLeft(sessionId)
  }

  // ==================== Input ====================

  /** 发送终端输入（经 Rust 终端链路 → WS 帧 → 桌面端 PTY） */
  function sendInput(sessionId: string, data: string, specialKey?: string): boolean {
    const buffer = buffers.get(sessionId)
    if (!buffer || !buffer.subscribed) {
      logger.warn(`[terminalBuffer] sendInput: session ${sessionId} not subscribed`)
      return false
    }
    invokeTerminal('terminal-session.send-input', {
      sessionId,
      data,
      specialKey: specialKey ?? null,
    }).catch((e) => {
      logger.warn(`[terminalBuffer] sendInput failed for ${sessionId}:`, e)
    })
    return true
  }

  return {
    buffers,
    realtimeHandlers,
    ensureBuffer,
    getBuffer,
    markSubscribed,
    markUnsubscribed,
    markAllUnsubscribed,
    markPageEntered,
    markPageLeft,
    markSessionStopped,
    markSessionRunning,
    resetCursor,
    ackRendered,
    clearBuffer,
    clearAllBuffers,
    registerRealtimeHandler,
    unregisterRealtimeHandler,
    subscribeSession,
    reconcileState,
    resetMissingStrikes,
    sendInput,
  }
})
