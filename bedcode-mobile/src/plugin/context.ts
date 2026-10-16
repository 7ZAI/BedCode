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
  PluginRouteDescriptor,
  PluginDialogOptions,
  PluginDialogHandle,
  ShellSurfaceContribution,
  ShellSlotContribution,
  ShellCapsuleItem,
  ShellSettingsEntry,
} from './types'
import { hasPermissionForApi } from './permission'
import * as pluginCmds from './commands'
import * as pluginEvents from './events'
import { defineComponent, h } from 'vue'
import { getPluginRegistry } from './registry'
import PluginViewHost from './components/PluginViewHost.vue'
import { registerPluginRoute, openPluginRoute } from './routes'
import { getSharedModule } from './shared-runtime'
import { getShellRegistry } from '@/shell/registry'
import { invoke } from '@tauri-apps/api/core'
// 全局弹窗控制器：必须经包说明符解析（与 App.vue 挂载的 PluginGlobalDialog.vue 同源）。
// 移动端 file: 依赖是快照拷贝，若直连源码相对路径会与组件产生两个模块实例，
// openGlobalDialog 的广播到不了组件（弹窗静默不渲染）；包说明符保证同一拷贝文件。
import { openGlobalDialog } from '@binblink/bedcode-plugin-sdk-mobile/global-dialog'
import { subscribeAppBackPressed } from './back-button'

// ==================== Android 系统返回键（跨插件共享单例） ====================
// 原生 listener 与分发顺序（应用层 → 平台层兜底）收敛在 `./back-button`：多消费者
// 共享同一条原生监听，处理器返回 true 表示已消费（详见该文件头注）。
// 这里只做「插件 API → 分发层」的接线。

/**
 * 提取 Tauri invoke 拒绝值的可读信息
 *
 * 拒绝值形态三类：
 * 1. 宿主 Rust 命令 Err(AppError)：经 ADR 0030 信封序列化，reject 值 = `{ code, request_id, params? }`
 *    （真源 `bedcode-server-base::error::AppError::Serialize`，移动宿主经 wasm-core re-export 共用；
 *    信封**永不携带**错误原文 / 堆栈，技术详情只进宿主日志）；
 * 2. dev-shell / 单测环境：Error 实例；
 * 3. 兼容纯字符串形态（整核前移动端本地 error.rs `serialize_str` 行为，已退役）。
 *
 * 信封没有 message 字段，取 `code, request_id=<id>` 作可读标识——request_id 是宿主日志
 * 关联键（同条 tracing 的 `request_id` / `error` 字段），带上它才能从客户端侧 grep 到那一次失败。
 */
function extractInvokeErrorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  if (typeof e === 'string') return e
  if (e && typeof e === 'object') {
    const obj = e as Record<string, unknown>
    if (typeof obj.message === 'string') return obj.message
    if (typeof obj.code === 'string') {
      const rid = typeof obj.request_id === 'string' ? `, request_id=${obj.request_id}` : ''
      return `${obj.code}${rid}`
    }
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
      // 保留底层错误信息，避免把真实失败原因（如 WASM trap、插件未激活、无活跃对端）
      // 统一掩盖成 Command not found。信封形态经 extractInvokeErrorMessage 取 code + request_id。
      try {
        return await pluginCmds.pluginInvoke(info.id, id, args.length === 1 ? args[0] : args)
      } catch (e) {
        // 原始拒绝值全量落盘（dev 控制台 + 攒批转发，release 也转发 error 级）：
        // 信封只有 code/request_id，细节在宿主日志，此处留形状供关联排查
        frontendLogger.error(`[PluginContext] command "${id}" failed:`, e)
        const detail = extractInvokeErrorMessage(e)
        throw new Error(detail ? `Command not found: ${id} (${detail})` : `Command not found: ${id}`)
      }
    },
  }

  // ==================== SessionAPI ====================
  const session: SessionAPI = {
    async list(): Promise<any[]> {
      requirePermission('session.list')
      // 票 04：会话列表迁桌面 HTTP（`GET /api/sessions`）；权限判定不变
      const { listSessions } = await import('@/plugin/sessionCommands')
      const result = await listSessions()
      if (result.code === 0 && result.data) {
        return result.data.sessions || []
      }
      throw new Error(result.message || 'Failed to load sessions')
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
  //
  // 票 2026-10-10 批次 C2：`registerToolboxPage` / `registerNavTab` /
  // `registerTerminalToolbarItem` / `registerTerminalView` 四个旧嵌入扩展点整面退役
  // （宿主壳只认 registerSurface 一种运行面形态）。退役清单的防回接锁见
  // src/__tests__/shell/retiredHostUIRetirementLocks.test.ts（R4）。
  const ui: UIRegistry = {
    registerSurface(surface: ShellSurfaceContribution): Disposable {
      // 宿主壳运行面：应用自持 UI 的挂载位，无特权动作——不做 requirePermission
      // 快速失败（真源仲裁仍在 Rust host 原语层；本面不触发任何宿主能力）。
      // appId 一律 = 插件 id（壳内应用模型与该插件同 id）。
      //
      // 壳直渲染运行面（不经旧插件 UI 宿主路径），但插件组件靠 inject('pluginContext')
      // 取上下文——在此套 PluginViewHost 包装补 provide；与 pluginAppSource.resolveSurface
      // 的旧路径同款（插件形态细节收在插件系统内，壳不认识插件）。
      const wrapped = defineComponent({
        name: `ShellSurface-${info.id}`,
        // 壳透传 { app } prop：声明吞掉，避免作为未知 attr 落到根节点
        props: { app: { type: Object, default: null } },
        setup() {
          return () => h(PluginViewHost, { pluginId: info.id, component: surface.component })
        },
      })
      const disposable = getShellRegistry().registerSurface(info.id, {
        ...surface,
        component: wrapped,
      })
      disposables.push(disposable)
      return disposable
    },
    registerSlot(slot: ShellSlotContribution): Disposable {
      const disposable = getShellRegistry().registerSlot(info.id, slot)
      disposables.push(disposable)
      return disposable
    },
    registerCapsuleItem(item: ShellCapsuleItem): Disposable {
      const disposable = getShellRegistry().registerCapsuleItem(info.id, item)
      disposables.push(disposable)
      return disposable
    },
    registerSettingsEntry(entry: ShellSettingsEntry): Disposable {
      const disposable = getShellRegistry().registerSettingsEntry(info.id, entry)
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
    onBackPressed(handler: (payload: { canGoBack: boolean }) => boolean | void): Disposable {
      requirePermission('ui.onBackPressed')
      const unsubscribe = subscribeAppBackPressed(handler)
      const disposable = { dispose: unsubscribe }
      disposables.push(disposable)
      return disposable
    },
    showDialog(options: PluginDialogOptions): PluginDialogHandle {
      requirePermission('ui.showDialog')
      return openGlobalDialog({ ...options, pluginContext: context! })
    },
  }

  // 已退役扩展点的显性抛错桩（§5.1.3 fail-visible 形态①：旧读路径不得静默降级）
  //
  // 为什么保留同名桩而不是直接删干净：旧插件调用时若拿到 undefined，
  // 崩点会落在插件自己的调用处，报错只说「不是函数」，开发者会误判成自己写错。
  // 这里当场抛出并指名扩展点 + 迁移出路，让断链可见、可定位。
  // 桩不进 UIRegistry 类型——类型面必须让旧调用点**编译期**就红。
  const retiredUi = (api: string): never => {
    throw new Error(
      `[plugin] ui.${api} 已于票 2026-10-10 整面退役：宿主壳只认 ui.registerSurface 一种运行面形态。` +
        `请改用 registerSurface（应用自持整个运行面，含底部导航与页签），应用内子页用 registerRoute + openPage。`,
    )
  }
  Object.assign(ui, {
    registerToolboxPage: () => retiredUi('registerToolboxPage'),
    registerNavTab: () => retiredUi('registerNavTab'),
    registerTerminalToolbarItem: () => retiredUi('registerTerminalToolbarItem'),
    registerTerminalView: () => retiredUi('registerTerminalView'),
  })

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
  /**
   * 把插件的消息树展平为「完整点分 key → 叶子值」的一层表
   *
   * 为什么必须展平（2026-10-14 实测修正）：宿主取文案走 `t('<插件id>.<域>.<键>')`，
   * vue-i18n 按点号逐段寻路径。插件若把消息写成嵌套对象（如 `{ app: { title: '远程终端' } }`），
   * 前缀后得到的是**键名带点的对象**（`'com.x.app' → {title}`）——路径寻址在第二段就断了，
   * 全部文案回退成 key 字面量（实测 bedcode 应用整个运行面显示 `com.bedcode.terminal-session.app.title`）。
   * 展平后每个键都是叶子，路径直达；数组与标量原样保留（vue-i18n 的列表/复数消息不能拆）。
   */
  function flattenMessages(
    source: Record<string, unknown>,
    prefix: string,
    out: Record<string, unknown>,
  ): void {
    for (const [key, value] of Object.entries(source)) {
      const fullKey = prefix ? `${prefix}.${key}` : key
      if (value && typeof value === 'object' && !Array.isArray(value)) {
        flattenMessages(value as Record<string, unknown>, fullKey, out)
      } else {
        out[fullKey] = value
      }
    }
  }

  const i18n: I18nAPI = {
    registerMessages(locale: string, messages: Record<string, unknown>): void {
      const hostI18n = (window as any).__BEDCODE_SHARED__?.i18n
      if (!hostI18n) return
      const prefixed: Record<string, unknown> = {}
      flattenMessages(messages, info.id, prefixed)
      // 直接合并新消息，vue-i18n 会自动与现有消息深度合并
      hostI18n.global.mergeLocaleMessage(locale, prefixed)
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