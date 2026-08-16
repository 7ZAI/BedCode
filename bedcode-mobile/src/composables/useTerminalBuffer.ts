/**
 * Terminal Buffer Composable
 *
 * TerminalView 用的 composable — 管理会话订阅与实时输出写入。
 * 数据真源在服务端：历史回放（incremental 续传 / reset 全量重播）与实时推送
 * 同通道流式到达，前端只维护字节游标，不再缓存输出字节。
 */

import { useTerminalBufferStore, type SubscribeResultInfo } from '@/stores/terminalBuffer'
import { wsLeaveSession } from '@/composables/useMobileCommands'
import { createWriteCoalescer } from '@/composables/writeCoalescer'
import type { Terminal } from '@xterm/xterm'

/** 会话页预加载的超时上限（毫秒）：超时不再等待，直接跳转由终端页自行重试 */
const PREPARE_TIMEOUT_MS = 8000

// ==================== Types ====================

export type { OutputPayload, SubscribeResultInfo } from '@/stores/terminalBuffer'

// ==================== Write Coalescer ====================
// 为什么需要 rAF 合并写入：
// - TUI 应用（opencode、Claude Code、vim、htop 等）在一次屏幕刷新内会发出大量
//   cursor 定位 + 字符写入的连续转义序列，每个 WS 消息触发一次 terminal.write()
//   都会让 xterm 调度一次 render。
// - xterm.js WebGL 渲染器使用双缓冲，多个异步 render 在同一帧内排队时
//   会出现「前一帧部分内容 + 当前帧新内容」同时可见（鬼影/重影）。
// - 参考 xterm.js 官方推荐：DEC Mode 2026 (Synchronized Output) 是在一次刷新内
//   收集多次修改、只渲染一次的协议机制。但 PTY 应用不一定发出 BSU/ESU 序列。
// - 在前端按 rAF 合并多次 terminal.write() 等价于应用了同步输出语义：
//   同一帧内所有写入只产生一次 render commit，避免双缓冲竞态。
// 实现见 @/composables/writeCoalescer

// ==================== Composable ====================

export function useTerminalBuffer() {
  const store = useTerminalBufferStore()

  /**
   * 注册实时输出 handler — 服务端回放（历史）与实时推送同通道到达，
   * 统一经 rAF 合并写入 xterm
   *
   * @param sessionId - 会话 ID
   * @param terminal - xterm Terminal 实例
   * @param wrapSyncOutput - 是否用 DEC 2026 包裹每次写入（仅 WebGL 渲染器需要；
   *   DOM 渲染器默认关闭，避免与 TUI 应用自身 2026 序列嵌套导致闪烁）
   * @param onRawOutput - 原始输出字节钩子（合并前、写入前调用，供 TUI 兼容嗅探）
   */
  function registerRealtimeHandler(
    sessionId: string,
    terminal: Terminal,
    wrapSyncOutput = false,
    onRawOutput?: (data: Uint8Array) => void,
  ) {
    const writeCoalescer = createWriteCoalescer(terminal, { wrapSyncOutput })
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
   * 订阅会话 — 已订阅/在途则跳过；逻辑收敛到 store（防重 + 缓冲帧排空），
   * 所有订阅路径（页面进入 / 重连恢复 / 自愈）统一入口
   *
   * 服务端裁决 mode（替代旧版 minSeq > startSeq 客户端猜测）：
   * - incremental：游标在保留区间内，从游标字节级裁剪续传
   * - reset：游标失效（头部淘汰/流重建/首次），清屏后全量重播
   *
   * @param sessionId - 会话 ID
   * @returns 订阅裁决信息；已订阅/在途/失败时返回 null
   */
  async function subscribeSession(sessionId: string): Promise<SubscribeResultInfo | null> {
    return store.subscribeSession(sessionId)
  }

  /**
   * 取消订阅会话（页面卸载/会话停止/删除时调用）
   *
   * @param sessionId - 会话 ID
   */
  async function unsubscribeSession(sessionId: string) {
    store.unregisterRealtimeHandler(sessionId)
    store.markUnsubscribed(sessionId)
    try {
      await wsLeaveSession(sessionId)
    } catch (e) {
      console.warn('[useTerminalBuffer] Leave session failed:', e)
    }
  }

  /**
   * 强制全量重播 — 页面重进时 xterm 为全新实例，旧游标续传会丢失历史。
   * 重置游标与订阅状态，下次订阅服务端裁决 reset 全量重播
   *
   * @param sessionId - 会话 ID
   */
  function forceReplay(sessionId: string) {
    store.forceReplay(sessionId)
  }

  /**
   * 预加载会话输出 — 会话页点击进入终端前的准备：强制全量重播 + 订阅。
   * 回放帧在 handler 注册前由 store 缓冲，终端页挂载时统一写入，
   * 实现「终端准备好后才跳转」：进入终端页即渲染历史，无需二次等待。
   *
   * 返回是否已就绪；失败/超时返回 false，终端页走原有 forceReplay + 重试路径。
   *
   * @param sessionId - 会话 ID
   */
  async function prepareSession(sessionId: string): Promise<boolean> {
    forceReplay(sessionId)
    let timer: ReturnType<typeof setTimeout> | undefined
    try {
      await Promise.race([
        subscribeSession(sessionId),
        new Promise<never>((_, reject) => {
          timer = setTimeout(() => reject(new Error('prepare timeout')), PREPARE_TIMEOUT_MS)
        }),
      ])
    } catch (e) {
      // 超时/订阅失败：不阻塞跳转，终端页自行重试
      console.warn(`[useTerminalBuffer] Prepare session ${sessionId} failed:`, e)
      return false
    } finally {
      if (timer) clearTimeout(timer)
    }
    const ready = !!store.getBuffer(sessionId)?.subscribed
    if (ready) store.markPrepared(sessionId)
    return ready
  }

  /**
   * 连接断开时 — 标记所有 buffer 未订阅
   */
  function handleDisconnect() {
    store.markAllUnsubscribed()
  }


  /**
   * 会话停止时 — 标记 buffer + 取消订阅状态 + 取消后端订阅。
   *
   * 注意：不注销实时 handler——终端页面可能仍存活，会话重启后输出链路
   * 依赖该 handler 渲染（注销后无任何路径重新注册，页面将永久冻结）。
   * handler 生命周期归视图（挂载注册 / 卸载注销），与会话状态无关
   */
  async function handleSessionStopped(sessionId: string) {
    store.markSessionStopped(sessionId)
    // 停止即取消订阅：清 subscribed 与缓冲帧，与后端状态保持一致
    store.markUnsubscribed(sessionId)
    try {
      await wsLeaveSession(sessionId)
    } catch (e) {
      console.warn('[useTerminalBuffer] Leave stopped session failed:', e)
    }
  }

  /**
   * 会话恢复运行时 — 复位 sessionStopped（同 id 重启场景：旧流已终止，
   * 不复位则 ws_output 监听器永久丢弃新流帧，终端冻结在旧历史）
   */
  function markSessionRunning(sessionId: string) {
    store.markSessionRunning(sessionId)
  }

  /**
   * 会话删除时 — 清理 buffer + 取消后端订阅
   */
  async function handleSessionRemoved(sessionId: string) {
    store.unregisterRealtimeHandler(sessionId)
    try {
      await wsLeaveSession(sessionId)
    } catch (e) {
      console.warn('[useTerminalBuffer] Leave removed session failed:', e)
    }
    store.clearBuffer(sessionId)
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
  }
}
