/**
 * Terminal Buffer Composable
 *
 * TerminalView 用的 composable — 管理会话终端输出订阅与实时输出写入。
 * 订阅由 Rust 链路持有（src-tauri/src/terminal_link.rs，票 05 新插件端点协议）：
 * 进入终端页 fresh subscribe → 插件回放环窗口（历史与实时同一条流，裸字节）；
 * 离开关闭连接。前端只消费段2 Channel 的裸字节并按本地计数回发 ack；
 * `ring_resync`（terminal-resync 事件）是唯一重锚信号（清屏 + 基准重置）。
 * 详见 .scratch/2026-09-26-mobile-desktop-adaptation/spec.md §3.3
 */

import { useTerminalBufferStore } from '@/stores/terminalBuffer'
import { logger } from '@/utils/frontendLogger'
import { createWriteCoalescer } from '@/composables/writeCoalescer'
import { useToast } from '@/composables/useToast'
import i18n from '@/locales'
import type { Terminal } from '@xterm/xterm'

/** 会话页预加载的超时上限（毫秒）：超时不再等待，直接跳转由终端页自行重试 */
const PREPARE_TIMEOUT_MS = 8000

/**
 * 回放静止判定窗口（毫秒，对齐桌面端 TerminalPreview 的 REPLAY_IDLE_MS）：
 * 回放完成后持续这么久无新输出才补一次全量重绘；实时持续输出时计时器
 * 不断重置不会触发
 */
const REPLAY_IDLE_REFRESH_MS = 250

/** sessionId → 回放静止全量重绘定时器（注销时清理，防页面卸载后僵尸刷新） */
const replayIdleTimers = new Map<string, ReturnType<typeof setTimeout>>()

/** sessionId → xterm onWriteParsed 订阅句柄（注销时释放，防同一实例重复注册叠加监听） */
const writeParsedDisposables = new Map<string, { dispose(): void }>()

// ==================== 链路调试统计（渲染背压 ack，2s 节流） ====================

/** sessionId → ack 计数与上次打点时刻（非响应式，纯日志对账用） */
const ackStats = new Map<string, { count: number; lastLogAt: number }>()
/** ack 打点间隔（ms）：onWriteParsed 高频触发，不节流会刷屏 */
const ACK_LOG_INTERVAL_MS = 2000

/** 释放 xterm onWriteParsed 订阅（注销/删除路径必调，防监听泄漏） */
function disposeWriteParsed(sessionId: string) {
  writeParsedDisposables.get(sessionId)?.dispose()
  writeParsedDisposables.delete(sessionId)
}

/** 清理回放静止重绘定时器 */
function clearReplayIdleTimer(sessionId: string) {
  const timer = replayIdleTimers.get(sessionId)
  if (timer) {
    clearTimeout(timer)
    replayIdleTimers.delete(sessionId)
  }
}

/** 让出下一帧渲染（rAF 暂停的后台 WebView / 测试环境立即继续） */
function yieldNextFrame(): Promise<void> {
  return new Promise((resolve) => {
    if (typeof requestAnimationFrame === 'function') {
      requestAnimationFrame(() => resolve())
    } else {
      resolve()
    }
  })
}

// ==================== Types ====================

/** registerRealtimeHandler 返回值：供视图做「渲染完成后再撤加载遮罩」的门控 */
export interface RealtimeHandlerRegistration {
  /**
   * 回放完成信号：新协议无独立历史拼接（回放随订阅流直达），立即 resolve；
   * 门控以 `subscribed`（terminal-state phase=live）为准
   */
  replayDone: Promise<void>
}

// ==================== Write Coalescer ====================
// 为什么需要 rAF 合并写入：
// - TUI 应用（opencode、Claude Code、vim、htop 等）在一次屏幕刷新内会发出大量
//   cursor 定位 + 字符写入的连续转义序列，每个 WS 消息触发一次 terminal.write()
//   都会让 xterm 调度一次 render。
// - xterm.js WebGL 渲染器使用双缓冲，多个异步 render 在同一帧内排队时
//   会出现「前一帧部分内容 + 当前帧新内容」同时可见（鬼影/重影）。
// - 在前端按 rAF 合并多次 terminal.write()：同一帧内所有写入只产生一次
//   render commit，避免双缓冲竞态。DEC 2026 同步输出协议已由 xterm.js 6.0
//   内置处理（应用侧包裹会与 TUI 应用自身 2026 序列嵌套，不再需要）。
// 实现见 @/composables/writeCoalescer

// ==================== Composable ====================

export function useTerminalBuffer() {
  const store = useTerminalBufferStore()
  const toast = useToast()

  /**
   * 注册实时输出 handler — 服务端回放（历史）与实时推送统一经 rAF 合并写入 xterm
   *
   * @param sessionId - 会话 ID
   * @param terminal - xterm Terminal 实例
   * @param onRawOutput - 原始输出字节钩子（合并前、写入前调用，供 TUI 兼容嗅探）
   * @returns 回放完成信号（replayDone），供视图门控加载遮罩
   */
  function registerRealtimeHandler(
    sessionId: string,
    terminal: Terminal,
    onRawOutput?: (data: Uint8Array) => void,
  ): RealtimeHandlerRegistration {
    // 链路调试：渲染管线挂载（拼接历史 → 消费实时帧的入口）
    logger.debug(`[useTerminalBuffer] register realtime handler (${sessionId})`)
    const writeCoalescer = createWriteCoalescer(terminal)
    // 渲染背压（spec 04-06）：写入解析完成 → 回发 ack，让服务端按本端实际
    // 消费速度推进 unacked 记账（64KB 阈值 + 250ms 空闲节流在 socket 内部）。
    // 不设正统门控：onWriteParsed 触发即证明本端正在消费写入管线，任何订阅端
    // 的确认都代表 PTY 字节被消化。此前依赖 isCanonicalRenderer 导致「桌面启动
    // 会话、手机观看」等 resize 未 applied 场景 ack 永不回发 → 服务端 unacked
    // 超高位水 → PTY 读整体暂停 → 输出卡死（2.1.x 修复）。mock 会话/未连接时
    // store.ackRendered → socket.ackRendered 内部空转安全
    writeParsedDisposables.get(sessionId)?.dispose()
    writeParsedDisposables.set(sessionId, terminal.onWriteParsed(() => {
      // 链路调试（背压对账）：onWriteParsed 触发即回发 ack——计数 + 节流打点，
      // offset 与 Rust terminal_link ack 回发日志对照验证反馈环
      const stats = ackStats.get(sessionId) ?? { count: 0, lastLogAt: 0 }
      stats.count++
      const now = Date.now()
      if (now - stats.lastLogAt >= ACK_LOG_INTERVAL_MS) {
        stats.lastLogAt = now
        logger.debug(
          `[useTerminalBuffer] render ack #${stats.count} (${sessionId}): ` +
            `offset=${store.getBuffer(sessionId)?.lastRenderedOffset ?? '-'}`,
        )
      }
      ackStats.set(sessionId, stats)
      store.ackRendered(sessionId)
    }))

    // 回放静止全量重绘兜底：回放起点若落在被 LRU 裁剪的转义序列中段，
    // 增量解析会残留脏屏（光标/属性错位）；连续静止窗口无新数据时补一次整屏
    // refresh（等价用户点击刷新，幂等无副作用）
    let replayIdleTimer: ReturnType<typeof setTimeout> | null = null
    const armReplayIdleRefresh = () => {
      clearReplayIdleTimer(sessionId)
      const timer = setTimeout(() => {
        replayIdleTimer = null
        replayIdleTimers.delete(sessionId)
        // xterm 可能已销毁（页面卸载竞态）：element 已脱离 DOM 则跳过
        if (terminal.element?.isConnected && terminal.rows > 0) {
          terminal.refresh(0, terminal.rows - 1)
        }
      }, REPLAY_IDLE_REFRESH_MS)
      replayIdleTimer = timer
      // 存定时器句柄（非闭包）：clearReplayIdleTimer 靠 clearTimeout 清理，
      // 存闭包会让 clearTimeout 收到函数变成 no-op、僵尸刷新清不掉
      replayIdleTimers.set(sessionId, timer)
    }

    // 回放就绪信号：store 无独立历史拼接（回放随订阅流直达），onReplayDone 立即
    // 触发；此处同步赋值 resolver 不与事件竞态
    let resolveReplayDone: (() => void) | null = null
    const replayDone = new Promise<void>((resolve) => {
      resolveReplayDone = resolve
    })
    // 统一收尾：resolve 后置 null；独立函数避开调用点控制流窄化成 never 的误报
    const settleReplayDone = () => {
      resolveReplayDone?.()
      resolveReplayDone = null
    }

    // 回放（历史）与实时同一条流：registerRealtimeHandler 登记通道与渲染入口，
    // 订阅（fresh subscribe → 回放）由 subscribeWithRetry 触发；输出裸字节
    // 按到达序写入。无独立历史拼接（旧 spliceHistory 语义退役）
    store.registerRealtimeHandler(sessionId, {
      onOutput: (data: Uint8Array) => {
        onRawOutput?.(data)
        writeCoalescer(data)
        // 输出到达即重置静止窗口：持续输出期间不触发补刷
        if (replayIdleTimer) armReplayIdleRefresh()
      },
      onClear: () => {
        // 链路调试：清屏仅发生在重锚重播路径（低频，出现即链路异常信号）
        logger.debug(`[useTerminalBuffer] terminal clear for re-anchored replay (${sessionId})`)
        writeCoalescer.dispose()
        if (terminal) {
          terminal.clear()
        }
      },
      onTruncated: (offset: number) => {
        logger.warn(`[useTerminalBuffer] history truncated at offset=${offset}`)
        // 用户可见后果是「画面被清空 + 重播」（环淘汰/重连重订阅）：必须给出
        // 原因提示，否则看起来像凭空丢内容
        toast.warning(i18n.global.t('mobile.terminal.historyTruncated'))
      },
      onReplayDone: () => {
        armReplayIdleRefresh()
        settleReplayDone()
      },
    })

    return { replayDone }
  }

  /**
   * 注销实时输出 handler
   *
   * @param sessionId - 会话 ID
   */
  function unregisterRealtimeHandler(sessionId: string) {
    ackStats.delete(sessionId)
    clearReplayIdleTimer(sessionId)
    disposeWriteParsed(sessionId)
    store.unregisterRealtimeHandler(sessionId)
  }

  /**
   * 订阅会话 — fresh subscribe（链路已在运行时重播环窗口）；
   * 逻辑收敛到 store（Rust 命令驱动 + terminal-state 事件同步）
   *
   * @param sessionId - 会话 ID
   * @returns 始终 null（订阅确认经事件/对账异步到达）
   */
  async function subscribeSession(sessionId: string): Promise<null> {
    return store.subscribeSession(sessionId)
  }

  /**
   * 退出终端页（页面卸载时调用）— 停止前端消费并**关闭链路**；
   * 重进时 registerRealtimeHandler + fresh subscribe 重播回放
   *
   * @param sessionId - 会话 ID
   */
  async function unsubscribeSession(sessionId: string) {
    clearReplayIdleTimer(sessionId)
    disposeWriteParsed(sessionId)
    store.unregisterRealtimeHandler(sessionId)
  }

  /**
   * 预加载会话输出 — 会话页点击进入终端前的准备：订阅 + 连接（预热）。
   * 终端页挂载时会再次 fresh subscribe 触发回放；本函数只保证「连接已就绪」，
   * 减少首次挂载的握手等待。
   *
   * 返回是否已就绪；失败/超时返回 false，终端页走原有重试路径。
   *
   * @param sessionId - 会话 ID
   */
  async function prepareSession(sessionId: string): Promise<boolean> {
    try {
      // 触发连接（socket 回调异步置 subscribed）
      await store.subscribeSession(sessionId)
      // 轮询等待 subscribed（连接 + 认证 + 订阅往返，通常 <1s）
      const deadline = Date.now() + PREPARE_TIMEOUT_MS
      while (Date.now() < deadline) {
        if (store.getBuffer(sessionId)?.subscribed) {
          return true
        }
        await new Promise((resolve) => setTimeout(resolve, 100))
      }
      return false
    } catch (e) {
      // 订阅失败：不阻塞跳转，终端页自行重试
      logger.warn(`[useTerminalBuffer] Prepare session ${sessionId} failed:`, e)
      return false
    }
  }

  /**
   * 连接断开时 — 标记所有 buffer 未订阅（socket 自动退避重连恢复）
   */
  function handleDisconnect() {
    store.markAllUnsubscribed()
  }

  /**
   * 会话停止时 — 标记 buffer + 关闭 socket。
   *
   * 注意：不注销实时 handler——终端页面可能仍存活，会话重启后输出链路
   * 依赖该 handler 渲染（注销后无任何路径重新注册，页面将永久冻结）。
   * handler 生命周期归视图（挂载注册 / 卸载注销），与会话状态无关
   */
  async function handleSessionStopped(sessionId: string) {
    store.markSessionStopped(sessionId)
  }

  /**
   * 会话恢复运行时 — 复位 sessionStopped（同 id 重启场景：旧流已终止，
   * lastRenderedSeq 已由 markSessionStopped 重置，重新订阅全量重播）
   */
  function markSessionRunning(sessionId: string) {
    store.markSessionRunning(sessionId)
  }

  /**
   * 会话删除时 — 清理 buffer + 关闭 socket
   */
  async function handleSessionRemoved(sessionId: string) {
    clearReplayIdleTimer(sessionId)
    disposeWriteParsed(sessionId)
    store.unregisterRealtimeHandler(sessionId)
    store.clearBuffer(sessionId)
  }

  /** 发送终端输入（经终端 WS input 帧） */
  function sendInput(sessionId: string, data: string, specialKey?: string): boolean {
    return store.sendInput(sessionId, data, specialKey)
  }

  return {
    store,
    registerRealtimeHandler,
    unregisterRealtimeHandler,
    subscribeSession,
    unsubscribeSession,
    prepareSession,
    handleDisconnect,
    handleSessionStopped,
    handleSessionRemoved,
    markSessionRunning,
    sendInput,
  }
}
