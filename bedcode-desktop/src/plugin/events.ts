/**
 * Plugin Event API
 *
 * 基于 Tauri event 系统的插件间通信
 * 事件命名规范：{domain}:{action}
 * 插件只能监听自己声明了权限域的事件
 *
 * 同时桥接 Tauri `listen()` 事件到内存总线，
 * 使 Rust 侧 `app_handle.emit()` 发送的事件能到达前端 handler
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

/** 在内存总线中注册 handler */
function registerInMemory(pluginId: string, event: string, handler: EventHandler): Disposable {
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

/** 监听事件 — 同时注册内存总线和 Tauri listen */
export function on(pluginId: string, event: string, handler: EventHandler): Disposable {
  // 1. 注册内存总线（前端 → 前端事件）
  const memDisposable = registerInMemory(pluginId, event, handler)

  // 2. 同时注册 Tauri listen（桥接 Rust → 前端事件）
  // Rust 侧 app_handle.emit() 发送的事件通过 Tauri 事件系统到达前端
  let tauriUnlisten: (() => void) | null = null
  import('@tauri-apps/api/event').then(({ listen }) => {
    listen(event, (tauriEvent: any) => {
      handler(tauriEvent.payload)
    }).then(unlisten => {
      tauriUnlisten = unlisten
    })
  }).catch(() => {
    // Tauri API 不可用时静默忽略（如测试环境）
  })

  return {
    dispose() {
      memDisposable.dispose()
      tauriUnlisten?.()
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
