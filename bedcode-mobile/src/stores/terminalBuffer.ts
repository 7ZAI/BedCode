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
  /** 会话是否已停止 */
  sessionStopped: boolean
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

      const handler = realtimeHandlers.get(sessionId)

      // 旧版服务端（无字节偏移）：无法维护游标，仅透传（兼容路径）
      if (payload.start_offset === undefined || payload.end_offset === undefined) {
        const data = decodeBase64(payload.data_base64)
        handler?.onOutput(data, payload)
        return
      }

      // 连续性校验（防御）：服务端契约保证帧间字节连续，
      // 违反即不变量破坏 → 清屏 + 丢弃游标 + 重新订阅（服务端裁决 reset 全量重播）
      if (buffer.cursor >= 0 && payload.start_offset !== buffer.cursor) {
        console.error(
          `[terminalBuffer] continuity violation: start=${payload.start_offset}, cursor=${buffer.cursor}. Resubscribing with reset`
        )
        resubscribeWithReset(sessionId, buffer, handler)
        return
      }

      // 游标推进到帧尾（= 已渲染位置）
      buffer.cursor = payload.end_offset

      const data = decodeBase64(payload.data_base64)
      handler?.onOutput(data, payload)
    })
  }

  /** 连续性不变量破坏后的自愈：丢弃游标，重新订阅（服务端给正确答案） */
  async function resubscribeWithReset(
    sessionId: string,
    buffer: SessionBuffer,
    handler?: RealtimeHandler,
  ) {
    buffer.cursor = -1
    buffer.subscribed = false
    handler?.onClear?.()
    try {
      // 游标丢弃 → 服务端裁决 reset，清屏后全量重播
      await subscribeRemote(sessionId, undefined)
      buffer.subscribed = true
    } catch (e) {
      console.warn('[terminalBuffer] Resubscribe after violation failed:', e)
    }
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
        sessionStopped: false,
      }
      buffers.set(sessionId, buffer)
      // 有 buffer 时需要全局监听器
      startGlobalListener()
    }
    return buffer
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

  /** 标记未订阅（断连/取消订阅时） */
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
    }
  }

  /** 标记会话停止 */
  function markSessionStopped(sessionId: string) {
    const buffer = buffers.get(sessionId)
    if (buffer) {
      buffer.sessionStopped = true
    }
  }

  /** 清理单个会话订阅状态 */
  function clearBuffer(sessionId: string) {
    buffers.delete(sessionId)
    realtimeHandlers.delete(sessionId)
    // 所有 buffer 都清理后，关闭全局监听器
    if (buffers.size === 0) {
      stopGlobalListener()
    }
  }

  /** 清理所有订阅状态 */
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
