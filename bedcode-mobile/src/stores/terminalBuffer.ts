/**
 * Terminal Buffer Store（Rust 后端持有终端 WS 后的重写）
 *
 * 移动端终端输出订阅（用户需求 1/2/3 落地）：
 * - 订阅由 Rust 后端管理、前端触发：会话启动 → `terminalSubscribe`；停止 /
 *   手动断开 → `terminalUnsubscribe`；意外断开 → Rust 自动退避重连重订阅
 *   （保留字节游标），前端无需干预
 * - 数据真源 = Rust 侧会话级字节缓存；前端只在进入终端页时获取「一次性历史」
 *   （`terminalGetHistory`，缓存优先、淘汰时回退桌面 HTTP），拼接完历史后才
 *   开始消费实时帧（`terminal-frame` 事件）——「拼完历史才通知前端消费」
 * - 字节连续（TB v3）：游标 = lastRenderedOffset（已渲染区间末端）；去重 /
 *   缺口（重拼接）/ 截断（minOffset 越过游标）/ 跨帧裁剪（overlap = 游标 -
 *   startOffset）全部按字节区间运算
 * - 渲染背压 ack：视图 onWriteParsed → `terminalAckRendered`（Rust 节流回发）
 *
 * 状态机（对齐 Rust terminal_state 事件）：
 *   idle → connecting → auth → history → live
 *   history/live = 已订阅（subscribed）；页面只影响「是否消费事件」，不影响订阅
 */

import { defineStore } from 'pinia'
import { logger } from '@/utils/frontendLogger'
import { reactive, ref } from 'vue'
import { emit } from '@tauri-apps/api/event'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import {
  terminalGetHistory,
  terminalSubscribe,
  terminalUnsubscribe,
  terminalUnsubscribeAll,
  terminalRemove,
  terminalSendInput,
  terminalAckRendered,
  terminalSetMode,
} from '@/composables/useMobileCommands'

// ==================== Types ====================

/** 实时帧（Rust terminal-frame 事件载荷） */
export interface OutputFrame {
  data: Uint8Array
  /** 帧内首字节的会话内累计偏移 */
  startOffset: number
  /** 帧内末字节偏移 = startOffset + len（游标推进基准） */
  endOffset: number
  isWaiting: boolean
}

export type { TerminalHistoryResult as TerminalHistoryInfo } from '@/composables/useMobileCommands'

/** 订阅确认信息（subscribe 时的快照元数据；兼容旧 API 语义） */
export interface SubscribeSnapshot {
  snapshotOffset: number
  minOffset: number
  historyBytes: number
}

export type SubscribeResultInfo = SubscribeSnapshot

/** 实时输出回调 — TerminalView 注册 */
export interface RealtimeHandler {
  onOutput: (data: Uint8Array, frame: OutputFrame) => void
  /** 分片写入：resolve 于 xterm 解析完成后（历史拼接背压信号） */
  writeParsed?: (data: Uint8Array) => Promise<void>
  /** 历史拼接完成（末批已解析） */
  onReplayDone?: () => void
  /** 清屏（驻留历史头部被淘汰，需全量重播时） */
  onClear?: () => void
  /** 历史头部被淘汰提示 */
  onTruncated?: (minOffset: number) => void
}

/** 单会话订阅状态 */
export interface SessionBuffer {
  /** 连接/订阅阶段（Rust terminal-state 事件同步）：idle/connecting/auth/history/live */
  phase: 'idle' | 'connecting' | 'auth' | 'history' | 'live'
  /** 已订阅后端（phase ∈ history/live；供视图判断） */
  subscribed: boolean
  /** 订阅建立中（Rust 连接握手未完成前） */
  subscribing: boolean
  /** 已渲染到的帧末字节偏移（快照/历史/实时共用游标） */
  lastRenderedOffset: number | null
  /** 本次订阅历史边界（Rust snapshot_offset） */
  snapshotOffset: number
  /** Rust 驻留最旧字节位置（min_offset） */
  minOffset: number
  /** 会话是否已停止 */
  sessionStopped: boolean
  /** 历史缓存头部曾被淘汰（回放起点非流首，消费方需提示） */
  headTrimmed: boolean
  /** 历史拼接中：实时帧缓冲待 FLUSH（拼完历史才消费） */
  historyPreparing: boolean
  /** 历史拼接完成前到达的实时帧（按序缓冲） */
  bufferedLive: OutputFrame[]
  /** 缓冲帧总字节数（防御性上限） */
  bufferedBytes: number
  /** 截断提示已展示（每会话一次） */
  truncatedNotified: boolean
}

/** Rust 侧 phase 映射 */
const PHASE_MAP: Record<string, SessionBuffer['phase']> = {
  idle: 'idle',
  connecting: 'connecting',
  auth: 'auth',
  history: 'history',
  live: 'live',
}

// ==================== Store ====================

export const useTerminalBufferStore = defineStore('terminalBuffer', () => {
  // ==================== State ====================

  /** sessionId → 订阅状态 */
  const buffers = reactive(new Map<string, SessionBuffer>())

  /** sessionId → 实时回调（TerminalView 注册的） */
  const realtimeHandlers = reactive(new Map<string, RealtimeHandler>())

  /** 历史拼接代数（注册/注销/清理时递增；在途拼接循环每批检查，失效即放弃） */
  const replayGenerations = new Map<string, number>()

  /** 预加载已就绪的会话（会话页 prepareSession 成功后标记，终端页挂载时消费一次） */
  const preparedSessionId = ref<string | null>(null)

  let frameUnlisten: UnlistenFn | null = null
  let stateUnlisten: UnlistenFn | null = null

  /** 拼接缺口重试冷却（ms） */
  const GAP_RESPLICE_COOLDOWN_MS = 3000
  /** 拼接完成前实时帧缓冲上限（字节，防御性） */
  const MAX_BUFFERED_LIVE_BYTES = 8 * 1024 * 1024
  /** 拼接缺口阶段最近一次重拼接时间 */
  const lastGapRespliceAt = new Map<string, number>()
  /** 缺口冷却期丢帧兜底定时器：冷却到期且游标仍落后时主动重拼接（防流尾缺口无触发） */
  const gapRetryTimers = new Map<string, ReturnType<typeof setTimeout>>()
  /** spliceHistory 等待 phase=live 的超时（ms，对齐视图 HISTORY_SETTLE_TIMEOUT_MS） */
  const SPLICE_WAIT_LIVE_TIMEOUT_MS = 8000
  /** phase → live 等待者（key = sessionId → finish 回调列表；onStateEvent 唤醒/超时兜底） */
  const liveWaiters = new Map<string, Array<(ok: boolean) => void>>()
  /**
   * 已知订阅阶段（key = sessionId → phase）：onStateEvent 在 buffer 未创建时
   * 也记录——live 事件早于页面挂载（直接进页面路径）时若被丢弃，buffer 挂载后
   * phase 停在 idle，spliceHistory 误等 live 8s。ensureBuffer 继承该值
   */
  const knownPhases = new Map<string, SessionBuffer['phase']>()
  /** terminal_output_activity 通知节流（ms） */
  const ACTIVITY_THROTTLE_MS = 200
  /** 输出活动通知时间戳 */
  const lastActivityAt = new Map<string, number>()

  // ==================== 事件监听（Rust → 前端） ====================

  /** 惰性注册全局事件监听（terminal-frame / terminal-state） */
  async function ensureEventListeners() {
    if (frameUnlisten && stateUnlisten) return
    if (!frameUnlisten) {
      frameUnlisten = await listen('terminal-frame', (event) => {
        const payload = event.payload as {
          session_id?: string
          start_offset?: number
          end_offset?: number
          data_base64?: string
        }
        if (!payload.session_id) return
        onFrameEvent(
          payload as { session_id: string; start_offset: number; end_offset: number; data_base64: string },
        )
      })
    }
    if (!stateUnlisten) {
      stateUnlisten = await listen('terminal-state', (event) => {
        const payload = event.payload as {
          session_id?: string
          phase?: string
          detail?: string
          snapshot_offset?: number
          min_offset?: number
          cursor?: number
        }
        if (!payload.session_id) return
        onStateEvent(payload as { session_id: string; phase?: string; detail?: string })
      })
    }
  }

  /** 实时帧事件：历史拼接中缓冲，拼接完成按序 FLUSH；未拼接的分发渲染 */
  function onFrameEvent(payload: { session_id: string; start_offset: number; end_offset: number; data_base64: string }) {
    const buffer = buffers.get(payload.session_id)
    if (!buffer || buffer.sessionStopped) return
    const handler = realtimeHandlers.get(payload.session_id)
    if (!handler) return // 页面未开：帧只进 Rust 缓存，前端丢弃（重进时经 getHistory 回补）
    const frame: OutputFrame = {
      data: base64ToBytes(payload.data_base64),
      startOffset: payload.start_offset,
      endOffset: payload.end_offset,
      isWaiting: false,
    }
    if (buffer.historyPreparing) {
      buffer.bufferedLive.push(frame)
      buffer.bufferedBytes += frame.data.byteLength
      // 防御性上限：拼接期极端滞留（超大历史 + 慢消费）时丢弃最旧，防止内存失控
      while (buffer.bufferedBytes > MAX_BUFFERED_LIVE_BYTES && buffer.bufferedLive.length > 0) {
        const oldest = buffer.bufferedLive.shift()
        if (oldest) buffer.bufferedBytes -= oldest.data.byteLength
      }
      return
    }
    deliverFrame(payload.session_id, frame)
  }

  /**
   * 等待 phase=live（WS 重播段全部入缓存）：history_end 帧落地前，TCP 有序保证
   * 重播帧必已先到——这是「getHistory 缓存优先返回完整历史」的可靠边界。
   * 初始即为 live / 会话已停止时立即返回；phase 唤醒 / 超时兜底。
   */
  function waitForLive(sessionId: string, timeoutMs: number): Promise<boolean> {
    const buffer = buffers.get(sessionId)
    if (buffer && (buffer.phase === 'live' || buffer.sessionStopped)) {
      return Promise.resolve(buffer.phase === 'live')
    }
    return new Promise((resolve) => {
      let settled = false
      function finish(ok: boolean) {
        if (settled) return
        settled = true
        const list = liveWaiters.get(sessionId)
        if (list) {
          const i = list.indexOf(finish)
          if (i >= 0) list.splice(i, 1)
          if (list.length === 0) liveWaiters.delete(sessionId)
        }
        resolve(ok)
      }
      liveWaiters.set(sessionId, [...(liveWaiters.get(sessionId) ?? []), finish])
      setTimeout(() => finish(false), timeoutMs)
    })
  }

  /** 唤醒全部等待者（phase=live / 停止短路）；等待者自行判定结果 */
  function resolveLiveWaiters(sessionId: string) {
    const list = liveWaiters.get(sessionId)
    if (list) {
      liveWaiters.delete(sessionId)
      for (const f of list) f(true)
    }
  }

  /** Rust 链路状态事件：同步 phase/subscribed/停止等 */
  function onStateEvent(payload: { session_id: string; phase?: string; detail?: string }) {
    // 唤醒 live 等待者须先于 buffer 守卫：清 buffer 后在途 spliceHistory 的等待
    // 也需被唤醒短路（否则 await 悬挂泄漏）；判定在 spliceHistory 侧进行
    const statePhase = PHASE_MAP[payload.phase ?? ''] ?? null
    if (statePhase === 'live' || payload.detail === 'stopped' || payload.detail === 'session_missing') {
      resolveLiveWaiters(payload.session_id)
    }
    // buffer 未创建（页面未挂载）时也记录最新订阅阶段：挂载时 ensureBuffer
    // 继承，避免 live 事件早于页面被丢弃 → phase 停在 idle → spliceHistory 空等
    if (statePhase) knownPhases.set(payload.session_id, statePhase)
    if (payload.detail === 'stopped' || payload.detail === 'session_missing' || payload.detail === 'unsubscribed') {
      knownPhases.delete(payload.session_id)
    }
    const buffer = buffers.get(payload.session_id)
    if (!buffer) return
    const phase = PHASE_MAP[payload.phase ?? ''] ?? buffer.phase
    buffer.phase = phase
    buffer.subscribed = phase === 'history' || phase === 'live'
    if (phase === 'history' || phase === 'live') {
      buffer.subscribing = false
      buffer.sessionStopped = false
    }
    if (payload.detail === 'stopped' || payload.detail === 'session_missing') {
      buffer.phase = 'idle'
      buffer.subscribed = false
      buffer.subscribing = false
      buffer.sessionStopped = true
      buffer.lastRenderedOffset = null
      buffer.bufferedLive = []
      buffer.bufferedBytes = 0
      buffer.historyPreparing = false
    }
    if (payload.detail === 'unsubscribed') {
      buffer.subscribed = false
      if (buffer.phase !== 'idle') buffer.phase = 'idle'
      buffer.subscribing = false
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
        subscribed: false,
        subscribing: false,
        lastRenderedOffset: null,
        snapshotOffset: 0,
        minOffset: 0,
        sessionStopped: false,
        headTrimmed: false,
        historyPreparing: false,
        bufferedLive: [],
        bufferedBytes: 0,
        truncatedNotified: false,
      }
      buffers.set(sessionId, buffer)
    }
    return buffer
  }

  /** base64 → Uint8Array（Rust 事件/历史数据解码） */
  function base64ToBytes(b64: string): Uint8Array {
    if (typeof atob !== 'function') {
      // 测试环境兜底
      return new Uint8Array(0)
    }
    const bin = atob(b64)
    const out = new Uint8Array(bin.length)
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
    return out
  }

  // ==================== 渲染/历史拼接 ====================

  /**
   * 交付实时帧（历史拼接完成后）：去重 → 缺口（重拼接）→ 跨帧裁剪 → 推进游标
   */
  function deliverFrame(sessionId: string, frame: OutputFrame) {
    const buffer = buffers.get(sessionId)
    if (!buffer || buffer.sessionStopped) return

    // 去重：整帧已渲染（endOffset ≤ 游标）跳过
    if (buffer.lastRenderedOffset !== null && frame.endOffset <= buffer.lastRenderedOffset) return

    // 缺口：帧首越过游标（Rust 缓存被淘汰/连接缺口）→ 从游标重新拼接历史补回
    if (buffer.lastRenderedOffset !== null && frame.startOffset > buffer.lastRenderedOffset) {
      const now = Date.now()
      const last = lastGapRespliceAt.get(sessionId) ?? 0
      if (now - last >= GAP_RESPLICE_COOLDOWN_MS) {
        lastGapRespliceAt.set(sessionId, now)
        logger.error(
          `[terminalBuffer] offset gap: frame.start=${frame.startOffset}, last_rendered=${buffer.lastRenderedOffset}. Re-splicing history`,
        )
        forceReplay(sessionId)
      } else {
        // 冷却期内：安排到期兜底重拼接——冷却期 gap 帧被跳过且不触发 forceReplay；
        // 若之后无新 gap 帧（流恰在冷却期终止），尾部缺口将永久残留。定时器到期
        // 主动补一次续传（from=游标），幂等且由 historyPreparing/代数守卫防重
        scheduleGapRetry(sessionId, GAP_RESPLICE_COOLDOWN_MS - (now - last))
      }
      return
    }

    // 跨帧裁剪：帧覆盖已渲染游标（重播首帧跨游标）→ 裁掉前半段，零重复
    const cursor = buffer.lastRenderedOffset ?? frame.startOffset
    let data = frame.data
    if (cursor > frame.startOffset) {
      const overlap = cursor - frame.startOffset
      data = frame.data.subarray(overlap)
    }
    buffer.lastRenderedOffset = Math.max(buffer.lastRenderedOffset ?? 0, frame.endOffset)

    // 插件 TerminalOutput 通知（仅传 session_id 语义；节流）
    const now = Date.now()
    const last = lastActivityAt.get(sessionId) ?? 0
    if (now - last >= ACTIVITY_THROTTLE_MS) {
      lastActivityAt.set(sessionId, now)
      emit('terminal_output_activity', { session_id: sessionId }).catch(() => {})
    }

    realtimeHandlers.get(sessionId)?.onOutput(data, frame)
  }

  /**
   * 历史拼接（registerRealtimeHandler 挂载时启动）：
   * 一次性取 [from, snapshotOffset) 历史（Rust 缓存 / HTTP 回退）→ 写入 xterm →
   * 完成后才开始消费实时帧——「拼接完历史才通知前端终端组件开始消费」
   */
  async function spliceHistory(sessionId: string, gen: number) {
    const buffer = buffers.get(sessionId)
    const handler = realtimeHandlers.get(sessionId)
    if (!buffer || !handler) return

    // P1 竞态防护（spec D-A2 单一真源前提 = WS 重播段已全部入缓存）：
    // phase ∈ {connecting/auth/history} 表示重播未完成，此刻 getHistory 缓存优先会
    // 命中「部分缓存」→ 游标停在部分尾 → 剩余重播段 end ≤ snapshot 静默入缓存、
    // 不推事件 → 缺口存在却无 gap 帧触发自愈（若会话恰无新输出则永久缺口）。
    // 可靠边界 = phase=live（history_end 已收；TCP 有序保证重播帧先于其全部到达）。
    // 订阅失败/超时：复位拼接态，之后实时帧经 deliverFrame 缺口 → forceReplay 自愈。
    if (buffer.phase !== 'live') {
      const ready = await waitForLive(sessionId, SPLICE_WAIT_LIVE_TIMEOUT_MS)
      if (replayGenerations.get(sessionId) !== gen) return
      // 重新读 map 拿最新状态（await 后 buffer 闭包被 TS 静态收窄，且 reactive
      // 值已可能被 onStateEvent 推进）：订阅失败/被取消/停止 → 复位拼接态，
      // 之后实时帧经 deliverFrame 缺口 → forceReplay 自愈
      const current = buffers.get(sessionId)
      if (!ready || !current || current.phase !== 'live' || current.sessionStopped) {
        buffer.historyPreparing = false
        handler.onReplayDone?.()
        return
      }
    }

    const from = buffer.lastRenderedOffset ?? 0
    let result
    try {
      result = await terminalGetHistory(sessionId, from)
    } catch (e: any) {
      logger.warn(`[terminalBuffer] history fetch failed for ${sessionId}:`, e?.message || e)
      buffer.historyPreparing = false
      handler.onReplayDone?.()
      return
    }
    if (replayGenerations.get(sessionId) !== gen) return

    // 截断：驻留历史头部被淘汰（minOffset > 游标）→ 清屏 + 提示 + 锚定重播
    if (buffer.lastRenderedOffset !== null && result.minOffset > buffer.lastRenderedOffset) {
      handler.onClear?.()
      buffer.lastRenderedOffset = null
      buffer.headTrimmed = true
      if (!buffer.truncatedNotified) {
        buffer.truncatedNotified = true
        handler.onTruncated?.(result.minOffset)
      }
    }

    // 写入历史段（写解析完成才推进游标——写入管线确认）
    const history = base64ToBytes(result.dataBase64)
    if (history.byteLength > 0) {
      if (handler.writeParsed) {
        try {
          await handler.writeParsed(history)
        } catch (e: any) {
          // 写入管线异常（极端时序下 xterm 已 dispose）：复位拼接态避免
          // historyPreparing 永久卡死 → 实时帧永久缓冲；游标未推进，缺口由
          // 后续实时帧 gap → forceReplay 自愈
          logger.warn(`[terminalBuffer] history write failed for ${sessionId}:`, e?.message || e)
          buffer.historyPreparing = false
          handler.onReplayDone?.()
          return
        }
      } else {
        handler.onOutput(history, {
          data: history,
          startOffset: Math.max(from, buffer.minOffset),
          endOffset: result.snapshotOffset,
          isWaiting: false,
        })
      }
    }
    if (replayGenerations.get(sessionId) !== gen) return

    buffer.minOffset = result.minOffset
    buffer.snapshotOffset = result.snapshotOffset
    buffer.lastRenderedOffset = Math.max(buffer.lastRenderedOffset ?? 0, result.snapshotOffset)
    if (result.minOffset > 0 && !buffer.headTrimmed) {
      // 历史头部有淘汰（缓存 LL 或 Rust 16MB 上限）：回放起点非流首提示
      buffer.headTrimmed = true
    }

    buffer.historyPreparing = false
    // FLUSH：拼接期缓冲的实时帧按序写入（游标去重/裁剪保序）
    const live = buffer.bufferedLive
    buffer.bufferedLive = []
    buffer.bufferedBytes = 0
    for (const frame of live) {
      deliverFrame(sessionId, frame)
    }
    handler.onReplayDone?.()
  }

  // ==================== Socket/订阅生命周期（Rust 驱动） ====================

  /** 订阅会话（统一入口，幂等）：确保 Rust 链路订阅；页面进出不再控制订阅 */
  async function subscribeSession(sessionId: string): Promise<SubscribeSnapshot | null> {
    const buffer = ensureBuffer(sessionId)
    if (buffer.sessionStopped) return null
    await ensureEventListeners().catch((e) => {
      logger.warn('[terminalBuffer] event listener init failed:', e)
    })

    // 已订阅：直接返回快照元数据（重复调用/页面存活）
    if (buffer.subscribed) {
      return {
        snapshotOffset: buffer.snapshotOffset,
        minOffset: buffer.minOffset,
        historyBytes: 0,
      }
    }
    buffer.subscribing = true
    buffer.phase = 'connecting'
    await terminalSubscribe(sessionId).catch((e) => {
      logger.warn(`[terminalBuffer] subscribe ${sessionId} failed:`, e)
      buffer.subscribing = false
    })
    return null // 订阅确认经 terminal-state 事件异步到达
  }

  function resetMissingStrikes(sessionId: string) {
    // Rust 侧管理会话缺失重试；前端无需计数。保留签名兼容
    void sessionId
  }

  /** 强制重拼接：页面重进/缺口自愈——从当前游标重新取历史（幂等：from=游标） */
  function forceReplay(sessionId: string) {
    const buffer = ensureBuffer(sessionId)
    const gen = (replayGenerations.get(sessionId) ?? 0) + 1
    replayGenerations.set(sessionId, gen)
    buffer.bufferedLive = []
    buffer.bufferedBytes = 0
    buffer.historyPreparing = true
    void spliceHistory(sessionId, gen)
  }

  /** gap 冷却期丢帧兜底：冷却期内的 gap 帧被跳过且不触发重拼接，若之后
   *  无新 gap 帧（流恰在冷却期终止）尾部缺口将永久残留——安排冷却到期后
   *  主动补一次续传重拼接（幂等：from=游标；breaks 由 historyPreparing / 代数守卫）
   */
  function scheduleGapRetry(sessionId: string, delayMs: number) {
    if (gapRetryTimers.has(sessionId)) return
    gapRetryTimers.set(
      sessionId,
      setTimeout(() => {
        gapRetryTimers.delete(sessionId)
        const buffer = buffers.get(sessionId)
        if (!buffer || buffer.sessionStopped || buffer.historyPreparing || buffer.phase !== 'live') return
        forceReplay(sessionId)
      }, delayMs),
    )
  }

  /** 进入终端页 = 全量重播：xterm 每次进入都是全新实例，保留旧游标只续传
   *  [旧游标, tail) 会丢失 [0, 旧游标) 的 scrollback。由 TerminalView onMounted
   *  在 registerRealtimeHandler（其内部 spliceHistory 立即读游标）之前调用；
   *  gap 自愈路径的 forceReplay 不走这里——续传补缺口语义保持不变
   */
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

  /** 渲染背压 ack：推进 Rust 侧 ack 水位（Rust 节流回发桌面端） */
  function ackRendered(sessionId: string) {
    const buffer = buffers.get(sessionId)
    if (!buffer || buffer.lastRenderedOffset === null) return
    terminalAckRendered(sessionId, buffer.lastRenderedOffset).catch((e) => {
      logger.warn(`[terminalBuffer] ack failed for ${sessionId}:`, e)
    })
  }

  /** 退出终端页：清理前端消费态；Rust 订阅保持（会话未停），切批量传播 */
  function markPageLeft(sessionId: string) {
    const buffer = buffers.get(sessionId)
    if (buffer) {
      buffer.bufferedLive = []
      buffer.bufferedBytes = 0
      buffer.historyPreparing = false
    }
    // 双速：回退 batch（满 batch_bytes 才转发——桌面端可配置），减少空转流量
    terminalSetMode(sessionId, 'batch').catch(() => {})
  }

  /** 进入终端页：实时模式（读即传） */
  function markPageEntered(sessionId: string) {
    terminalSetMode(sessionId, 'realtime').catch(() => {})
  }

  /** 标记未订阅（手动取消）：取消 Rust 订阅 + 清实时缓冲，保留游标（重开可续） */
  function markUnsubscribed(sessionId: string) {
    invalidatePrepared(sessionId)
    knownPhases.delete(sessionId)
    terminalUnsubscribe(sessionId).catch(() => {})
    const buffer = buffers.get(sessionId)
    if (buffer) {
      buffer.subscribed = false
      buffer.phase = 'idle'
      buffer.subscribing = false
      buffer.bufferedLive = []
      buffer.bufferedBytes = 0
      buffer.historyPreparing = false
    }
  }

  /** 全部未订阅（设备断开时；Rust 链路全部关闭，重连后由 onPaired 重新订阅） */
  function markAllUnsubscribed() {
    terminalUnsubscribeAll().catch(() => {})
    knownPhases.clear()
    for (const buffer of buffers.values()) {
      buffer.subscribed = false
      buffer.subscribing = false
      buffer.bufferedLive = []
      buffer.bufferedBytes = 0
      buffer.historyPreparing = false
    }
  }

  /** 标记会话停止：取消 Rust 订阅 + 关链路；游标重置（同 id 重启新流坐标空间） */
  function markSessionStopped(sessionId: string) {
    invalidatePrepared(sessionId)
    knownPhases.delete(sessionId)
    // 代数推进：中止在途历史拼接（其完成回调会重写游标/拼接态——停止语义下
    // 游标重置必须占先；页面加载遮罩由视图层 HISTORY_SETTLE_TIMEOUT_MS 兜底）
    replayGenerations.set(sessionId, (replayGenerations.get(sessionId) ?? 0) + 1)
    const buffer = buffers.get(sessionId)
    if (buffer) {
      buffer.sessionStopped = true
      buffer.subscribed = false
      buffer.phase = 'idle'
      buffer.subscribing = false
      buffer.lastRenderedOffset = null
      buffer.bufferedLive = []
      buffer.bufferedBytes = 0
      buffer.historyPreparing = false
    }
    terminalUnsubscribe(sessionId).catch(() => {})
  }

  /** 标记会话恢复运行：重新订阅（Rust 重建链路） */
  function markSessionRunning(sessionId: string) {
    const buffer = buffers.get(sessionId)
    if (buffer) {
      buffer.sessionStopped = false
      buffer.subscribed = false
      buffer.phase = 'idle'
      buffer.subscribing = false
      buffer.lastRenderedOffset = null
      buffer.bufferedLive = []
      buffer.bufferedBytes = 0
      buffer.historyPreparing = false
    }
    void subscribeSession(sessionId)
  }

  /** 清理单个会话（会话删除时）：Rust 链路/缓存一并清 */
  function clearBuffer(sessionId: string) {
    invalidatePrepared(sessionId)
    replayGenerations.set(sessionId, (replayGenerations.get(sessionId) ?? 0) + 1)
    knownPhases.delete(sessionId)
    const gapTimer = gapRetryTimers.get(sessionId)
    if (gapTimer) {
      clearTimeout(gapTimer)
      gapRetryTimers.delete(sessionId)
    }
    terminalRemove(sessionId).catch(() => {})
    buffers.delete(sessionId)
    realtimeHandlers.delete(sessionId)
    lastGapRespliceAt.delete(sessionId)
    lastActivityAt.delete(sessionId)
  }

  /** 清理所有订阅状态（连接断开/退出） */
  function clearAllBuffers() {
    preparedSessionId.value = null
    replayGenerations.clear()
    knownPhases.clear()
    for (const t of gapRetryTimers.values()) clearTimeout(t)
    gapRetryTimers.clear()
    terminalUnsubscribeAll().catch(() => {})
    buffers.clear()
    realtimeHandlers.clear()
    lastGapRespliceAt.clear()
    lastActivityAt.clear()
    if (frameUnlisten) {
      frameUnlisten()
      frameUnlisten = null
    }
    if (stateUnlisten) {
      stateUnlisten()
      stateUnlisten = null
    }
  }

  // ==================== Realtime Handler（TerminalView 挂载/卸载） ====================

  /** 注册实时输出回调（TerminalView onMounted）：启动历史拼接，拼完才消费实时 */
  function registerRealtimeHandler(sessionId: string, handler: RealtimeHandler) {
    const buffer = ensureBuffer(sessionId)
    realtimeHandlers.set(sessionId, handler)
    // 代数推进：重注册使在途拼接循环失效（防双循环交错写入同一 xterm）
    const gen = (replayGenerations.get(sessionId) ?? 0) + 1
    replayGenerations.set(sessionId, gen)

    buffer.historyPreparing = true
    buffer.bufferedLive = []
    buffer.bufferedBytes = 0
    markPageEntered(sessionId)
    // 事件监听惰性注册（订阅成功后可能先于页面挂载；双保险幂等）
    ensureEventListeners().catch((e) => {
      logger.warn('[terminalBuffer] event listener init failed:', e)
    })

    void spliceHistory(sessionId, gen)
  }

  /** 注销实时输出回调（TerminalView onUnmounted）：停止消费；订阅保持，切 batch */
  function unregisterRealtimeHandler(sessionId: string) {
    realtimeHandlers.delete(sessionId)
    replayGenerations.set(sessionId, (replayGenerations.get(sessionId) ?? 0) + 1)
    markPageLeft(sessionId)
  }

  // ==================== Preload ====================

  function markPrepared(sessionId: string) {
    preparedSessionId.value = sessionId
  }

  function consumePrepared(): string | null {
    const id = preparedSessionId.value
    preparedSessionId.value = null
    return id
  }

  function invalidatePrepared(sessionId: string) {
    if (preparedSessionId.value === sessionId) preparedSessionId.value = null
  }

  // ==================== Input ====================

  /** 发送终端输入（经 Rust 终端链路 → WS input 帧 → 桌面端 PTY） */
  function sendInput(sessionId: string, data: string, specialKey?: string): boolean {
    const buffer = buffers.get(sessionId)
    if (!buffer || !buffer.subscribed) {
      logger.warn(`[terminalBuffer] sendInput: session ${sessionId} not subscribed`)
      return false
    }
    terminalSendInput(sessionId, data, specialKey ?? null).catch((e) => {
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
    markPrepared,
    consumePrepared,
    markUnsubscribed,
    markAllUnsubscribed,
    markPageEntered,
    markPageLeft,
    markSessionStopped,
    markSessionRunning,
    forceReplay,
    resetCursor,
    ackRendered,
    clearBuffer,
    clearAllBuffers,
    registerRealtimeHandler,
    unregisterRealtimeHandler,
    subscribeSession,
    resetMissingStrikes,
    sendInput,
  }
})