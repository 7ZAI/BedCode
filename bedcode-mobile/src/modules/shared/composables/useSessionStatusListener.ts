import { listen } from '@tauri-apps/api/event'
import { useSessionStore } from '@/modules/shared/stores/session'

// Re-export from model
import type { SessionStatusEvent, SessionRestartEvent } from './model'
export type { SessionStatusEvent, SessionRestartEvent }

let unlistenStatusChange: (() => void) | null = null
let unlistenRestart: (() => void) | null = null
let unlistenRefresh: (() => void) | null = null

/**
 * 监听会话状态变化事件
 * 当后端会话状态变化时自动更新前端状态
 */
export function useSessionStatusListener() {
  const sessionStore = useSessionStore()

  /**
   * 启动监听会话状态变化
   */
  async function startListening() {
    if (unlistenStatusChange) return // 已经监听中

    // 监听状态变化
    unlistenStatusChange = await listen<SessionStatusEvent>('session-status-changed', async (event) => {
      const { sessionId, oldStatus, newStatus, sessionName } = event.payload
      console.log('[SessionStatusListener] Status changed:', { sessionId, oldStatus, newStatus, sessionName })

      // 刷新会话列表以获取最新状态
      await sessionStore.loadSessions()
    })

    // 监听会话重启
    unlistenRestart = await listen<SessionRestartEvent>('session-restarted', async (event) => {
      const { oldSessionId, newSessionId, sessionName } = event.payload
      console.log('[SessionStatusListener] Session restarted:', { oldSessionId, newSessionId, sessionName })

      // 刷新会话列表
      await sessionStore.loadSessions()
    })

    // 监听移动端触发的会话刷新事件
    unlistenRefresh = await listen<{ refreshType: string; source: string }>('sessions-refresh', async (event) => {
      const { refreshType, source } = event.payload
      console.log('[SessionStatusListener] Sessions refresh event:', { refreshType, source })

      // 刷新会话列表
      await sessionStore.loadSessions()
      // 同时刷新配置列表（如果需要）
      if (refreshType === 'configs' || refreshType === 'all') {
        await sessionStore.loadConfigs()
      }
    })
  }

  /**
   * 停止监听
   */
  function stopListening() {
    if (unlistenStatusChange) {
      unlistenStatusChange()
      unlistenStatusChange = null
    }
    if (unlistenRestart) {
      unlistenRestart()
      unlistenRestart = null
    }
    if (unlistenRefresh) {
      unlistenRefresh()
      unlistenRefresh = null
    }
  }

  return {
    startListening,
    stopListening,
  }
}