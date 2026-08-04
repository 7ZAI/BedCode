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

/** 注册事件监听 — 同时注册内存总线与 Tauri listen */
export function on(pluginId: string, event: string, handler: EventHandler): Disposable {
  const key = namespacedEvent(pluginId, event)
  let set = handlers.get(key)
  if (!set) {
    set = new Set()
    handlers.set(key, set)
  }
  set.add(handler)

  // 桥接 Rust 侧 app_handle.emit() 发送的事件（WASM 宿主 emit_event 亦走此通道）。
  // 事件名为完整名称（如 plugin:file-transfer:tasks-changed），经 Tauri 事件系统到达前端，
  // 转发给内存总线中的 handler；Tauri API 不可用时（如测试环境）静默降级。
  let tauriUnlisten: (() => void) | null = null
  import('@tauri-apps/api/event')
    .then(({ listen }) => {
      listen(event, (tauriEvent: any) => {
        handler(tauriEvent.payload)
      }).then(unlisten => {
        tauriUnlisten = unlisten
      })
    })
    .catch(() => {
      // 非 Tauri 环境：仅走内存总线
    })

  return {
    dispose() {
      set!.delete(handler)
      if (set!.size === 0) {
        handlers.delete(key)
      }
      tauriUnlisten?.()
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
