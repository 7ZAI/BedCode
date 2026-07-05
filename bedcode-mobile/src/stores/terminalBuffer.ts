/**
 * Terminal Buffer Store
 *
 * 全局终端输出缓冲区 — 分离数据接收与渲染
 * 后台会话只维护轻量 JS buffer，不持有 xterm 实例
 * 切换到某会话时，从 buffer 一次性写入历史数据到 xterm
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
}

/** 实时输出回调 — TerminalView 注册，新数据同时写 buffer + xterm */
export interface RealtimeHandler {
  onOutput: (data: Uint8Array, payload: OutputPayload) => void
  /** buffer 被清空（增量回退/全量重播）时调用，TerminalView 应清空 xterm */
  onClear?: () => void
}

/** 单会话缓冲区 */
export interface SessionBuffer {
  /** 原始输出 chunks，每个是 Uint8Array（解码后的 base64 数据） */
  chunks: Uint8Array[]
  /** 总字节数，用于容量控制 */
  totalBytes: number
  /** 最后接收的输出索引（去重游标） */
  lastIndex: number
  /** 最后接收的 end_index（合并消息的结束索引） */
  lastEndIndex: number
  /** buffer 是否有缺口（溢出丢弃或断连期间缺失） */
  hasGap: boolean
  /** 该会话是否已向后端订阅 */
  subscribed: boolean
  /** 会话是否已停止 */
  sessionStopped: boolean
}

// ==================== Constants ====================

/** 每会话 buffer 上限（与 xterm scrollback 5000 行对齐） */
const MAX_BUFFER_BYTES = 2 * 1024 * 1024

// ==================== Store ====================

export const useTerminalBufferStore = defineStore('terminalBuffer', () => {
  // ==================== State ====================

  /** sessionId → SessionBuffer */
  const buffers = reactive(new Map<string, SessionBuffer>())

  /** sessionId → 实时回调（TerminalView 注册的） */
  const realtimeHandlers = reactive(new Map<string, RealtimeHandler>())

  /** 全局 ws_output 监听器 unlisten 函数 */
  const unlistenRef = ref<UnlistenFn | null>(null)
  /** 是否已启动全局监听 */
  let listenerStarted = false

  // ==================== Global Listener ====================

  /** 启动全局 ws_output 监听器（只启动一次） */
  async function startGlobalListener() {
    if (listenerStarted) return
    listenerStarted = true

    unlistenRef.value = await listen<OutputPayload>('ws_output', (event) => {
      const payload = event.payload
      const sessionId = payload.session_id
      const buffer = buffers.get(sessionId)

      // 没有 buffer 的会话忽略（未被任何终端访问过）
      if (!buffer) return

      // 会话已停止后不再接收
      if (buffer.sessionStopped) return

      // 索引去重
      if (payload.index !== undefined && payload.index <= buffer.lastIndex) {
        return
      }

      // 解码 base64 → Uint8Array
      const data = decodeBase64(payload.data_base64)

      // 追加到 buffer
      appendToBuffer(sessionId, data, payload.index, payload.end_index ?? payload.index)

      // 同时回调实时 handler（TerminalView 可见时）
      const handler = realtimeHandlers.get(sessionId)
      if (handler) {
        handler.onOutput(data, payload)
      }
    })
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

  /** 确保会话有 buffer，不存在则创建 */
  function ensureBuffer(sessionId: string): SessionBuffer {
    let buffer = buffers.get(sessionId)
    if (!buffer) {
      buffer = {
        chunks: [],
        totalBytes: 0,
        lastIndex: -1,
        lastEndIndex: -1,
        hasGap: false,
        subscribed: false,
        sessionStopped: false,
      }
      buffers.set(sessionId, buffer)
      // 有 buffer 时需要全局监听器
      startGlobalListener()
    }
    return buffer
  }

  /** 追加输出数据到 buffer */
  function appendToBuffer(sessionId: string, data: Uint8Array, index: number, endIndex: number) {
    const buffer = ensureBuffer(sessionId)

    buffer.chunks.push(data)
    buffer.totalBytes += data.length
    buffer.lastIndex = index
    buffer.lastEndIndex = endIndex

    // 容量溢出时丢弃最旧 chunks
    while (buffer.totalBytes > MAX_BUFFER_BYTES && buffer.chunks.length > 1) {
      const removed = buffer.chunks.shift()!
      buffer.totalBytes -= removed.length
      buffer.hasGap = true
    }
  }

  /** 获取会话 buffer */
  function getBuffer(sessionId: string): SessionBuffer | undefined {
    return buffers.get(sessionId)
  }

  /** 标记已订阅后端 */
  function markSubscribed(sessionId: string) {
    const buffer = ensureBuffer(sessionId)
    buffer.subscribed = true
  }

  /** 标记未订阅（断连时） */
  function markUnsubscribed(sessionId: string) {
    const buffer = buffers.get(sessionId)
    if (buffer) {
      buffer.subscribed = false
    }
  }

  /** 标记所有 buffer 未订阅（连接断开时） */
  function markAllUnsubscribed() {
    for (const buffer of buffers.values()) {
      buffer.subscribed = false
      buffer.hasGap = true
    }
  }

  /** 标记会话停止 */
  function markSessionStopped(sessionId: string) {
    const buffer = buffers.get(sessionId)
    if (buffer) {
      buffer.sessionStopped = true
    }
  }

  /** 清理单个会话 buffer */
  function clearBuffer(sessionId: string) {
    buffers.delete(sessionId)
    realtimeHandlers.delete(sessionId)
    // 所有 buffer 都清理后，关闭全局监听器
    if (buffers.size === 0) {
      stopGlobalListener()
    }
  }

  /** 清理所有 buffer */
  function clearAllBuffers() {
    buffers.clear()
    realtimeHandlers.clear()
    stopGlobalListener()
  }

  // ==================== Realtime Handler ====================

  /** 注册实时输出回调（TerminalView onMounted 时调用） */
  function registerRealtimeHandler(sessionId: string, handler: RealtimeHandler) {
    realtimeHandlers.set(sessionId, handler)
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
    appendToBuffer,
    markSubscribed,
    markUnsubscribed,
    markAllUnsubscribed,
    markSessionStopped,
    clearBuffer,
    clearAllBuffers,
    registerRealtimeHandler,
    unregisterRealtimeHandler,
    startGlobalListener,
  }
})
