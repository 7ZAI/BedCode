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
  I18nAPI,
  SystemAPI,
  SidebarPanelDescriptor,
  ToolboxPageDescriptor,
  StatusBarItemDescriptor,
  InputExtensionDescriptor,
  TerminalToolbarItemDescriptor,
  TitleBarItemDescriptor,
  PageToolbarItemDescriptor,
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
      // WASM 插件 invoke_command 的约定与 manifest contributes.commands 一致，
      // 使用全名（如 "auto-task.list-task-history"）；插件侧 `_ =>` 兜底按全名匹配，
      // 不能去前缀，否则落入 Unknown command（registry/命令面板/插件视图均传全名）
      try {
        return await pluginCmds.pluginInvoke(info.id, id, args.length === 1 ? args[0] : args)
      } catch (e: any) {
        // 保留底层错误信息，避免把真实失败原因（如 WASM trap、插件未激活）
        // 统一掩盖成 "Command not found"，便于定位问题
        const detail = e?.message ? ` (${e.message})` : ''
        throw new Error(`Command not found: ${id}${detail}`)
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
    registerPageToolbarItem(item: PageToolbarItemDescriptor): Disposable {
      requirePermission('ui.registerPageToolbarItem')
      const registry = getPluginRegistry()
      const disposable = registry.registerPageToolbarItem(info.id, item)
      disposables.push(disposable)
      return disposable
    },
    registerFileHandler(handler: FileHandlerDescriptor): Disposable {
      requirePermission('ui.registerFileHandler')
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
      const registry = getPluginRegistry()
      const disposable = registry.registerHttpEndpoint(info.id, path, handler)
      disposables.push(disposable)
      return disposable
    },
  }

  // ==================== SystemAPI ====================

  /** 检查 system:open 权限，失败时抛 i18n 文案错误 */
  function requireSystemOpenPermission(apiMethod: string): void {
    if (!hasPermissionForApi(permissions, apiMethod)) {
      const hostI18n = (window as any).__BEDCODE_SHARED__?.i18n
      const message = hostI18n
        ? hostI18n.global.t('desktop.plugin.noSystemOpenPermission', { plugin: info.id })
        : 'desktop.plugin.noSystemOpenPermission'
      throw new Error(message)
    }
  }

  const system: SystemAPI = {
    async revealInDir(path: string): Promise<void> {
      requireSystemOpenPermission('system.revealInDir')
      return pluginCmds.pluginRevealInDir(info.id, path)
    },
  }

  // ==================== I18nAPI ====================
  const i18n: I18nAPI = {
    getI18n(): any {
      return (window as any).__BEDCODE_SHARED__?.i18n
    },
    registerMessages(locale: string, messages: Record<string, any>): void {
      const hostI18n = (window as any).__BEDCODE_SHARED__?.i18n
      if (!hostI18n) return
      // 用插件 ID 作为命名空间前缀，避免 key 冲突
      const prefixed: Record<string, any> = {}
      for (const [key, value] of Object.entries(messages)) {
        prefixed[`${info.id}.${key}`] = value
      }
      // 直接合并新消息，vue-i18n 会自动与现有消息深度合并
      hostI18n.global.mergeLocaleMessage(locale, prefixed)
    },
    t(key: string, params?: Record<string, any>): string {
      const hostI18n = (window as any).__BEDCODE_SHARED__?.i18n
      if (!hostI18n) return key
      // 自动添加插件 ID 前缀
      const fullKey = `${info.id}.${key}`
      return hostI18n.global.t(fullKey, params)
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
    i18n,
    system,
    _disposables: disposables,
  }
}
