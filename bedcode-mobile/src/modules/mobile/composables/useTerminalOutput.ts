/**
 * Terminal Output Dispatcher
 *
 * 全局单一 ws_output 监听器 + sessionId → callback 分发
 * 替代每个 TerminalView 实例各自 listen('ws_output') 的方式
 * N 个终端实例只产生 1 个 Tauri IPC 监听器，减少 N-1 倍无效回调
 */

import { listen } from '@tauri-apps/api/event'

/** ws_output 事件载荷 */
export interface OutputPayload {
  session_id: string
  data_base64: string
  index: number
  end_index?: number
  is_waiting: boolean
}

/** 注册回调 */
export interface OutputHandler {
  onOutput: (payload: OutputPayload) => void
}

// ==================== Global State ====================

/** sessionId → handler 映射 */
const handlerMap = new Map<string, OutputHandler>()

/** 全局监听器 unlisten 函数 */
let unlistenRef: (() => void) | null = null

/** 是否已启动全局监听 */
let started = false

// ==================== Global Listener ====================

/** 启动全局 ws_output 监听器（只启动一次） */
async function ensureGlobalListener() {
  if (started) return
  started = true

  unlistenRef = await listen<OutputPayload>('ws_output', (event) => {
    const handler = handlerMap.get(event.payload.session_id)
    if (handler) {
      handler.onOutput(event.payload)
    }
  })
}

/** 停止全局监听器 */
function stopGlobalListener() {
  if (unlistenRef) {
    unlistenRef()
    unlistenRef = null
  }
  started = false
}

// ==================== Composable ====================

/**
 * 终端输出分发 composable
 *
 * 全局单一 ws_output 监听器，按 sessionId 分发到对应终端
 * TerminalView 在 onMounted/onActivated 时注册，onUnmounted 时注销
 */
export function useTerminalOutput() {
  /**
   * 注册终端输出处理器
   *
   * @param sessionId - 会话 ID
   * @param handler - 输出回调
   */
  async function registerHandler(sessionId: string, handler: OutputHandler) {
    await ensureGlobalListener()
    handlerMap.set(sessionId, handler)
  }

  /**
   * 注销终端输出处理器
   *
   * @param sessionId - 会话 ID
   */
  function unregisterHandler(sessionId: string) {
    handlerMap.delete(sessionId)
    // 所有处理器都注销后，关闭全局监听器释放资源
    if (handlerMap.size === 0) {
      stopGlobalListener()
    }
  }

  return {
    registerHandler,
    unregisterHandler,
  }
}
