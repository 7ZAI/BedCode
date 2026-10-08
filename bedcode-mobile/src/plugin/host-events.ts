/**
 * 宿主事件面 → 插件（票 15：终端订阅状态联动）
 *
 * 终端 store 迁移插件后，「断线 / 会话状态 / 会话停止 / 会话移除」对终端订阅
 * 状态的联动由插件自行维护。此处把相关 Tauri 事件按**白名单**封装为单一回调
 * （经 mobileApi.onSessionEvent 注入），插件不裸听宿主内部事件名。
 */

import { watch } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { MobileHostSessionEvent, Disposable } from '@binblink/bedcode-plugin-sdk-mobile'
import { logger } from '@/utils/frontendLogger'
import { useMobileConnection } from '@/composables/useMobileConnection'

/** 订阅宿主会话/连接生命周期事件；返回 Disposable（dispose 幂等） */
export function subscribeHostSessionEvents(
  handler: (event: MobileHostSessionEvent) => void,
): Disposable {
  const unlisteners: UnlistenFn[] = []
  let disposed = false

  const register = (name: string, map: (payload: any) => MobileHostSessionEvent | null): void => {
    void listen(name, (event) => {
      const mapped = map(event.payload)
      if (mapped) handler(mapped)
    })
      .then((un) => {
        if (disposed) un()
        else unlisteners.push(un)
      })
      .catch((e) => {
        // 非 Tauri 环境（单测 / dev-shell）：联动缺失不影响主链路，记日志即可
        logger.warn(`[HostEvents] listen ${name} failed:`, e)
      })
  }

  register('ws_disconnected', () => ({ type: 'disconnected' }))
  register('ws_unexpected_disconnect', () => ({ type: 'disconnected' }))

  // 连接状态归零是「手动断开 / 切换设备前置清理 / 意外断开」的统一路径（后两者不
  // 必然带 ws_disconnected 事件）：订阅该翻转，插件据此清订阅信念（幂等双保险）
  const connection = useMobileConnection()
  const stopWatch = watch(
    () => connection.isConnected.value,
    (connected) => {
      if (!connected) handler({ type: 'disconnected' })
    },
  )
  register('ws_sync_session_status_changed', (p) => ({
    type: 'session_status',
    sessionId: p?.session_id,
    newStatus: p?.new_status,
  }))
  register('ws_sync_session_stopped', (p) => ({ type: 'session_stopped', sessionId: p?.session_id }))
  register('ws_sync_session_removed', (p) => ({ type: 'session_removed', sessionId: p?.session_id }))

  return {
    dispose() {
      disposed = true
      stopWatch()
      for (const un of unlisteners) un()
      unlisteners.length = 0
    },
  }
}
