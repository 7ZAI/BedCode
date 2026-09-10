/**
 * Plugin Events
 *
 * 插件间事件总线 — 按 plugin_id 命名空间隔离
 */

import type { Disposable } from './types'
import { logger } from '@/utils/frontendLogger'

type EventHandler = (...args: any[]) => void

/** 插件事件映射：namespaced_event → handlers */
const handlers = new Map<string, Set<EventHandler>>()

/** 反向索引：pluginId → 该插件注册的全部 disposable（clearPluginEvents 真正 dispose 用） */
const pluginDisposables = new Map<string, Set<Disposable>>()

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
  // dispose 与 Tauri listen 建立存在竞态：dispose 先行时，listen resolve 后须立即反注册
  let disposed = false
  import('@tauri-apps/api/event')
    .then(({ listen }) => {
      listen(event, (tauriEvent: any) => {
        handler(tauriEvent.payload)
      }).then(unlisten => {
        if (disposed) {
          unlisten()
        } else {
          tauriUnlisten = unlisten
        }
      })
    })
    .catch(() => {
      // 非 Tauri 环境：仅走内存总线
    })

  const disposable: Disposable = {
    dispose() {
      if (disposed) return
      disposed = true
      set!.delete(handler)
      if (set!.size === 0) {
        handlers.delete(key)
      }
      tauriUnlisten?.()
      // 从反向索引摘除自身：单独 dispose 与 clearPluginEvents 两条路径都经此收敛
      const disposables = pluginDisposables.get(pluginId)
      if (disposables) {
        disposables.delete(disposable)
        if (disposables.size === 0) {
          pluginDisposables.delete(pluginId)
        }
      }
    },
  }
  let disposables = pluginDisposables.get(pluginId)
  if (!disposables) {
    disposables = new Set()
    pluginDisposables.set(pluginId, disposables)
  }
  disposables.add(disposable)
  return disposable
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
          logger.error(`[PluginEvents] Error in handler for ${key}:`, e)
        }
      }
    }
  }
}

/**
 * 清理插件的所有事件监听
 *
 * 逐条 dispose（含 tauriUnlisten 反注册）后再清内存 Set，消除 Tauri listener
 * 句柄泄漏；幂等——已 dispose 的条目跳过。`context._disposables` 主清理路径
 * 不变，本函数是「前端模块未加载但已有 Tauri listener」的次级防线兜底。
 */
export function clearPluginEvents(pluginId: string): void {
  const disposables = pluginDisposables.get(pluginId)
  if (disposables) {
    // 先摘除反向索引再逐条 dispose：dispose 内部会摘自身，快照防迭代中修改
    pluginDisposables.delete(pluginId)
    for (const d of [...disposables]) {
      try {
        d.dispose()
      } catch (e) {
        logger.error(`[PluginEvents] Error disposing listener for ${pluginId}:`, e)
      }
    }
  }
  // 兜底清扫：无索引登记的残留 handler（防御一致性缺口）
  const prefix = `${pluginId}::`
  for (const key of [...handlers.keys()]) {
    if (key.startsWith(prefix)) {
      handlers.delete(key)
    }
  }
}
