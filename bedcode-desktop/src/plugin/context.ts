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
  FileServiceAPI,
  SystemAPI,
  FileServiceMount,
  MountOptions,
  PeerFileServiceInfo,
  UploadRequestMeta,
  TransferRequestMeta,
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

/** Webview 上传策略钩子事件载荷（宿主 emit，camelCase 与 Rust 侧一致） */
interface UploadHookEventPayload {
  requestId: string
  pluginId: string
  mountPath: string
  meta: UploadRequestMeta
}

/** Webview 批量传输请求钩子事件载荷（v2，宿主 emit） */
interface TransferHookEventPayload {
  requestId: string
  pluginId: string
  mountPath: string
  meta: TransferRequestMeta
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

  // ==================== FileServiceAPI ====================

  /** 检查 fileservice 权限，失败时抛 i18n 文案错误 */
  function requireFileservicePermission(apiMethod: string): void {
    if (!hasPermissionForApi(permissions, apiMethod)) {
      const hostI18n = (window as any).__BEDCODE_SHARED__?.i18n
      const message = hostI18n
        ? hostI18n.global.t('desktop.plugin.noFileservicePermission', { plugin: info.id })
        : 'desktop.plugin.noFileservicePermission'
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

      // v2：批量传输请求钩子（onTransferRequest，与 onUploadRequest 同构）
      const transferHook = options.onTransferRequest
      let transferHookUnlisten: (() => void) | null = null
      if (transferHook) {
        try {
          const { listen } = await import('@tauri-apps/api/event')
          transferHookUnlisten = await listen<TransferHookEventPayload>(
            'filesrv:transfer_request_hook',
            async (event) => {
              const payload = event.payload
              // 宿主全局 emit，必须过滤属于当前插件 + 当前挂载点的事件
              if (payload.pluginId !== info.id || payload.mountPath !== result.mountPath) return

              try {
                const decision = await transferHook(payload.meta)
                await pluginCmds.pluginFilesrvRespondTransferRequest(
                  info.id,
                  payload.requestId,
                  decision,
                )
              } catch (err) {
                console.error(`[FileService] transfer hook error for ${info.id}:`, err)
                // fail-closed：hook 异常一律拒绝，回填失败只记 debug
                try {
                  await pluginCmds.pluginFilesrvRespondTransferRequest(info.id, payload.requestId, {
                    allow: false,
                    reason: 'hook-error',
                  })
                } catch (respondErr) {
                  console.debug('[FileService] respond after transfer-hook error failed (likely timed out):', respondErr)
                }
              }
            },
          )
        } catch (listenErr) {
          // 非 Tauri 环境（如单元测试）降级：不影响 mount 本身
          console.warn('[FileService] failed to establish transfer hook listener:', listenErr)
        }
      }
      const transferHookDisposable: Disposable = {
        dispose() {
          if (transferHookUnlisten) {
            transferHookUnlisten()
            transferHookUnlisten = null
          }
        },
      }
      disposables.push(transferHookDisposable)

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
          transferHookDisposable.dispose()
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

    async pickFiles(): Promise<string[]> {
      requireFileservicePermission('fileService.pickFiles')
      return pluginCmds.pluginPickFiles(info.id)
    },

    // ==================== v2 传输批命令（接收端应答 / 设置） ====================

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
    fileService,
    i18n,
    system,
    _disposables: disposables,
  }
}
