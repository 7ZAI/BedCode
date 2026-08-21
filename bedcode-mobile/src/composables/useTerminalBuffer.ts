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

// ==================== Types ====================

export type { SubscribeResultInfo } from '@/stores/terminalBuffer'

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
   */
  function registerRealtimeHandler(
    sessionId: string,
    terminal: Terminal,
    onRawOutput?: (data: Uint8Array) => void,
    /** 渲染背压门控：仅本端为会话正统渲染端时回发 ack（非正统时服务端丢弃，白红流量） */
    shouldAck?: () => boolean,
  ) {
    const writeCoalescer = createWriteCoalescer(terminal)
    // 渲染背压（spec 04-06）：写入解析完成 → 回发 ack，让服务端按本端实际
    // 消费速度推进 unacked 记账（64KB 阈值 + 250ms 空闲节流在 socket 内部）。
    // shouldAck 门控：仅本端为会话正统渲染端时发送（非正统时服务端丢弃）
    terminal.onWriteParsed(() => {
      if (shouldAck?.()) {
        store.ackRendered(sessionId)
      }
    })
    store.registerRealtimeHandler(sessionId, {
      onOutput: (data: Uint8Array) => {
        onRawOutput?.(data)
        writeCoalescer(data)
      },
      onClear: () => {
        writeCoalescer.dispose()
        if (terminal) {
          terminal.clear()
        }
      },
      onTruncated: (minSeq: number) => {
        console.warn(`[useTerminalBuffer] history truncated at min_seq=${minSeq}`)
      },
    })
  }

  /**
   * 注销实时输出 handler
   *
   * @param sessionId - 会话 ID
   */
  function unregisterRealtimeHandler(sessionId: string) {
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
