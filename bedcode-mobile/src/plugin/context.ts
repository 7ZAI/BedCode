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
  LifecycleAPI,
  LoggerAPI,
  DialogAPI,
  NotificationAPI,
  StatusAPI,
  FileServiceAPI,
  FileServiceMount,
  MountOptions,
  PeerFileServiceInfo,
  UploadRequestMeta,
  ToolboxPageDescriptor,
  NavTabDescriptor,
  TerminalToolbarItemDescriptor,
  SettingsSectionDescriptor,
} from './types'
import { hasPermissionForApi } from './permission'
import * as pluginCmds from './commands'
import * as pluginEvents from './events'
import { getPluginRegistry } from './registry'
import { getSharedModule } from './shared-runtime'

/** Webview 上传策略钩子事件载荷（宿主 emit，camelCase 与 Rust 侧一致） */
interface UploadHookEventPayload {
  requestId: string
  pluginId: string
  mountPath: string
  meta: UploadRequestMeta
}

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
      // 本地 handler 查不到时回退到 WASM 命令桥（宿主 PluginManager.invoke_command）；
      // 保留底层错误信息，避免把真实失败原因（如 WASM trap、插件未激活）统一掩盖成 Command not found
      try {
        return await pluginCmds.pluginInvoke(info.id, id, args.length === 1 ? args[0] : args)
      } catch (e: any) {
        const detail = e?.message ? ` (${e.message})` : ''
        throw new Error(`Command not found: ${id}${detail}`)
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

  // ==================== FileServiceAPI ====================

  /** 检查 fileservice 权限，失败时抛 i18n 文案错误 */
  function requireFileservicePermission(apiMethod: string): void {
    if (!hasPermissionForApi(permissions, apiMethod)) {
      const hostI18n = (window as any).__BEDCODE_SHARED__?.i18n
      const message = hostI18n
        ? hostI18n.global.t('mobile.plugin.noFileservicePermission', { plugin: info.id })
        : 'mobile.plugin.noFileservicePermission'
      throw new Error(message)
    }
  }

  const fileService: FileServiceAPI = {
    async mount(options: MountOptions): Promise<FileServiceMount> {
      requireFileservicePermission('fileService.mount')

      const hook = options.onUploadRequest
      // 构造线上传输选项：剥离函数，只序列化数据字段
      const wireOptions: Record<string, unknown> = {
        mountPath: options.mountPath,
        roots: options.roots,
        operations: options.operations,
      }
      const result = await pluginCmds.pluginFilesrvMount(info.id, wireOptions)

      // 若插件提供了上传策略钩子，建立 Tauri 事件监听
      let hookUnlisten: (() => void) | null = null
      if (hook) {
        try {
          const { listen } = await import('@tauri-apps/api/event')
          hookUnlisten = await listen<UploadHookEventPayload>(
            'filesrv:upload_request',
            async (event) => {
              const payload = event.payload
              // 宿主全局 emit，必须过滤属于当前插件 + 当前挂载点的事件
              if (payload.pluginId !== info.id || payload.mountPath !== result.mountPath) return

              try {
                const decision = await hook(payload.meta)
                await pluginCmds.pluginFilesrvRespondUploadRequest(
                  info.id,
                  payload.requestId,
                  decision.allow,
                  decision.reason,
                )
              } catch (err) {
                console.error(`[FileService] upload hook error for ${info.id}:`, err)
                // fail-closed：hook 异常一律拒绝，回填失败只记 debug
                try {
                  await pluginCmds.pluginFilesrvRespondUploadRequest(
                    info.id,
                    payload.requestId,
                    false,
                    'hook-error',
                  )
                } catch (respondErr) {
                  console.debug('[FileService] respond after hook-error failed (likely timed out):', respondErr)
                }
              }
            },
          )
        } catch (listenErr) {
          // 非 Tauri 环境（如单元测试）降级：不影响 mount 本身
          console.warn('[FileService] failed to establish upload hook listener:', listenErr)
        }
      }

      // 封装 unlisten 为 Disposable，随插件 deactivate 清理
      const hookDisposable: Disposable = {
        dispose() {
          if (hookUnlisten) {
            hookUnlisten()
            hookUnlisten = null
          }
        },
      }
      disposables.push(hookDisposable)

      let disposed = false
      return {
        mountPath: result.mountPath,
        async updateRoots(roots: string[]): Promise<void> {
          requireFileservicePermission('fileService.updateRoots')
          return pluginCmds.pluginFilesrvUpdateRoots(info.id, result.mountPath, roots)
        },
        async dispose(): Promise<void> {
          if (disposed) return
          disposed = true
          requireFileservicePermission('fileService.unmount')
          hookDisposable.dispose()
          return pluginCmds.pluginFilesrvDispose(info.id, result.mountPath)
        },
      }
    },

    async getPeerInfo(peerId: string): Promise<PeerFileServiceInfo | null> {
      requireFileservicePermission('fileService.getPeer')
      return pluginCmds.pluginFilesrvGetPeer(info.id, peerId)
    },

    async pickDirectory(): Promise<string | null> {
      requireFileservicePermission('fileService.pickDirectory')
      return pluginCmds.pluginPickDirectory(info.id)
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
      const { sendNotification, isPermissionGranted, requestPermission } = await import(
        '@tauri-apps/plugin-notification',
      )
      if (!(await isPermissionGranted())) {
        const granted = await requestPermission()
        if (!granted) return
      }
      sendNotification({ title, body })
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

  return {
    id: info.id,
    commands,
    terminal,
    session,
    ui,
    events,
    storage,
    fileService,
    i18n,
    lifecycle,
    logger,
    dialogs,
    notifications,
    status,
    _disposables: disposables,
  }
}
