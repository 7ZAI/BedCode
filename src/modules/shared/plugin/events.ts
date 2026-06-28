/**
 * Plugin Event API
 *
 * 基于 Tauri event 系统的插件间通信
 * 事件命名规范：{domain}:{action}
 * 插件只能监听自己声明了权限域的事件
 */

import type { Disposable } from './types'

type EventHandler = (...args: any[]) => void

/** 全局事件总线 */
const handlers = new Map<string, Set<EventHandler>>()

/** 监听事件 */
export function on(pluginId: string, event: string, handler: EventHandler): Disposable {
  const key = `${pluginId}:${event}`
  if (!handlers.has(key)) {
    handlers.set(key, new Set())
  }
  handlers.get(key)!.add(handler)

  return {
    dispose() {
      handlers.get(key)?.delete(handler)
    },
  }
}

/** 发射事件 */
export function emit(event: string, ...args: any[]): void {
  for (const [key, handlerSet] of handlers.entries()) {
    // key 格式为 pluginId:eventName
    const eventName = key.split(':').slice(1).join(':')
    if (eventName === event) {
      handlerSet.forEach(h => {
        try {
          h(...args)
        } catch (e) {
          console.error(`[PluginEvents] Error in handler for ${event}:`, e)
        }
      })
    }
  }
}

/** 清理插件的所有事件监听 */
export function clearPluginEvents(pluginId: string): void {
  for (const key of handlers.keys()) {
    if (key.startsWith(`${pluginId}:`)) {
      handlers.delete(key)
    }
  }
}
