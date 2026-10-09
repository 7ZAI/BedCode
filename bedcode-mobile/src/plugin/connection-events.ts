/**
 * 宿主连接生命周期事件面 → 插件（票 2026-10-09：宿主页下沉 terminal-session）
 *
 * 旧宿主 DevicesView/useMobileConnection 直接听 `ws_reconnecting` /
 * `ws_reconnected` / `ws_unexpected_disconnect` / `ws_reauth_rejected` /
 * `ws_reconnect_failed` / `ws_event_channel_ready`。宿主页迁入插件后，
 * 插件不能裸听宿主内部事件名——此处按**白名单**封装为单一回调，
 * 经 mobileApi.onConnectionEvent 注入。只投引擎事实（重连节奏 / 认证拒绝 /
 * 事件通道就绪），不含业务语义。
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { Disposable } from '@binblink/bedcode-plugin-sdk-mobile'
import { logger } from '@/utils/frontendLogger'

/** 连接生命周期事件（引擎事实面，与宿主 ws_* 事件一一对应） */
export type HostConnectionEvent =
  | { type: 'reconnecting'; retry: number; maxRetry: number }
  | { type: 'reconnected' }
  | { type: 'unexpected_disconnect' }
  | { type: 'reauth_rejected'; reason: string }
  | { type: 'reconnect_failed'; reason: string }
  | { type: 'event_channel_ready' }

/** 订阅宿主连接生命周期事件；返回 Disposable（dispose 幂等） */
export function subscribeConnectionEvents(
  handler: (event: HostConnectionEvent) => void,
): Disposable {
  const unlisteners: UnlistenFn[] = []
  let disposed = false

  const register = (name: string, map: (payload: any) => HostConnectionEvent | null): void => {
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
        logger.warn(`[ConnectionEvents] listen ${name} failed:`, e)
      })
  }

  register('ws_reconnecting', (p) => ({
    type: 'reconnecting',
    retry: p?.retry ?? 0,
    maxRetry: p?.max_retry ?? 0,
  }))
  register('ws_reconnected', () => ({ type: 'reconnected' }))
  register('ws_unexpected_disconnect', () => ({ type: 'unexpected_disconnect' }))
  register('ws_reauth_rejected', (p) => ({ type: 'reauth_rejected', reason: p?.reason ?? '' }))
  register('ws_reconnect_failed', (p) => ({ type: 'reconnect_failed', reason: p?.reason ?? '' }))
  register('ws_event_channel_ready', () => ({ type: 'event_channel_ready' }))

  return {
    dispose() {
      disposed = true
      for (const un of unlisteners) un()
      unlisteners.length = 0
    },
  }
}