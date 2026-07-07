/**
 * Terminal Buffer Composable
 *
 * TerminalView 用的 composable — 从 store 读取 buffer、注册实时 handler、写入历史到 xterm
 */

import { useTerminalBufferStore, type OutputPayload } from '@/stores/terminalBuffer'
import {
  wsJoinSession,
  wsLeaveSession,
} from '@/composables/useMobileCommands'
import type { Terminal } from '@xterm/xterm'

// ==================== Types ====================

export type { OutputPayload } from '@/stores/terminalBuffer'

// ==================== Composable ====================

export function useTerminalBuffer() {
  const store = useTerminalBufferStore()

  /**
   * 写入 buffer 中的历史数据到 xterm
   *
   * @param sessionId - 会话 ID
   * @param terminal - xterm Terminal 实例
   */
  function writeBufferHistoryToTerminal(sessionId: string, terminal: Terminal) {
    const buffer = store.getBuffer(sessionId)
    if (!buffer || buffer.chunks.length === 0) return

    // 逐 chunk 写入，xterm.write() 内部异步处理但调用同步
    for (const chunk of buffer.chunks) {
      terminal.write(chunk)
    }
  }

  /**
   * 注册实时输出 handler — 新数据同时写 buffer（store 已处理）和 xterm
   *
   * @param sessionId - 会话 ID
   * @param terminal - xterm Terminal 实例
   */
  function registerRealtimeHandler(sessionId: string, terminal: Terminal) {
    store.registerRealtimeHandler(sessionId, {
      onOutput: (data: Uint8Array, _payload: OutputPayload) => {
        if (terminal) {
          terminal.write(data)
        }
      },
      onClear: () => {
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
   * 订阅会话 — 如果 buffer 已标记 subscribed 则跳过
   *
   * @param sessionId - 会话 ID
   * @returns SubscribeResult 或 null（已订阅时跳过）
   */
  async function subscribeSession(sessionId: string): Promise<{ minSeq: number; maxSeq: number; historyCount: number } | null> {
    const buffer = store.getBuffer(sessionId)
    if (buffer?.subscribed) return null // 已订阅，跳过

    // 增量同步：有 lastIndex 时从断点继续
    const startSeq = buffer && buffer.lastEndIndex >= 0 ? buffer.lastEndIndex + 1 : undefined

    // 全量回放时重置 buffer 去重游标
    if (startSeq === undefined) {
      if (buffer) {
        buffer.lastIndex = -1
        buffer.lastEndIndex = -1
      }
    }

    // 先确保 buffer 存在 + 监听器启动，再订阅后端
    store.ensureBuffer(sessionId)

    const result = await wsJoinSession(sessionId, startSeq)

    // 增量同步回退检测：后端 minSeq > startSeq，说明旧数据已被覆盖
    if (startSeq !== undefined && result && result.minSeq > startSeq) {
      console.warn(
        `[useTerminalBuffer] Incremental sync gap: minSeq=${result.minSeq} > startSeq=${startSeq}, clearing buffer for fresh replay`
      )
      // 清空 buffer 避免显示不完整的拼接内容
      const buf = store.getBuffer(sessionId)
      if (buf) {
        buf.chunks = []
        buf.totalBytes = 0
        buf.lastIndex = -1
        buf.lastEndIndex = -1
        buf.hasGap = true
      }
      // 通知已注册的 realtimeHandler 清空 xterm，避免全量回放后内容重复
      const handler = store.realtimeHandlers.get(sessionId)
      if (handler?.onClear) {
        handler.onClear()
      }
    }

    store.markSubscribed(sessionId)
    return result
  }

  /**
   * 取消订阅会话（会话停止/删除时调用）
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
   * 连接断开时 — 标记所有 buffer 未订阅 + hasGap
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
    writeBufferHistoryToTerminal,
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
