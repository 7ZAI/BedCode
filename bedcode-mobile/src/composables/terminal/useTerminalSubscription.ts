/**
 * 终端订阅与历史渲染门控域（TerminalView 拆分产物，移动端特有）
 *
 * 两块职责（都是移动端链路特有，桌面端无对应域）：
 *
 * 1. **订阅重试**：订阅失败（弱网/桌面端重启/超时）时终端会静默空白且无重试路径，
 *    这里做 toast 提示 + 3s 定时重试，成功或页面卸载/断连后停止。
 * 2. **渲染就绪门控**：加载遮罩放行门控——等「订阅 + 回放就绪」再撤遮罩，
 *    避免用户看到内容逐批蹦出的闪烁过程。两条件（票 05：无独立历史拼接，
 *    回放随订阅流直达）：
 *      ① 本地回放完成（registerRealtimeHandler 的 replayDone，新协议立即触发）
 *      ② 服务端就绪：phase 到达 'live'（**收到 subscribed**，票 05 门控信号——
 *         回放环窗口与实时同一条流，无 history_end 边界）。
 *         'connecting'/'auth' 为中间态继续等待——防止首次订阅失败重试期间遮罩
 *         提前撤除、回放随后才逐批蹦出
 *      ③ 首次 fit 校准生效（tryInitialFit 成功或重试放弃）
 *    任一环节卡死（订阅失败/会话停止/极端慢）由 HISTORY_SETTLE_TIMEOUT_MS 兜底。
 */
import type { TerminalKernelContext } from './terminalKernel'
import { watch } from 'vue'
import { logger } from '@/utils/frontendLogger'
import { isMockSession } from '@/composables/useMockTerminal'
import { useToast } from '@/composables/useToast'
import i18n from '@/locales'
import { useTerminalBufferStore } from '@/stores/terminalBuffer'

/** 历史渲染门控整体超时兜底（订阅失败/会话停止/无输出会话时不悬挂遮罩） */
export const HISTORY_SETTLE_TIMEOUT_MS = 8000
/** 订阅失败后的重试间隔（ms） */
export const SUBSCRIBE_RETRY_INTERVAL_MS = 3000

/** 门控放行结果：settled = 三条件齐备；timeout = 兜底超时放行 */
export type HistoryGateResult = 'settled' | 'timeout'

export interface TerminalSubscriptionDeps {
  /** 终端缓冲 store（Pinia 单例，由组件经 useTerminalBuffer 注入） */
  bufferStore: ReturnType<typeof useTerminalBufferStore>
  /** 会话订阅入口（fresh subscribe 语义；订阅确认经事件异步到达） */
  subscribeSession: (sessionId: string) => Promise<null>
}

export function useTerminalSubscription(ctx: TerminalKernelContext, deps: TerminalSubscriptionDeps) {
  const toast = useToast()
  const { bufferStore, subscribeSession } = deps

  // ==================== 历史渲染门控信号 ====================
  let historySettled: Promise<void> = Promise.resolve()
  let settleReplay: (() => void) | null = null
  let settleServerHistory: (() => void) | null = null
  let settleFirstFit: (() => void) | null = null
  /** 门控整体超时兜底定时器（race 结束后清理，防僵尸 timer） */
  let gateTimeoutTimer: ReturnType<typeof setTimeout> | null = null
  let serverGateFallbackTimer: ReturnType<typeof setTimeout> | null = null

  /** 挂载时布防：重建三信号 promise（onUnmounted 后不再复用） */
  function armHistoryGate() {
    historySettled = new Promise<void>((resolve) => {
      let replayDone = false
      let serverDone = false
      let fitDone = false
      const tryResolve = () => {
        if (replayDone && serverDone && fitDone) resolve()
      }
      settleReplay = () => { replayDone = true; tryResolve() }
      settleServerHistory = () => { serverDone = true; tryResolve() }
      settleFirstFit = () => { fitDone = true; tryResolve() }
    })
  }

  /** 本地缓存分片回放完成（registerRealtimeHandler 的 replayDone 接线） */
  function markReplayDone() {
    settleReplay?.()
    settleReplay = null
  }

  /** 首次 fit 校准生效（收敛或重试放弃） */
  function markFirstFitDone() {
    settleFirstFit?.()
    settleFirstFit = null
  }

  /**
   * 订阅就绪监听：phase 到达 'live'（收 subscribed，票 05 门控信号）即放行；
   * 中间态继续等待；连续 HISTORY_SETTLE_TIMEOUT_MS 未到 live（订阅失败重试中/
   * 会话停止/无输出会话）强制放行，避免遮罩悬挂。watch 随组件作用域自动清理，
   * 兜底定时器由 dispose 清理
   */
  function armServerHistoryWatcher() {
    let stop: (() => void) | null = null
    const settle = () => {
      settleServerHistory?.()
      settleServerHistory = null
      if (serverGateFallbackTimer) {
        clearTimeout(serverGateFallbackTimer)
        serverGateFallbackTimer = null
      }
      stop?.()
      stop = null
    }
    stop = watch(
      () => bufferStore.getBuffer(ctx.getSessionId())?.phase,
      (phase) => {
        if (phase === 'live') settle()
      },
      { immediate: true },
    )
    serverGateFallbackTimer = setTimeout(settle, HISTORY_SETTLE_TIMEOUT_MS)
  }

  /**
   * 立即放行「订阅就绪」条件：mock 会话无服务端就绪信号、非活跃/未连接时本次
   * 挂载不会发起订阅，都不存在待等的 subscribed，须立即放行（否则只能等超时兜底）
   */
  function settleServerHistoryNow() {
    settleServerHistory?.()
    settleServerHistory = null
    if (serverGateFallbackTimer) {
      clearTimeout(serverGateFallbackTimer)
      serverGateFallbackTimer = null
    }
  }

  /** 布防门控（三信号 + 服务端历史段监听） */
  function armGate() {
    armHistoryGate()
    armServerHistoryWatcher()
  }

  /**
   * 等待「历史输出渲染完成」门控放行（三条件齐备或兜底超时）。
   * @returns 'settled' = 三条件齐备；'timeout' = 兜底超时放行
   */
  async function waitForHistoryGate(): Promise<HistoryGateResult> {
    const gate = historySettled
    let settledByTimeout = false
    await Promise.race([
      gate,
      new Promise<void>((resolve) => {
        gateTimeoutTimer = setTimeout(() => {
          settledByTimeout = true
          resolve()
        }, HISTORY_SETTLE_TIMEOUT_MS)
      }),
    ])
    if (gateTimeoutTimer !== null) {
      clearTimeout(gateTimeoutTimer)
      gateTimeoutTimer = null
    }
    if (settledByTimeout) {
      logger.warn(`[TerminalView] history gate timeout (${ctx.getSessionId()})`)
    }
    return settledByTimeout ? 'timeout' : 'settled'
  }

  // ==================== 订阅重试 ====================

  let subscribeRetryTimer: ReturnType<typeof setTimeout> | null = null
  let subscribeRetryToasted = false
  let disposed = false

  function clearSubscribeRetry() {
    if (subscribeRetryTimer) {
      clearTimeout(subscribeRetryTimer)
      subscribeRetryTimer = null
    }
  }

  /** 订阅 + 失败自动重试（页面存活且会话活跃期间有效） */
  async function subscribeWithRetry() {
    const sid = ctx.getSessionId()
    if (!sid || isMockSession(sid)) return
    // fresh subscribe（重播回放）：无论信念如何都触发一次；已订阅时 Rust
    // 重播后事件/对账收敛，未订阅时可立即建立链路
    await subscribeSession(sid)
    const buffer = bufferStore.getBuffer(sid)

    // 已订阅（成功或此前已订阅）：复位重试状态
    if (buffer?.subscribed) {
      subscribeRetryToasted = false
      return
    }
    // 订阅请求仍在途（防重早退）：不提示，稍后重试
    if (buffer?.subscribing) {
      scheduleSubscribeRetry(sid)
      return
    }

    // 订阅失败：首次失败提示一次，随后静默重试
    if (!subscribeRetryToasted) {
      subscribeRetryToasted = true
      toast.error(i18n.global.t('mobile.terminal.subscribeFailed'))
    }
    scheduleSubscribeRetry(sid)
  }

  /**
   * 定时重试（失败 / 订阅在途两路径共用）：到期重试前先过订阅收敛守卫——
   * 一旦 subscribed 事件/对账已到达（信念置真），不再重播（fresh subscribe
   * 会触发环窗口重播 + 清屏重刷，页面已渲染后再刷一次即闪烁）
   */
  function scheduleSubscribeRetry(sid: string) {
    clearSubscribeRetry()
    subscribeRetryTimer = setTimeout(async () => {
      subscribeRetryTimer = null
      if (disposed) return
      if (!ctx.isConnected() || !ctx.isSessionActive()) return
      if (bufferStore.getBuffer(sid)?.subscribed) return
      await subscribeWithRetry()
    }, SUBSCRIBE_RETRY_INTERVAL_MS)
  }

  /** 组件卸载清理：重试定时器 + 门控兜底定时器（放行信号不再需要） */
  function disposeSubscription() {
    disposed = true
    clearSubscribeRetry()
    if (serverGateFallbackTimer) {
      clearTimeout(serverGateFallbackTimer)
      serverGateFallbackTimer = null
    }
    if (gateTimeoutTimer) {
      clearTimeout(gateTimeoutTimer)
      gateTimeoutTimer = null
    }
    settleReplay = null
    settleServerHistory = null
    settleFirstFit = null
  }

  return {
    armGate,
    markReplayDone,
    markFirstFitDone,
    settleServerHistoryNow,
    waitForHistoryGate,
    subscribeWithRetry,
    clearSubscribeRetry,
    disposeSubscription,
  }
}
