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
  SystemAPI,
  FileServiceMount,
  MountOptions,
  PeerFileServiceInfo,
  UploadRequestMeta,
  TransferRequestMeta,
  SafEntry,
  SafCopyHandle,
  SafCopyStatus,
  PickedSharedDirectory,
  ToolboxPageDescriptor,
  NavTabDescriptor,
  TerminalToolbarItemDescriptor,
  SettingsSectionDescriptor,
  PluginRouteDescriptor,
} from './types'
import { hasPermissionForApi } from './permission'
import * as pluginCmds from './commands'
import * as pluginEvents from './events'
import { getPluginRegistry } from './registry'
import { registerPluginRoute, openPluginRoute } from './routes'
import { getSharedModule } from './shared-runtime'
import { invoke } from '@tauri-apps/api/core'

/** Webview 上传策略钩子事件载荷（宿主 emit，camelCase 与 Rust 侧一致） */
interface UploadHookEventPayload {
  requestId: string
  pluginId: string
  mountPath: string
  meta: UploadRequestMeta
}

/** Webview 批量传输钩子事件载荷（v2；meta 为 TransferRequestMeta） */
interface TransferHookEventPayload {
  requestId: string
  pluginId: string
  mountPath: string
  meta: TransferRequestMeta
}

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
      const batchHook = options.onTransferRequest
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

      // v2：批量传输请求钩子（onTransferRequest）——与上传钩子同构的事件桥
      let batchHookUnlisten: (() => void) | null = null
      if (batchHook) {
        try {
          const { listen } = await import('@tauri-apps/api/event')
          batchHookUnlisten = await listen<TransferHookEventPayload>(
            'filesrv:transfer_request_hook',
            async (event) => {
              const payload = event.payload
              if (payload.pluginId !== info.id || payload.mountPath !== result.mountPath) return

              try {
                const decision = await batchHook(payload.meta)
                await pluginCmds.pluginFilesrvRespondTransferRequest(
                  info.id,
                  payload.requestId,
                  JSON.stringify(decision),
                )
              } catch (err) {
                console.error(`[FileService] transfer hook error for ${info.id}:`, err)
                // fail-closed：hook 异常一律 deny（回填失败只记 debug）
                try {
                  await pluginCmds.pluginFilesrvRespondTransferRequest(
                    info.id,
                    payload.requestId,
                    JSON.stringify({ allow: false, reason: 'hook-error' }),
                  )
                } catch (respondErr) {
                  console.debug('[FileService] respond after transfer hook-error failed (likely timed out):', respondErr)
                }
              }
            },
          )
        } catch (listenErr) {
          console.warn('[FileService] failed to establish transfer hook listener:', listenErr)
        }
      }

      // 封装 unlisten 为 Disposable，随插件 deactivate 清理
      const hookDisposable: Disposable = {
        dispose() {
          if (hookUnlisten) {
            hookUnlisten()
            hookUnlisten = null
          }
          if (batchHookUnlisten) {
            batchHookUnlisten()
            batchHookUnlisten = null
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

    // ==================== v2 批量传输批准 ====================

    async approveTransferRequest(batchId: string): Promise<void> {
      requireFileservicePermission('fileService.approveTransferRequest')
      return pluginCmds.pluginFilesrvApproveTransfer(info.id, batchId)
    },

    async rejectTransferRequest(batchId: string): Promise<void> {
      requireFileservicePermission('fileService.rejectTransferRequest')
      return pluginCmds.pluginFilesrvRejectTransfer(info.id, batchId)
    },

    async setApprovalTimeout(mountPath: string, seconds: number): Promise<void> {
      requireFileservicePermission('fileService.setApprovalTimeout')
      return pluginCmds.pluginFilesrvSetApprovalTimeout(info.id, mountPath, seconds)
    },

    async cancelReceivingSession(sessionId: string): Promise<void> {
      requireFileservicePermission('fileService.cancelReceivingSession')
      return pluginCmds.pluginFilesrvCancelReceiving(info.id, sessionId)
    },

    async getPeerInfo(peerId: string): Promise<PeerFileServiceInfo | null> {
      requireFileservicePermission('fileService.getPeer')
      return pluginCmds.pluginFilesrvGetPeer(info.id, peerId)
    },

    async pickDirectory(): Promise<string | null> {
      requireFileservicePermission('fileService.pickDirectory')
      return pluginCmds.pluginPickDirectory(info.id)
    },

    async pickFile(): Promise<string | null> {
      requireFileservicePermission('fileService.pickFile')
      return pluginCmds.pluginPickFile(info.id)
    },

    async pickSharedDirectory(): Promise<PickedSharedDirectory | null> {
      requireFileservicePermission('fileService.pickSharedDirectory')
      return pluginCmds.pluginPickSharedDirectory(info.id)
    },

    async listDir(path: string): Promise<SafEntry[]> {
      requireFileservicePermission('fileService.listDir')
      return pluginCmds.pluginSafListDir(info.id, path)
    },

    // SAF 存储访问（共享目录遍历 + 中转复制；Android 真机可用，其他平台 reject）
    saf: {
      async listTree(treeUri: string, documentId: string): Promise<SafEntry[]> {
        requireFileservicePermission('fileService.saf.listTree')
        return pluginCmds.pluginSafListTree(info.id, treeUri, documentId)
      },
      async copyStart(uri: string, destName: string): Promise<SafCopyHandle> {
        requireFileservicePermission('fileService.saf.copyStart')
        return pluginCmds.pluginSafCopyStart(info.id, uri, destName)
      },
      async copyStatus(copyId: string): Promise<SafCopyStatus> {
        requireFileservicePermission('fileService.saf.copyStatus')
        return pluginCmds.pluginSafCopyStatus(info.id, copyId)
      },
      async copyCancel(copyId: string): Promise<void> {
        requireFileservicePermission('fileService.saf.copyCancel')
        return pluginCmds.pluginSafCopyCancel(info.id, copyId)
      },
      async cleanupStaleCopies(): Promise<void> {
        requireFileservicePermission('fileService.saf.cleanupStaleCopies')
        return pluginCmds.pluginSafCleanupStaleCopies(info.id)
      },
      async checkAuthorized(treeUri: string): Promise<boolean> {
        requireFileservicePermission('fileService.saf.checkAuthorized')
        return pluginCmds.pluginSafCheckAuthorized(info.id, treeUri)
      },
    },

    /** 引导授予「所有文件访问权限」（Android 11+ 分区存储下读取顶层自定义目录必需；
     * 无运行时弹窗，跳转系统授权页）。返回当前是否已授权；非 Android 平台 reject */
    async requestAllFilesAccess(): Promise<boolean> {
      requireFileservicePermission('fileService.requestAllFilesAccess')
      return pluginCmds.pluginOpenAllFilesSettings(info.id)
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
      // 走自定义 Kotlin 插件（TaskNotificationPlugin）：插件自主通知，不受设置页开关控制
      try {
        const check = await invoke<{ granted: boolean }>('plugin:task-notification|checkNotificationPermission')
        if (!check.granted) {
          const req = await invoke<{ granted: boolean }>('plugin:task-notification|requestNotificationPermission')
          if (!req.granted) return
        }
        await invoke('plugin:task-notification|showPluginNotification', { title, body })
      } catch (e) {
        console.warn('[PluginContext] notify failed:', e)
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
    system,
    status,
    _disposables: disposables,
  }
}
