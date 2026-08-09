/**
 * Terminal Buffer Composable
 *
 * TerminalView 用的 composable — 管理会话订阅与实时输出写入。
 * 数据真源在服务端：历史回放（incremental 续传 / reset 全量重播）与实时推送
 * 同通道流式到达，前端只维护字节游标，不再缓存输出字节。
 */

import { useTerminalBufferStore, type SubscribeResultInfo } from '@/stores/terminalBuffer'
import { wsJoinSession, wsLeaveSession } from '@/composables/useMobileCommands'
import { createWriteCoalescer } from '@/composables/writeCoalescer'
import type { Terminal } from '@xterm/xterm'

// ==================== Types ====================

export type { OutputPayload, SubscribeResultInfo } from '@/stores/terminalBuffer'

// ==================== Write Coalescer ====================
//
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
   */
  function registerRealtimeHandler(sessionId: string, terminal: Terminal) {
    const writeCoalescer = createWriteCoalescer(terminal)
    store.registerRealtimeHandler(sessionId, {
      onOutput: (data: Uint8Array) => {
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
   * 订阅会话 — 已订阅则跳过；未订阅时以字节游标发起增量续传
   *
   * 服务端裁决 mode（替代旧版 minSeq > startSeq 客户端猜测）：
   * - incremental：游标在保留区间内，从游标字节级裁剪续传
   * - reset：游标失效（头部淘汰/流重建/首次），清屏后全量重播
   *
   * @param sessionId - 会话 ID
   * @returns 订阅裁决信息或 null（已订阅时跳过）
   */
  async function subscribeSession(sessionId: string): Promise<SubscribeResultInfo | null> {
    const buffer = store.getBuffer(sessionId)
    if (buffer?.subscribed) return null // 已订阅，跳过

    // 字节游标：上次渲染到的位置；-1（未渲染过）→ 首次全量重播
    const cursor = buffer && buffer.cursor >= 0 ? buffer.cursor : undefined

    // 先确保 buffer 存在 + 监听器启动，再订阅后端
    store.ensureBuffer(sessionId)

    const result = await wsJoinSession(sessionId, cursor)

    // 服务端裁决 reset：游标已失效，清屏后等待全量回放帧
    if (result.mode === 'reset') {
      const buf = store.getBuffer(sessionId)
      if (buf) {
        buf.cursor = -1
      }
      const handler = store.realtimeHandlers.get(sessionId)
      if (handler?.onClear) {
        handler.onClear()
      }
    }

    store.markSubscribed(sessionId)
    return result
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
   * 连接断开时 — 标记所有 buffer 未订阅
   */
  function handleDisconnect() {
    store.markAllUnsubscribed()
  }

  /**
   * 连接恢复时 — 重新订阅所有有 buffer 且未停止的会话
   */
  async function handleReconnect() {
    const sessionIds: string[] = []
    for (const [sessionId, buffer] of store.buffers.entries()) {
      if (!buffer.sessionStopped) {
        sessionIds.push(sessionId)
      }
    }

    for (const sessionId of sessionIds) {
      try {
        await subscribeSession(sessionId)
      } catch (e) {
        console.warn(`[useTerminalBuffer] Resubscribe failed for ${sessionId}:`, e)
      }
    }
  }

  /**
   * 会话停止时 — 标记 buffer + 取消后端订阅
   */
  async function handleSessionStopped(sessionId: string) {
    store.markSessionStopped(sessionId)
    store.unregisterRealtimeHandler(sessionId)
    try {
      await wsLeaveSession(sessionId)
    } catch (e) {
      console.warn('[useTerminalBuffer] Leave stopped session failed:', e)
    }
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
    handleDisconnect,
    handleReconnect,
    handleSessionStopped,
    handleSessionRemoved,
  }
}
