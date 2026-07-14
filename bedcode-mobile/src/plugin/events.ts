/**
 * Plugin Events
 *
 * 插件间事件总线 — 按 plugin_id 命名空间隔离
 */

import type { Disposable } from './types'

type EventHandler = (...args: any[]) => void

/** 插件事件映射：namespaced_event → handlers */
const handlers = new Map<string, Set<EventHandler>>()

/** 构造命名空间隔离的事件名 */
function namespacedEvent(pluginId: string, event: string): string {
  return `${pluginId}::${event}`
}

/** 注册事件监听 */
export function on(pluginId: string, event: string, handler: EventHandler): Disposable {
  const key = namespacedEvent(pluginId, event)
  let set = handlers.get(key)
  if (!set) {
    set = new Set()
    handlers.set(key, set)
  }
  set.add(handler)

  return {
    dispose() {
      set!.delete(handler)
      if (set!.size === 0) {
        handlers.delete(key)
      }
    },
  }
}

/** 发射事件 */
export function emit(event: string, ...args: any[]): void {
  // 全局事件：匹配所有 pluginId 前缀
  for (const [key, set] of handlers) {
    const idx = key.indexOf('::')
    if (idx !== -1 && key.slice(idx + 2) === event) {
      for (const handler of set) {
        try {
          handler(...args)
        } catch (e) {
          console.error(`[PluginEvents] Error in handler for ${key}:`, e)
        }
      }
    }
  }
}

/** 清理插件的所有事件监听 */
export function clearPluginEvents(pluginId: string): void {
  const prefix = `${pluginId}::`
  for (const key of [...handlers.keys()]) {
    if (key.startsWith(prefix)) {
      handlers.delete(key)
    }
  }
}
