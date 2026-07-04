/**
 * Plugin Context
 *
 * 为每个插件创建 PluginContext 实例 — 插件访问宿主能力的唯一通道
 */

import type {
  PluginContext,
  PluginInfo,
  Disposable,
  CommandRegistry,
  TerminalAPI,
  SessionAPI,
  UIRegistry,
  EventAPI,
  StorageAPI,
  HttpAPI,
  SidebarPanelDescriptor,
  ToolboxPageDescriptor,
  StatusBarItemDescriptor,
  InputExtensionDescriptor,
  TerminalToolbarItemDescriptor,
  TitleBarItemDescriptor,
  FileHandlerDescriptor,
} from './types'
import { hasPermissionForApi } from './permission'
import * as pluginCmds from './commands'
import * as pluginEvents from './events'
import { getPluginRegistry } from './registry'

/** 创建插件的 PluginContext */
export function createPluginContext(info: PluginInfo): PluginContext {
  const disposables: Disposable[] = []
  const permissions = info.permissions

  /** 快速失败：检查权限 */
  function requirePermission(apiMethod: string): void {
    if (!hasPermissionForApi(permissions, apiMethod)) {
      throw new Error(`Plugin ${info.id} lacks permission for ${apiMethod}`)
    }
  }

  // ==================== CommandRegistry ====================
  const commandHandlers = new Map<string, (...args: any[]) => any>()

  const commands: CommandRegistry = {
    register(id: string, handler: (...args: any[]) => any): Disposable {
      commandHandlers.set(id, handler)
      const disposable = {
        dispose() {
          commandHandlers.delete(id)
        },
      }
      disposables.push(disposable)
      return disposable
    },
    async execute(id: string, ...args: any[]): Promise<any> {
      // 先查找前端注册的本地命令
      const handler = commandHandlers.get(id)
      if (handler) {
        return handler(...args)
      }
      // 尝试调用 Rust 插件的 command（通过 plugin_invoke 路由）
      try {
        return await pluginCmds.pluginInvoke(info.id, id, args.length === 1 ? args[0] : args)
      } catch {
        throw new Error(`Command not found: ${id}`)
      }
    },
  }

  // ==================== TerminalAPI ====================
  const terminal: TerminalAPI = {
    async sendInput(sessionId: string, text: string): Promise<void> {
      requirePermission('terminal.sendInput')
      return pluginCmds.pluginTerminalSendInput(info.id, sessionId, text)
    },
    onOutput(handler: (sessionId: string, data: string) => void): Disposable {
      requirePermission('terminal.onOutput')
      const disposable = pluginEvents.on(info.id, 'terminal:output', handler as any)
      disposables.push(disposable)
      return disposable
    },
    onInput(handler: (sessionId: string, text: string) => string | null): Disposable {
      requirePermission('terminal.onInput')
      const disposable = pluginEvents.on(info.id, 'terminal:input', handler as any)
      disposables.push(disposable)
      return disposable
    },
  }

  // ==================== SessionAPI ====================
  const session: SessionAPI = {
    async list(): Promise<any[]> {
      requirePermission('session.list')
      const { listSessions } = await import('@/composables/useDesktopCommands')
      return listSessions()
    },
    async get(sessionId: string): Promise<any> {
      requirePermission('session.get')
      const { getSession } = await import('@/composables/useDesktopCommands')
      return getSession(sessionId)
    },
    onStatusChange(handler: (event: any) => void): Disposable {
      requirePermission('session.onStatusChange')
      const disposable = pluginEvents.on(info.id, 'session:statusChange', handler)
      disposables.push(disposable)
      return disposable
    },
  }

  // ==================== UIRegistry ====================
  const ui: UIRegistry = {
    registerSidebarPanel(panel: SidebarPanelDescriptor): Disposable {
      requirePermission('ui.registerSidebarPanel')
      const registry = getPluginRegistry()
      const disposable = registry.registerView(info.id, 'sidebar', panel)
      disposables.push(disposable)
      return disposable
    },
    registerToolboxPage(page: ToolboxPageDescriptor): Disposable {
      requirePermission('ui.registerToolboxPage')
      const registry = getPluginRegistry()
      const disposable = registry.registerView(info.id, 'toolbox', page)
      disposables.push(disposable)
      return disposable
    },
    registerStatusBarItem(item: StatusBarItemDescriptor): Disposable {
      requirePermission('ui.registerStatusBarItem')
      const registry = getPluginRegistry()
      const disposable = registry.registerStatusBarItem(info.id, item)
      disposables.push(disposable)
      return disposable
    },
    registerInputExtension(ext: InputExtensionDescriptor): Disposable {
      requirePermission('ui.registerInputExtension')
      const registry = getPluginRegistry()
      const disposable = registry.registerInputExtension(info.id, ext)
      disposables.push(disposable)
      return disposable
    },
    registerTerminalToolbarItem(item: TerminalToolbarItemDescriptor): Disposable {
      requirePermission('ui.registerTerminalToolbarItem')
      const registry = getPluginRegistry()
      const disposable = registry.registerTerminalToolbarItem(info.id, item)
      disposables.push(disposable)
      return disposable
    },
    registerTitleBarItem(item: TitleBarItemDescriptor): Disposable {
      requirePermission('ui.registerTitleBarItem')
      const registry = getPluginRegistry()
      const disposable = registry.registerTitleBarItem(info.id, item)
      disposables.push(disposable)
      return disposable
    },
    registerFileHandler(handler: FileHandlerDescriptor): Disposable {
      requirePermission('ui.registerSidebarPanel')
      const registry = getPluginRegistry()
      const disposable = registry.registerFileHandler(info.id, handler)
      disposables.push(disposable)
      return disposable
    },
  }

  // ==================== EventAPI ====================
  const events: EventAPI = {
    on(event: string, handler: (...args: any[]) => void): Disposable {
      const disposable = pluginEvents.on(info.id, event, handler)
      disposables.push(disposable)
      return disposable
    },
    emit(event: string, ...args: any[]): void {
      pluginEvents.emit(event, ...args)
    },
  }

  // ==================== StorageAPI ====================
  const storage: StorageAPI = {
    async get<T = any>(key: string): Promise<T | undefined> {
      const val = await pluginCmds.pluginStorageGet(info.id, key)
      return val as T | undefined
    },
    async set(key: string, value: any): Promise<void> {
      return pluginCmds.pluginStorageSet(info.id, key, value)
    },
    async delete(key: string): Promise<void> {
      return pluginCmds.pluginStorageDelete(info.id, key)
    },
    async flush(): Promise<void> {
      // 存储是即时写入的，flush 为 no-op
    },
  }

  // ==================== HttpAPI ====================
  const http: HttpAPI = {
    registerEndpoint(path: string, handler): Disposable {
      requirePermission('http.registerEndpoint')
      const disposable: Disposable = {
        dispose() {
          // 后续实现：通知 Rust 端移除端点
        },
      }
      disposables.push(disposable)
      return disposable
    },
  }

  return {
    id: info.id,
    extensionPath: info.extensionPath,
    commands,
    terminal,
    session,
    ui,
    events,
    storage,
    http,
    _disposables: disposables,
  }
}
