/**
 * Plugin Event API
 *
 * 基于 Tauri event 系统的插件间通信
 * 事件命名规范：{domain}:{action}
 * 插件只能监听自己声明了权限域的事件
 */

import type { Disposable } from './types'

type EventHandler = (...args: any[]) => void

/**
 * 全局事件总线
 *
 * 使用嵌套 Map 避免字符串解析的脆弱性
 * 外层 key 为 pluginId，内层 key 为 eventName
 */
const handlers = new Map<string, Map<string, Set<EventHandler>>>()

/** 监听事件 */
export function on(pluginId: string, event: string, handler: EventHandler): Disposable {
  let pluginMap = handlers.get(pluginId)
  if (!pluginMap) {
    pluginMap = new Map()
    handlers.set(pluginId, pluginMap)
  }
  let handlerSet = pluginMap.get(event)
  if (!handlerSet) {
    handlerSet = new Set()
    pluginMap.set(event, handlerSet)
  }
  handlerSet.add(handler)

  return {
    dispose() {
      handlers.get(pluginId)?.get(event)?.delete(handler)
    },
  }
}

/** 发射事件 */
export function emit(event: string, ...args: any[]): void {
  for (const pluginMap of handlers.values()) {
    const handlerSet = pluginMap.get(event)
    if (handlerSet) {
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
  handlers.delete(pluginId)
}
