/**
 * Terminal Buffer Store
 *
 * 全局终端输出订阅状态 — 数据真源在服务端（环形输出队列），
 * 前端只维护字节游标（已渲染位置），不再缓存输出字节。
 * 历史回放由服务端裁决（incremental 续传 / reset 全量重播）后流式推送。
 */

import { defineStore } from 'pinia'
import { reactive, ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

// ==================== Types ====================

/** ws_output 事件载荷 */
export interface OutputPayload {
  session_id: string
  data_base64: string
  index: number
  end_index?: number
  is_waiting: boolean
  /** 字节偏移（会话流坐标）——新服务端发送；旧版缺失时退化为无游标透传 */
  start_offset?: number
  end_offset?: number
}

/** 订阅裁决（服务端告知，消费者零猜测） */
export interface SubscribeResultInfo {
  minSeq: number
  maxSeq: number
  historyCount: number
  mode: 'incremental' | 'reset'
  minOffset: number
  maxOffset: number
}

/** 实时输出回调 — TerminalView 注册 */
export interface RealtimeHandler {
  onOutput: (data: Uint8Array, payload: OutputPayload) => void
  /** 订阅裁决 reset（游标失效/全量重播）时调用，TerminalView 应清空 xterm */
  onClear?: () => void
}

/** 单会话订阅状态（无本地数据缓存） */
export interface SessionBuffer {
  /** 已渲染到的字节偏移（游标），-1 = 尚未渲染过（首次订阅全量重播） */
  cursor: number
  /** 该会话是否已向后端订阅 */
  subscribed: boolean
  /** 订阅请求已发出、响应未返回（防止并发重复订阅） */
  subscribing: boolean
  /** 会话是否已停止 */
  sessionStopped: boolean
  /** 订阅确认前缓冲的回放帧（裁决消息与历史帧经不同消息路径，顺序无保证） */
  pending: OutputPayload[]
  /** 缓冲帧总字节数（防御性上限，超限重置订阅） */
  pendingBytes: number
  /** 页面重进已请求全量重播：在途订阅（旧游标）完成后须以重置游标重订阅 */
  replayRequested: boolean
}

// ==================== Store ====================

export const useTerminalBufferStore = defineStore('terminalBuffer', () => {
  // ==================== State ====================

  /** sessionId → 订阅状态 */
  const buffers = reactive(new Map<string, SessionBuffer>())

  /** sessionId → 实时回调（TerminalView 注册的） */
  const realtimeHandlers = reactive(new Map<string, RealtimeHandler>())

  /** 全局 ws_output 监听器 unlisten 函数 */
  const unlistenRef = ref<UnlistenFn | null>(null)
  /** 是否已启动全局监听 */
  let listenerStarted = false
  /** 监听注册中的 promise（并发调用共享同一注册；失败复位允许重试） */
  let listenerPromise: Promise<void> | null = null

  /** 订阅确认前缓冲回放帧的上限（防御性；服务端环形容量远小于此） */
  const MAX_PENDING_FRAME_BYTES = 8 * 1024 * 1024

  /** 自愈重订阅冷却间隔下限（毫秒）：限制同一会话连续性自愈的频率 */
  const RESUBSCRIBE_COOLDOWN_MIN_MS = 2000
  /** 自愈重订阅冷却间隔上限（毫秒）：连续自愈风暴（violation 循环）时指数退避封顶 */
  const RESUBSCRIBE_COOLDOWN_MAX_MS = 30000
  /** 连续自愈计数复位窗口（毫秒）：超过该时长无自愈，视为风暴结束，退避计数清零 */
  const RESUBSCRIBE_STREAK_RESET_MS = 60000
  /** sessionId → 上次 resubscribeWithReset 的时间戳 */
  const lastResubscribeAt = reactive(new Map<string, number>())
  /** sessionId → 连续自愈次数（指数退避用，风暴平息后经 STREAK_RESET 窗口清零） */
  const resubscribeStreak = reactive(new Map<string, number>())
  /** sessionId → 冷却被挡日志限频时间戳（风暴中每帧 violation 都会走到这里，2s 一条避免刷屏） */
  const lastCooldownLogAt = reactive(new Map<string, number>())

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

  /** 使预加载标记失效：会话状态被重置（自愈重订阅/断连/停止/清空）后，
   *  已缓冲回放帧不可信（被丢弃/游标失效），终端页挂载时必须走 forceReplay 兜底 */
  function invalidatePrepared(sessionId: string) {
    if (preparedSessionId.value === sessionId) preparedSessionId.value = null
  }
  // ==================== Global Listener ====================

  /** 启动全局 ws_output 监听器（只启动一次；返回注册完成 promise） */
  function startGlobalListener(): Promise<void> {
    if (listenerStarted) return Promise.resolve()
    if (listenerPromise) return listenerPromise

    listenerStarted = true
    listenerPromise = (async () => {
      try {
        unlistenRef.value = await listen<OutputPayload>('ws_output', (event) => {
          const payload = event.payload
          const sessionId = payload.session_id
          const buffer = buffers.get(sessionId)

          // 没有 buffer 的会话忽略（未被任何终端访问过）
          if (!buffer) return

          // 会话已停止后不再接收
          if (buffer.sessionStopped) return

          const handler = realtimeHandlers.get(sessionId)

          // 旧版服务端（无字节偏移）：无法维护游标，仅透传（兼容路径）
          if (payload.start_offset === undefined || payload.end_offset === undefined) {
            const data = decodeBase64(payload.data_base64)
            handler?.onOutput(data, payload)
            return
          }

          // 订阅确认前到达的回放帧：缓冲，确认后按序写入。
          // 服务端 SubscribeResult 与历史帧经不同 actor 消息路径发送，顺序无保证，
          // 帧可能先于裁决消息到达；直接写入会在 reset 清屏时被错误清除
          if (!buffer.subscribed) {
            buffer.pending.push(payload)
            buffer.pendingBytes += payload.data_base64.length
            if (buffer.pendingBytes > MAX_PENDING_FRAME_BYTES) {
              console.error('[terminalBuffer] pending frame overflow, resubscribing with reset')
              resubscribeWithReset(sessionId, buffer)
            }
            return
          }

          // 已订阅但无实时 handler（会话页预加载）：缓冲帧等待终端页挂载后写入，
          // 与订阅确认前缓冲同队列（pending），注册 handler 时统一排空
          if (!handler) {
            bufferFrameForPendingHandler(sessionId, buffer, payload)
            return
          }

          deliverFrame(sessionId, buffer, handler, payload)
        })
      } catch (e) {
        // 注册失败：复位标志，允许下次调用重试（订阅路径 await 后失败走重试）
        listenerStarted = false
        listenerPromise = null
        throw e
      }
    })()

    return listenerPromise
  }

  /** 交付帧：先做连续性校验，再推进游标并回调；返回是否成功交付 */
  function deliverFrame(
    sessionId: string,
    buffer: SessionBuffer,
    handler: RealtimeHandler | undefined,
    payload: OutputPayload,
  ): boolean {
    // 防御：无偏移帧（旧版服务端）不应到达此处——监听器已提前透传，
    // 缓冲帧也只收录带偏移的帧
    if (payload.start_offset === undefined || payload.end_offset === undefined) {
      console.warn('[terminalBuffer] deliverFrame received frame without offsets, skipping')
      return true
    }

    // 连续性校验（防御）：服务端契约保证帧间字节连续，
    // 违反即不变量破坏 → 清屏 + 丢弃游标 + 重新订阅（服务端裁决 reset 全量重播）
    if (buffer.cursor >= 0 && payload.start_offset !== buffer.cursor) {
      console.error(
        `[terminalBuffer] continuity violation: start=${payload.start_offset}, cursor=${buffer.cursor}. Resubscribing with reset`
      )
      resubscribeWithReset(sessionId, buffer)
      return false
    }

    // 游标推进到帧尾（= 已渲染位置）
    buffer.cursor = payload.end_offset
    const data = decodeBase64(payload.data_base64)
    handler?.onOutput(data, payload)
    return true
  }

  /**
   * 订阅确认后排空缓冲的回放帧（按到达顺序写入，保持字节连续）
   *
   * @param skipUpToOffset - 服务端裁决响应携带的快照 max_offset：end_offset <= 该值
   *   的帧（旧流残留/回放前缀）已含在服务端历史快照内，跳过避免重复写入；
   *   大于该值的帧是快照后的实时帧，不在回放内，保留写入
   */
  function flushPending(sessionId: string, buffer: SessionBuffer, skipUpToOffset?: number) {
    const frames = buffer.pending
    buffer.pending = []
    buffer.pendingBytes = 0
    if (frames.length === 0) return

    const handler = realtimeHandlers.get(sessionId)
    for (const payload of frames) {
      // 快照已覆盖的帧：跳过（服务端回放会重发这些字节，写入即重复）
      if (
        skipUpToOffset !== undefined &&
        payload.end_offset !== undefined &&
        payload.end_offset <= skipUpToOffset
      ) {
        continue
      }
      // 连续性不变量破坏：剩余帧丢弃，交给重订阅的全量回放
      if (!deliverFrame(sessionId, buffer, handler, payload)) break
    }
  }

  /**
   * 发起订阅请求（核心逻辑；调用方负责 ensureBuffer 与前置状态检查）
   *
   * 订阅确认后统一顺序：reset → 清屏 + 游标重置 → 标记已订阅 → 排空缓冲帧。
   * 失败时不抛出：丢弃缓冲帧、保持未订阅，由外部生命周期（重连 / 页面重进）重试。
   *
   * 重播用循环而非递归：forceReplay 竞态下需以重置游标再订阅一次，
   * 递归会在重播订阅在途时提前清 subscribing 标志，并发调用方可绕过
   * 防重（重复订阅 + 双清屏）；循环保持 subscribing 贯穿全部重播轮次
   */
  async function doSubscribe(
    sessionId: string,
    buffer: SessionBuffer,
  ): Promise<SubscribeResultInfo | null> {
    // 已订阅或订阅请求在途：跳过（防止并发重复订阅）
    if (buffer.subscribed || buffer.subscribing) return null

    buffer.subscribing = true
    try {
      for (;;) {
        // 注册等待：确保 ws_output 监听已就绪再发订阅请求，否则回放帧
        // （Tauri 事件不缓冲）会在监听注册前到达而丢失
        await startGlobalListener()

        // 字节游标：上次渲染到的位置；-1（未渲染过）→ 首次全量重播
        const requestedCursor = buffer.cursor >= 0 ? buffer.cursor : undefined
        const result = await subscribeRemote(sessionId, requestedCursor)

        // 服务端裁决 reset：游标已失效，清屏后等待全量回放帧
        if (result.mode === 'reset') {
          buffer.cursor = -1
          // 丢弃确认前缓冲的旧流残留帧：reset 裁决意味着游标失效，而订阅请求
          // 往返期间到达的帧只能来自被替换/中止的旧订阅流（服务端响应先于新
          // 回放帧发送，新回放帧必然晚于响应）。排空旧流残留帧会推进游标，
          // 新回放首帧（快照点起播）必然违反连续性 → 重订阅自持循环
          buffer.pending = []
          buffer.pendingBytes = 0
          const handler = realtimeHandlers.get(sessionId)
          handler?.onClear?.()
        }

        // 订阅建立：标记已订阅（后续 ws_output 帧按序写入）
        buffer.subscribed = true
        // incremental：缓冲帧跳过快照已覆盖部分后排空写入（服务端历史快照
        // 与订阅往返期间先到的帧字节重叠——不跳过则重复写入触发连续性自愈
        // 闪屏）；reset：缓冲帧已丢弃，回放帧随后按序到达（游标 -1 首帧锚定）
        if (result.mode !== 'reset') {
          flushPending(sessionId, buffer, result.maxOffset)
        }

        // forceReplay 与在途订阅竞态：订阅期间游标被重置为 -1（页面重进 /
        // 连续性自愈早退），但本次裁决按旧游标 incremental 完成——旧游标续传
        // 会丢失历史，须以重置游标重订阅一次（cursor=-1 → 服务端 reset 全量重播）
        if (buffer.replayRequested && requestedCursor !== undefined) {
          buffer.replayRequested = false
          buffer.cursor = -1
          buffer.subscribed = false
          buffer.pending = []
          buffer.pendingBytes = 0
          continue
        }
        // 已按 -1 全量重播（或本次请求本就无游标）：消费重播标记
        buffer.replayRequested = false
        return result
      }
    } catch (e) {
      // 订阅失败：丢弃缓冲帧（订阅未建立，旧帧无意义），保持未订阅允许外部重试
      buffer.pending = []
      buffer.pendingBytes = 0
      console.warn(`[terminalBuffer] Subscribe session ${sessionId} failed:`, e)
      return null
    } finally {
      buffer.subscribing = false
    }
  }

  /**
   * 订阅会话（统一入口，幂等）— 页面进入 / 重连恢复 / 自愈全部收敛于此
   *
   * @returns 订阅裁决信息；已订阅 / 订阅请求在途 / 订阅失败时返回 null
   */
  async function subscribeSession(sessionId: string): Promise<SubscribeResultInfo | null> {
    const buffer = ensureBuffer(sessionId)
    return doSubscribe(sessionId, buffer)
  }

  /**
   * 连续性不变量破坏后的自愈：丢弃游标与缓冲帧，重新订阅（服务端给正确答案）
   *
   * 指数退避冷却：连续违反（"违反 → 重订阅 → 重复流 → 再违反"自持风暴）时
   * 冷却从 2s 指数递增到 30s 封顶（窗口内无自愈则复位），限制清屏+全量重播
   * 的频率——每次自愈都清屏，高频下表现为终端闪烁/白屏（TUI 应用全屏重绘时
   * 最明显）。冷却期内违反帧仍被拒绝写入，风暴随退避自然平息，一次重订阅
   * 即可恢复
   */
  async function resubscribeWithReset(
    sessionId: string,
    buffer: SessionBuffer,
  ) {
    invalidatePrepared(sessionId)
    const now = Date.now()
    const last = lastResubscribeAt.get(sessionId) ?? 0
    // 退避计数：风暴平息（RESET 窗口内无自愈）后清零
    if (now - last > RESUBSCRIBE_STREAK_RESET_MS) {
      resubscribeStreak.set(sessionId, 0)
    }
    const streak = resubscribeStreak.get(sessionId) ?? 0
    const cooldown = Math.min(
      RESUBSCRIBE_COOLDOWN_MIN_MS * 2 ** Math.min(streak, 4),
      RESUBSCRIBE_COOLDOWN_MAX_MS,
    )
    // 冷却期内不重订阅，且不刷新 last/streak：被挡住的调用只是拒绝当前帧，
    // 若也推进退避计数，持续 violating 流会每帧刷新 last → 复位窗口（60s）
    // 永不满足 → streak 永不清零 → 冷却恒 30s 封顶 → 重订阅永不执行 →
    // 终端永久黑屏（2026-08-15 实测：cursor 卡死数分钟，每次 violation 一条 ERROR）
    if (now - last < cooldown) {
      // 限频日志：风暴中每帧 violation 都会走到这里，2s 一条避免刷屏
      const lastLog = lastCooldownLogAt.get(sessionId) ?? 0
      if (now - lastLog > 2000) {
        lastCooldownLogAt.set(sessionId, now)
        console.warn(
          `[terminalBuffer] resubscribe cooled down (streak=${streak}, retry in ${Math.ceil((cooldown - (now - last)) / 1000)}s), frames rejected`
        )
      }
      return
    }

    // 真正执行重订阅时才记录时间与风暴计数
    resubscribeStreak.set(sessionId, streak + 1)
    lastResubscribeAt.set(sessionId, now)

    buffer.cursor = -1
    buffer.subscribed = false
    buffer.pending = []
    buffer.pendingBytes = 0
    // 不在此清屏：真正清屏时机由 doSubscribe 的 reset 裁决分支决定（订阅确认后、
    // 全量回放帧到达前）。提前清屏会把黑屏窗口拉长到整个订阅往返耗时，
    // 且重订阅失败/超时时留下永久黑屏（旧内容本可继续展示到裁决到达）
    // 原订阅请求仍在途：标记重播请求（其按旧游标 incremental 裁决完成时，
    // doSubscribe 会以重置游标重订阅全量重播），不再发起新订阅
    if (buffer.subscribing) {
      buffer.replayRequested = true
      return
    }
    await doSubscribe(sessionId, buffer)
  }

  /** 停止全局监听器 */
  function stopGlobalListener() {
    if (unlistenRef.value) {
      unlistenRef.value()
      unlistenRef.value = null
    }
    listenerStarted = false
  }

  // ==================== Buffer Operations ====================

  /** 确保会话有订阅状态，不存在则创建 */
  function ensureBuffer(sessionId: string): SessionBuffer {
    let buffer = buffers.get(sessionId)
    if (!buffer) {
      buffer = {
        cursor: -1,
        subscribed: false,
        subscribing: false,
        sessionStopped: false,
        pending: [],
        pendingBytes: 0,
        replayRequested: false,
      }
      buffers.set(sessionId, buffer)
      // 有 buffer 时需要全局监听器（注册失败由订阅路径的 await 兜底重试）
      startGlobalListener().catch((e) => {
        console.warn('[terminalBuffer] Global listener start failed:', e)
      })
    }
    return buffer
  }

  /**
   * 强制全量重播：页面重进时 xterm 为全新实例，旧游标续传会丢失历史
   * （游标已被推进过的字节从未渲染过），重置游标与订阅状态后，
   * 下次订阅服务端裁决 reset 全量重播（优先清屏快照点起播）
   */
  function forceReplay(sessionId: string) {
    const buffer = ensureBuffer(sessionId)
    buffer.cursor = -1
    buffer.subscribed = false
    buffer.pending = []
    buffer.pendingBytes = 0
    buffer.replayRequested = true
  }

  /** 获取会话订阅状态 */
  function getBuffer(sessionId: string): SessionBuffer | undefined {
    return buffers.get(sessionId)
  }

  /** 标记已订阅后端 */
  function markSubscribed(sessionId: string) {
    const buffer = ensureBuffer(sessionId)
    buffer.subscribed = true
  }

  /** 标记未订阅（断连/取消订阅时）；订阅解除后缓冲帧无意义，一并丢弃 */
  function markUnsubscribed(sessionId: string) {
    invalidatePrepared(sessionId)
    const buffer = buffers.get(sessionId)
    if (buffer) {
      buffer.subscribed = false
      buffer.pending = []
      buffer.pendingBytes = 0
    }
  }

  /** 标记所有 buffer 未订阅（连接断开时） */
  function markAllUnsubscribed() {
    for (const sessionId of buffers.keys()) {
      invalidatePrepared(sessionId)
    }
    for (const buffer of buffers.values()) {
      buffer.subscribed = false
      buffer.pending = []
      buffer.pendingBytes = 0
    }
  }

  /**
   * 标记会话停止：游标与订阅状态一并失效。
   * 会话重启后偏移空间从 0 重建（新 SessionOutputManager），旧游标续传会
   * 渲染新流的中段——游标必须重置为 -1，重启后订阅走服务端 reset 全量重播
   */
  function markSessionStopped(sessionId: string) {
    invalidatePrepared(sessionId)
    const buffer = buffers.get(sessionId)
    if (buffer) {
      buffer.sessionStopped = true
      buffer.subscribed = false
      buffer.cursor = -1
      buffer.pending = []
      buffer.pendingBytes = 0
    }
  }

  /**
   * 标记会话恢复运行：复位 sessionStopped（会话停止后重启，旧流已终止、
   * 偏移空间重建——不复位则 ws_output 监听器永久丢弃新流帧，终端冻结；
   * 游标一并失效，重启后的订阅走服务端 reset 全量重播校准）
   *
   * 不主动复位 subscribing：若旧订阅在途（断连重连窗口），此处复位会让
   * doSubscribe 的 finally 误清新订阅的防重标志，存在双订阅窗口；保持现状
   * 由旧订阅自然结束后新订阅重试，极端情况下靠连续性 violation 自愈兜底
   */
  function markSessionRunning(sessionId: string) {
    const buffer = buffers.get(sessionId)
    if (buffer) {
      buffer.sessionStopped = false
      buffer.subscribed = false
      buffer.cursor = -1
      buffer.pending = []
      buffer.pendingBytes = 0
    }
  }

  /** 清理单个会话订阅状态 */
  function clearBuffer(sessionId: string) {
    invalidatePrepared(sessionId)
    buffers.delete(sessionId)
    realtimeHandlers.delete(sessionId)
    lastResubscribeAt.delete(sessionId)
    // 所有 buffer 都清理后，关闭全局监听器
    if (buffers.size === 0) {
      stopGlobalListener()
    }
  }

  /** 清理所有订阅状态 */
  function clearAllBuffers() {
    preparedSessionId.value = null
    buffers.clear()
    realtimeHandlers.clear()
    stopGlobalListener()
  }

  // ==================== Realtime Handler ====================

  /** 注册实时输出回调（TerminalView onMounted 时调用） */
  function registerRealtimeHandler(sessionId: string, handler: RealtimeHandler) {
    realtimeHandlers.set(sessionId, handler)

    // 预加载（会话页订阅后无 handler）期间缓冲的回放帧：回退游标到首帧起点后
    // 按到达顺序写入（帧在缓冲时已通过连续性校验，顺序即字节连续）。
    // 仅排空「已订阅但无 handler」路径的帧；未订阅缓冲的帧属订阅确认前残留，
    // 仍由 doSubscribe 的 flushPending 统一处理（reset 裁决时会被丢弃）
    const buffer = buffers.get(sessionId)
    if (buffer && buffer.subscribed && buffer.pending.length > 0) {
      const first = buffer.pending[0]
      if (first.start_offset !== undefined) buffer.cursor = first.start_offset
      flushPending(sessionId, buffer)
    }
  }

  /** 已订阅但无 handler 时的缓冲写入（连续性校验 + 上限保护） */
  function bufferFrameForPendingHandler(
    sessionId: string,
    buffer: SessionBuffer,
    payload: OutputPayload,
  ) {
    // 连续性校验（与 deliverFrame 一致）：破坏时走自愈重订阅（丢弃游标与
    // 缓冲帧，重置后全量回放），不变量保持与有 handler 时完全一致
    if (buffer.cursor >= 0 && payload.start_offset !== buffer.cursor) {
      console.warn(
        `[terminalBuffer] continuity violation (no handler): start=${payload.start_offset}, cursor=${buffer.cursor}. Resubscribing with reset`
      )
      resubscribeWithReset(sessionId, buffer)
      return
    }
    buffer.cursor = payload.end_offset ?? buffer.cursor
    buffer.pending.push(payload)
    buffer.pendingBytes += payload.data_base64.length
    // 无 handler 无处渲染：超限丢弃缓冲会留下中间缺口 → 与连续性违反同路径
    // 自愈（重置游标全量重播）；自愈被冷却挡住时，预加载标记已失效，终端页
    // 挂载走 forceReplay 兜底
    if (buffer.pendingBytes > MAX_PENDING_FRAME_BYTES) {
      console.warn(
        `[terminalBuffer] preload frame overflow (session ${sessionId}), resubscribing with reset`
      )
      resubscribeWithReset(sessionId, buffer)
    }
  }

  /** 注销实时输出回调（TerminalView onUnmounted 时调用） */
  function unregisterRealtimeHandler(sessionId: string) {
    realtimeHandlers.delete(sessionId)
  }

  // ==================== Utility ====================

  /** Base64 解码为 Uint8Array */
  function decodeBase64(base64: string): Uint8Array {
    const binary = atob(base64)
    const bytes = new Uint8Array(binary.length)
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i)
    }
    return bytes
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
    clearBuffer,
    clearAllBuffers,
    registerRealtimeHandler,
    unregisterRealtimeHandler,
    startGlobalListener,
    subscribeSession,
  }
})

// ==================== Remote Subscription ====================
// 订阅动作收敛到 store：连续性不变量破坏时可在监听器内自愈（重新订阅）

import { invoke } from '@tauri-apps/api/core'

/** 远端订阅（ws_subscribe_session），返回服务端裁决 */
async function subscribeRemote(sessionId: string, cursor: number | undefined) {
  const result = await invoke<SubscribeResultInfo>('ws_subscribe_session', {
    sessionId,
    startSeq: cursor === undefined ? null : cursor,
  })
  return result
}

// 供 composable 复用
export { subscribeRemote }
