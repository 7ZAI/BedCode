/**
 * Plugin Context
 *
 * 为每个插件创建 PluginContext 实例 — 权限检查 + API 代理
 */

import { logger as frontendLogger } from '@/utils/frontendLogger'
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
  LifecycleAPI,
  LoggerAPI,
  DialogAPI,
  NotificationAPI,
  StatusAPI,
  SystemAPI,
  ToolboxPageDescriptor,
  NavTabDescriptor,
  TerminalToolbarItemDescriptor,
  SettingsSectionDescriptor,
  PluginRouteDescriptor,
  PluginDialogOptions,
  PluginDialogHandle,
} from './types'
import { hasPermissionForApi } from './permission'
import * as pluginCmds from './commands'
import * as pluginEvents from './events'
import { getPluginRegistry } from './registry'
import { registerPluginRoute, openPluginRoute } from './routes'
import { getSharedModule } from './shared-runtime'
import { invoke } from '@tauri-apps/api/core'
// 全局弹窗控制器：必须经包说明符解析（与 App.vue 挂载的 PluginGlobalDialog.vue 同源）。
// 移动端 file: 依赖是快照拷贝，若直连源码相对路径会与组件产生两个模块实例，
// openGlobalDialog 的广播到不了组件（弹窗静默不渲染）；包说明符保证同一拷贝文件。
import { openGlobalDialog } from '@binblink/bedcode-plugin-sdk-mobile/global-dialog'

// ==================== Android 系统返回键（跨插件共享单例） ====================
// Tauri AppPlugin 的行为：只要 JS 侧存在 back-button listener，系统返回一律转发到 JS，
// 不再执行默认的 webview 后退/退出。故宿主只需注册一个原生 listener，向所有订阅者分发；
// 最后一个订阅者取消时摘除原生监听，恢复默认返回行为。
const backButtonSubscribers = new Set<(payload: { canGoBack: boolean }) => void>()
let backButtonUnregister: (() => Promise<void>) | null = null

async function ensureBackButtonListener(): Promise<void> {
  if (backButtonUnregister) return
  try {
    const { onBackButtonPress } = await import('@tauri-apps/api/app')
    const listener = await onBackButtonPress((payload) => {
      for (const fn of backButtonSubscribers) fn(payload)
    })
    backButtonUnregister = () => listener.unregister()
  } catch {
    // 非 Tauri 环境（dev-shell 浏览器 / 单元测试）：静默降级，回调永不触发
    backButtonUnregister = null
  }
}

/**
 * 提取 Tauri invoke 拒绝值的可读信息
 *
 * Rust 命令返回 Err(AppError) 时，AppError 的 Serialize 实现是纯字符串，
 * Tauri IPC 以该字符串 reject（非 Error 实例、无 .message 字段）；
 * dev-shell / 单测环境抛出的则是 Error 对象。两种形态都取到文本。
 */
function extractInvokeErrorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  if (typeof e === 'string') return e
  if (e && typeof e === 'object' && 'message' in e && typeof (e as { message: unknown }).message === 'string') {
    return (e as { message: string }).message
  }
  return ''
}

/** 创建插件的 PluginContext */
export function createPluginContext(info: PluginInfo): PluginContext {
  const disposables: Disposable[] = []
  const permissions = info.permissions
  let context: PluginContext | null = null

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
      // 本地 handler 查不到时回退到 WASM 命令桥（宿主 PluginManager.invoke_command）；
      // 保留底层错误信息，避免把真实失败原因（如 WASM trap、插件未激活）统一掩盖成 Command not found。
      // 注意：Rust AppError 经 Tauri IPC 以纯字符串 reject（无 .message），需按类型提取
      try {
        return await pluginCmds.pluginInvoke(info.id, id, args.length === 1 ? args[0] : args)
      } catch (e) {
        const detail = extractInvokeErrorMessage(e)
        throw new Error(detail ? `Command not found: ${id} (${detail})` : `Command not found: ${id}`)
      }
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
    registerRoute(route: PluginRouteDescriptor): Disposable {
      requirePermission('ui.registerRoute')
      const disposable = registerPluginRoute(info.id, route)
      disposables.push(disposable)
      return disposable
    },
    openPage(routeId: string): void {
      requirePermission('ui.openPage')
      openPluginRoute(info.id, routeId)
    },
    goBack(): void {
      requirePermission('ui.goBack')
      getSharedModule('router').back()
    },
    onBackPressed(handler: (payload: { canGoBack: boolean }) => void): Disposable {
      requirePermission('ui.onBackPressed')
      backButtonSubscribers.add(handler)
      void ensureBackButtonListener()
      const disposable = {
        dispose() {
          backButtonSubscribers.delete(handler)
          if (backButtonSubscribers.size === 0 && backButtonUnregister) {
            backButtonUnregister()
            backButtonUnregister = null
          }
        },
      }
      disposables.push(disposable)
      return disposable
    },
    showDialog(options: PluginDialogOptions): PluginDialogHandle {
      requirePermission('ui.showDialog')
      return openGlobalDialog({ ...options, pluginContext: context! })
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


  // ==================== SystemAPI ====================

  /** 检查 system:open 权限，失败时抛 i18n 文案错误 */
  function requireSystemOpenPermission(apiMethod: string): void {
    if (!hasPermissionForApi(permissions, apiMethod)) {
      const hostI18n = (window as any).__BEDCODE_SHARED__?.i18n
      const message = hostI18n
        ? hostI18n.global.t('mobile.plugin.noSystemOpenPermission', { plugin: info.id })
        : 'mobile.plugin.noSystemOpenPermission'
      throw new Error(message)
    }
  }

  const system: SystemAPI = {
    async openFile(path: string, displayName?: string): Promise<void> {
      requireSystemOpenPermission('system.openFile')
      return pluginCmds.pluginOpenFile(info.id, path, displayName ?? '')
    },
    async revealInDir(path: string): Promise<void> {
      requireSystemOpenPermission('system.revealInDir')
      return pluginCmds.pluginOpenFileLocation(info.id, path)
    },
    async revealReceivedFileLocation(fileName: string): Promise<void> {
      requireSystemOpenPermission('system.revealReceivedFileLocation')
      return pluginCmds.pluginRevealReceivedFile(info.id, fileName)
    },
    /**
     * 引导开启「所有文件访问」权限（打开公共 Download 目录所需；未授权时跳系统
     * 设置页，返回跳转前的授权状态；授权后重试 revealReceivedFileLocation 即达）
     */
    async requestAllFilesAccess(): Promise<boolean> {
      requireSystemOpenPermission('system.requestAllFilesAccess')
      return pluginCmds.pluginOpenAllFilesAccess(info.id)
    },
    /** 打开系统公共下载目录（设置页下载目录区「打开」，核对文件是否落盘；
     * 未授予「所有文件访问」时报 needs_all_files_access 前缀，前端据此引导授权） */
    async openDownloadDir(): Promise<void> {
      requireSystemOpenPermission('system.openDownloadDir')
      return pluginCmds.pluginOpenDownloadDir(info.id)
    },
  }

  // ==================== I18nAPI ====================
  const i18n: I18nAPI = {
    registerMessages(locale: string, messages: Record<string, unknown>): void {
      const hostI18n = (window as any).__BEDCODE_SHARED__?.i18n
      if (!hostI18n) return
      const prefixed: Record<string, unknown> = {}
      for (const [key, value] of Object.entries(messages)) {
        prefixed[`${info.id}.${key}`] = value
      }
      const existing = hostI18n.global.getLocaleMessage(locale)
      hostI18n.global.mergeLocaleMessage(locale, { ...existing, ...prefixed })
    },
    t(key: string, params?: Record<string, unknown>): string {
      const hostI18n = (window as any).__BEDCODE_SHARED__?.i18n
      if (!hostI18n) return key
      const fullKey = `${info.id}.${key}`
      return hostI18n.global.t(fullKey, params)
    },
  }

  // ==================== LifecycleAPI ====================
  const lifecycle: LifecycleAPI = {
    onAppStartup(handler: () => void): Disposable {
      const disposable = pluginEvents.on(info.id, 'plugin:lifecycle:appStartup', handler)
      disposables.push(disposable)
      return disposable
    },
    onAppShutdown(handler: () => void): Disposable {
      const disposable = pluginEvents.on(info.id, 'plugin:lifecycle:appShutdown', handler)
      disposables.push(disposable)
      return disposable
    },
    onAuthSuccess(handler: () => void): Disposable {
      const disposable = pluginEvents.on(info.id, 'plugin:lifecycle:authSuccess', handler)
      disposables.push(disposable)
      return disposable
    },
    onDisconnect(handler: (reason: string) => void): Disposable {
      const disposable = pluginEvents.on(info.id, 'plugin:lifecycle:disconnect', (payload: any) => handler(payload.reason))
      disposables.push(disposable)
      return disposable
    },
    onSessionCreated(handler: (sessionId: string) => void): Disposable {
      const disposable = pluginEvents.on(info.id, 'plugin:lifecycle:sessionCreated', (payload: any) => handler(payload.sessionId))
      disposables.push(disposable)
      return disposable
    },
    onSessionStopped(handler: (sessionId: string) => void): Disposable {
      const disposable = pluginEvents.on(info.id, 'plugin:lifecycle:sessionStopped', (payload: any) => handler(payload.sessionId))
      disposables.push(disposable)
      return disposable
    },
    onTerminalInput(handler: (sessionId: string, data: string) => void): Disposable {
      const disposable = pluginEvents.on(info.id, 'plugin:lifecycle:terminalInput', (payload: any) => handler(payload.sessionId, payload.data))
      disposables.push(disposable)
      return disposable
    },
    onTerminalOutput(handler: (sessionId: string, data: string) => void): Disposable {
      const disposable = pluginEvents.on(info.id, 'plugin:lifecycle:terminalOutput', (payload: any) => handler(payload.sessionId, payload.data))
      disposables.push(disposable)
      return disposable
    },
  }

  // ==================== LoggerAPI ====================
  const logger: LoggerAPI = {
    info(message: string): void { pluginCmds.pluginLog(info.id, 'info', message) },
    debug(message: string): void { pluginCmds.pluginLog(info.id, 'debug', message) },
    warn(message: string): void { pluginCmds.pluginLog(info.id, 'warn', message) },
    error(message: string): void { pluginCmds.pluginLog(info.id, 'error', message) },
  }

  // ==================== DialogAPI ====================
  const dialogs: DialogAPI = {
    showDialog(options) {
      return getSharedModule('dialogs').showDialog(options)
    },
    showConfirm(options) {
      return getSharedModule('dialogs').showConfirm(options)
    },
    showPrompt(options) {
      return getSharedModule('dialogs').showPrompt(options)
    },
    showToast(message, type = 'info') {
      getSharedModule('dialogs').showToast(message, type)
    },
  }

  // ==================== NotificationAPI ====================
  const notifications: NotificationAPI = {
    async notify(title, body) {
      // 走自定义 Kotlin 插件（TaskNotificationPlugin）：插件自主通知，不受设置页开关控制
      try {
        const check = await invoke<{ granted: boolean }>('plugin:task-notification|checkNotificationPermission')
        if (!check.granted) {
          const req = await invoke<{ granted: boolean }>('plugin:task-notification|requestNotificationPermission')
          if (!req.granted) return
        }
        await invoke('plugin:task-notification|showPluginNotification', { title, body })
      } catch (e) {
        // 注意：此处是宿主侧通知失败日志，走前端 logger 落盘；
        // 不能用下方插件 LoggerAPI 的 logger（单参、走 pluginLog 通道）
        frontendLogger.warn('[PluginContext] notify failed:', e)
      }
    },
  }

  // ==================== StatusAPI ====================
  const status: StatusAPI = {
    async reportReady() {
      return pluginCmds.pluginReportReady(info.id)
    },
    async reportError(error) {
      return pluginCmds.pluginMarkError(info.id, error)
    },
  }

  context = {
    id: info.id,
    commands,
    terminal,
    session,
    ui,
    events,
    storage,
    i18n,
    lifecycle,
    logger,
    dialogs,
    notifications,
    system,
    status,
    _disposables: disposables,
  }
  return context!
}