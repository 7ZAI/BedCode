/**
 * Plugin Context
 *
 * 为每个插件创建 PluginContext 实例 — 权限检查 + API 代理
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
  I18nAPI,
  ToolboxPageDescriptor,
  NavTabDescriptor,
  TerminalToolbarItemDescriptor,
  SettingsSectionDescriptor,
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
        dispose() { commandHandlers.delete(id) },
      }
      disposables.push(disposable)
      return disposable
    },
    async execute(id: string, ...args: any[]): Promise<any> {
      const handler = commandHandlers.get(id)
      if (handler) return handler(...args)
      throw new Error(`Command not found: ${id}`)
    },
  }

  // ==================== TerminalAPI ====================
  const terminal: TerminalAPI = {
    async sendInput(sessionId: string, text: string): Promise<void> {
      requirePermission('terminal.sendInput')
      const { wsSendInput } = await import('@/composables/useMobileCommands')
      return wsSendInput(sessionId, text)
    },
    onOutput(handler: (sessionId: string, data: string) => void): Disposable {
      requirePermission('terminal.onOutput')
      const disposable = pluginEvents.on(info.id, 'terminal:output', handler)
      disposables.push(disposable)
      return disposable
    },
  }

  // ==================== SessionAPI ====================
  const session: SessionAPI = {
    async list(): Promise<any[]> {
      requirePermission('session.list')
      const { wsLoadSessions } = await import('@/composables/useMobileCommands')
      return wsLoadSessions()
    },
    async get(sessionId: string): Promise<any> {
      requirePermission('session.get')
      const sessions = await session.list()
      return sessions.find((s: any) => s.id === sessionId)
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
    registerToolboxPage(page: ToolboxPageDescriptor): Disposable {
      requirePermission('ui.registerToolboxPage')
      const registry = getPluginRegistry()
      const disposable = registry.registerToolboxPage(info.id, page)
      disposables.push(disposable)
      return disposable
    },
    registerNavTab(tab: NavTabDescriptor): Disposable {
      requirePermission('ui.registerNavTab')
      const registry = getPluginRegistry()
      const disposable = registry.registerNavTab(info.id, tab)
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
    registerSettingsSection(section: SettingsSectionDescriptor): Disposable {
      requirePermission('ui.registerSettingsSection')
      const registry = getPluginRegistry()
      const disposable = registry.registerSettingsSection(info.id, section)
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
  }

  // ==================== I18nAPI ====================
  const i18n: I18nAPI = {
    registerMessages(locale: string, messages: Record<string, any>): void {
      const hostI18n = (window as any).__BEDCODE_SHARED__?.i18n
      if (!hostI18n) return
      const prefixed: Record<string, any> = {}
      for (const [key, value] of Object.entries(messages)) {
        prefixed[`${info.id}.${key}`] = value
      }
      const existing = hostI18n.global.getLocaleMessage(locale)
      hostI18n.global.mergeLocaleMessage(locale, { ...existing, ...prefixed })
    },
    t(key: string, params?: Record<string, any>): string {
      const hostI18n = (window as any).__BEDCODE_SHARED__?.i18n
      if (!hostI18n) return key
      const fullKey = `${info.id}.${key}`
      return hostI18n.global.t(fullKey, params)
    },
  }

  return {
    id: info.id,
    commands,
    terminal,
    session,
    ui,
    events,
    storage,
    i18n,
    _disposables: disposables,
  }
}
