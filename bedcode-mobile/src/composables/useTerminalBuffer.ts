/**
 * Terminal Buffer Composable
 *
 * TerminalView 用的 composable — 管理会话终端 WS 订阅与实时输出写入。
 * 输出为 TB v2 二进制帧（前端直连桌面端终端会话路由），数据真源在服务端；
 * 前端维护 lastRenderedSeq（去重基准）+ 历史缓存（页面重进回放源）。
 */

import { useTerminalBufferStore, type SubscribeResultInfo } from '@/stores/terminalBuffer'
import { createWriteCoalescer } from '@/composables/writeCoalescer'
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

export type { SubscribeResultInfo } from '@/stores/terminalBuffer'

/** registerRealtimeHandler 返回值：供视图做「历史渲染完成后再撤加载遮罩」的门控 */
export interface RealtimeHandlerRegistration {
  /**
   * 本地历史缓存分片回放完成的信号：
   * - 有缓存：末批解析完成（onReplayDone）时 resolve
   * - 无缓存（mock 会话 / 首次进入尚未订阅）：立即 resolve，
   *   服务端历史段结束由视图按 buffer.phase 离开 'history' 推导
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
    /** 渲染背压门控：仅本端为会话正统渲染端时回发 ack（非正统时服务端丢弃，白红流量） */
    shouldAck?: () => boolean,
  ): RealtimeHandlerRegistration {
    const writeCoalescer = createWriteCoalescer(terminal)
    // 渲染背压（spec 04-06）：写入解析完成 → 回发 ack，让服务端按本端实际
    // 消费速度推进 unacked 记账（64KB 阈值 + 250ms 空闲节流在 socket 内部）。
    // shouldAck 门控：仅本端为会话正统渲染端时发送（非正统时服务端丢弃）
    terminal.onWriteParsed(() => {
      if (shouldAck?.()) {
        store.ackRendered(sessionId)
      }
    })

    // 分片回放高水位写入（store 回放循环的背压信号）：合并批经 terminal.write
    // 的回调确认「已解析完成」，再让出一帧渲染才 resolve——回放节奏由本端
    // xterm 实际消费速度决定，历史回放期间渲染/触摸可插入，不再长冻结。
    // xterm 内部写队列 FIFO 保序：回放批与实时帧交错入队不破坏输出顺序
    const writeParsed = (data: Uint8Array): Promise<void> =>
      new Promise<void>((resolve) => {
        // terminal 可能已 dispose（页面切换/会话关闭）：与 writeCoalescer 守卫一致
        if (!terminal.element) {
          resolve()
          return
        }
        // 回放路径同样必须喂原始字节钩子（TUI 嗅探）：DECSET 1006h 若只出现在
        // 历史缓存中（进入会话前 TUI 应用已启用鼠标上报），不喂则嗅探器状态
        // 丢失 → isTuiMode 误判关闭 → 触摸滚动落在无 scrollback 的备用屏幕上
        // 完全失效（opencode 等进入后无法滚动查看的根因）
        onRawOutput?.(data)
        terminal.write(data, () => resolve())
      }).then(() => yieldNextFrame())

    // 回放静止全量重绘兜底：历史起点若落在被 LRU 裁剪的转义序列中段，
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

    // 回放就绪信号：本地缓存分片回放完成（onReplayDone）时 resolve。
    // store 层 onReplayDone 最早也在 writeParsed 的异步链之后触发，
    // 此处同步赋值 resolver 不会与回放收尾竞态
    let resolveReplayDone: (() => void) | null = null
    const replayDone = new Promise<void>((resolve) => {
      resolveReplayDone = resolve
    })
    // 统一收尾：resolve 后置 null；独立函数避开调用点控制流窄化成 never 的误报
    const settleReplayDone = () => {
      resolveReplayDone?.()
      resolveReplayDone = null
    }

    store.registerRealtimeHandler(sessionId, {
      onOutput: (data: Uint8Array) => {
        onRawOutput?.(data)
        writeCoalescer(data)
        // 输出到达即重置静止窗口：持续输出期间不触发补刷
        if (replayIdleTimer) armReplayIdleRefresh()
      },
      writeParsed,
      onClear: () => {
        writeCoalescer.dispose()
        if (terminal) {
          terminal.clear()
        }
      },
      onTruncated: (minSeq: number) => {
        console.warn(`[useTerminalBuffer] history truncated at min_seq=${minSeq}`)
      },
      onReplayDone: () => {
        armReplayIdleRefresh()
        settleReplayDone()
      },
    })

    // 无本地缓存（mock 会话 / 首次进入尚未订阅）：分片回放不会启动，
    // replayDone 立即完成——服务端历史段结束由视图按 phase 离开 'history' 推导
    const buffer = store.getBuffer(sessionId)
    if (!buffer || buffer.historyCache.length === 0) {
      settleReplayDone()
    }

    return { replayDone }
  }

  /**
   * 注销实时输出 handler
   *
   * @param sessionId - 会话 ID
   */
  function unregisterRealtimeHandler(sessionId: string) {
    clearReplayIdleTimer(sessionId)
    store.unregisterRealtimeHandler(sessionId)
  }

  /**
   * 订阅会话 — 已订阅/在途则跳过；逻辑收敛到 store（socket 驱动 + 快照拼接），
   * 所有订阅路径（页面进入 / 重连恢复）统一入口
   *
   * @param sessionId - 会话 ID
   * @returns 订阅确认信息（已订阅时）；连接建立中/失败时返回 null
   */
  async function subscribeSession(sessionId: string): Promise<SubscribeResultInfo | null> {
    return store.subscribeSession(sessionId)
  }

  /**
   * 取消订阅会话（页面卸载/会话停止/删除时调用）— 关闭终端 socket
   *
   * @param sessionId - 会话 ID
   */
  async function unsubscribeSession(sessionId: string) {
    clearReplayIdleTimer(sessionId)
    store.unregisterRealtimeHandler(sessionId)
    store.markUnsubscribed(sessionId)
  }

  /**
   * 强制全量重播 — 页面重进时 xterm 为全新实例。历史缓存由
   * registerRealtimeHandler 回放；连接存活时重发订阅（快照重播按 seq 跳过）
   *
   * @param sessionId - 会话 ID
   */
  function forceReplay(sessionId: string) {
    store.forceReplay(sessionId)
  }

  /**
   * 预加载会话输出 — 会话页点击进入终端前的准备：订阅 + 连接。
   * 回放帧在 handler 注册前由 store 历史缓存缓冲，终端页挂载时统一回放，
   * 实现「终端准备好后才跳转」：进入终端页即渲染历史，无需二次等待。
   *
   * 返回是否已就绪；失败/超时返回 false，终端页走原有重试路径。
   *
   * @param sessionId - 会话 ID
   */
  async function prepareSession(sessionId: string): Promise<boolean> {
    try {
      // 触发连接（socket 回调异步置 subscribed）
      await store.subscribeSession(sessionId)
      // 轮询等待 subscribe_ok（连接 + 认证 + 订阅往返，通常 <1s）
      const deadline = Date.now() + PREPARE_TIMEOUT_MS
      while (Date.now() < deadline) {
        if (store.getBuffer(sessionId)?.subscribed) {
          store.markPrepared(sessionId)
          return true
        }
        await new Promise((resolve) => setTimeout(resolve, 100))
      }
      return false
    } catch (e) {
      // 订阅失败：不阻塞跳转，终端页自行重试
      console.warn(`[useTerminalBuffer] Prepare session ${sessionId} failed:`, e)
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
    forceReplay,
    prepareSession,
    handleDisconnect,
    handleSessionStopped,
    handleSessionRemoved,
    markSessionRunning,
    sendInput,
  }
}
