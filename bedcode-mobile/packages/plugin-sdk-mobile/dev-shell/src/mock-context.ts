/**
 * Mock PluginContext
 *
 * 与宿主 context.ts 同接口、同语义（事件名 / i18n 前缀 / storage 命名空间 / UI 注册），
 * 但全部后端通道替换为浏览器实现：
 * - commands.execute：仅执行前端注册 handler；WASM 后端不在浏览器运行，未注册命令记日志
 * - storage：localStorage 持久化
 * - session/lifecycle：接 mock/session.ts 的模拟会话
 *   （terminal：TerminalAPI 已随票 15 阶段 B 整面退役，本 mock 同批移除）
 * - 权限检查跳过（dev-shell 视为全部授权，README 已说明与真机的差异）
 *
 * UI 注册面与宿主同构：贡献项写进**壳注册表**（不是 dev-shell 自留副本），
 * 运行面经 PluginContextHost 包装补 provide('pluginContext')，动态路由挂 vue-router。
 */
import { defineComponent, h } from 'vue'
import type {
  DialogOptions,
  Disposable,
  EventAPI,
  I18nAPI,
  LifecycleAPI,
  LoggerAPI,
  NotificationAPI,
  PluginContext,
  PluginDialogHandle,
  PluginDialogOptions,
  StatusAPI,
  UIRegistry,
} from '../../src/types'
import { openGlobalDialog } from '../../src/global-dialog'
import {
  emitDevEvent,
  onDevEvent,
  sessions,
} from './mock/session'
import { dialogService } from './mock/dialog-service'
import PluginContextHost from './shell/components/PluginContextHost.vue'
import { getShellRegistry } from './shell/registry'
import { getSharedModule } from './shared-runtime'
import {
  findRoute,
  getPluginRecord,
  pluginRouteName,
  pushLog,
  registerRouteEntry,
  type RouteEntry,
} from './registry'

/** 存储命名空间（与宿主插件 storage 的 per-plugin 隔离一致） */
function storageKey(pluginId: string, key: string): string {
  return `bedcode-dev-shell:${pluginId}:${key}`
}

/** 创建插件的 PluginContext */
export function createMockContext(pluginId: string): PluginContext {
  const disposables: Disposable[] = []
  // 供 ui.showDialog 闭包惰性引用（与桌面端 dev-shell 同模式，调用时 context 已初始化）
  let contextRef: PluginContext | null = null

  /** 收集 disposable，随插件 deactivate 统一清理 */
  function track(disposable: Disposable): Disposable {
    disposables.push(disposable)
    return disposable
  }

  // ==================== CommandRegistry ====================
  const commandHandlers = new Map<string, (...args: any[]) => any>()

  const commands = {
    register(id: string, handler: (...args: any[]) => any): Disposable {
      commandHandlers.set(id, handler)
      return track({
        dispose() {
          commandHandlers.delete(id)
        },
      })
    },
    async execute(id: string, ...args: any[]): Promise<any> {
      const handler = commandHandlers.get(id)
      if (handler) return handler(...args)
      pushLog(
        'warn',
        pluginId,
        `command "${id}" 未注册前端 handler——WASM 后端不在浏览器运行，请注册前端 handler 或到真机验证`,
      )
      return undefined
    },
  }

  // ==================== SessionAPI ====================
  const session = {
    async list(): Promise<any[]> {
      return sessions.value.map((s) => ({ ...s }))
    },
    async get(sessionId: string): Promise<any> {
      const s = sessions.value.find((x) => x.id === sessionId)
      return s ? { ...s } : null
    },
    onStatusChange(handler: (event: any) => void): Disposable {
      return track(onDevEvent('session:statusChange', handler))
    },
  }

  // ==================== UIRegistry ====================
  //
  // 与宿主 context.ts 同款：贡献项一律以 appId = 插件 id 写进壳注册表；运行面额外套
  // 一层 PluginContextHost 补 provide('pluginContext')——壳直渲染运行面，不认识插件，
  // 因此插件形态的细节（上下文注入）必须收在插件侧这层包装里。
  const shellRegistry = getShellRegistry()

  const ui: UIRegistry = {
    registerSurface(surface) {
      const wrapped = defineComponent({
        name: `DevShellSurface-${pluginId}`,
        // 壳透传 { app } prop：声明吞掉，避免作为未知 attr 落到根节点
        props: { app: { type: Object, default: null } },
        setup() {
          return () => h(PluginContextHost, { pluginId, component: surface.component })
        },
      })
      return track(shellRegistry.registerSurface(pluginId, { ...surface, component: wrapped }))
    },
    registerSlot(slot) {
      return track(shellRegistry.registerSlot(pluginId, slot))
    },
    registerCapsuleItem(item) {
      return track(shellRegistry.registerCapsuleItem(pluginId, item))
    },
    registerSettingsEntry(entry) {
      return track(shellRegistry.registerSettingsEntry(pluginId, entry))
    },
    registerRoute(route) {
      // 与宿主 routes.ts 同款：vue-router addRoute 承载整页跳转，注册表只做解析真源
      const router = getSharedModule('router')
      const header = route.header ?? true
      const removeRoute = router.addRoute({
        path: `/plugin/${pluginId}/${route.id}`,
        name: pluginRouteName(pluginId, route.id),
        meta: {
          pluginRoute: { pluginId, routeId: route.id, title: route.title, header },
        },
        component: () => import('./shell/components/screens/PluginRoutePage.vue'),
      })
      const entry: RouteEntry = {
        pluginId,
        route: { ...route, header },
        routeName: pluginRouteName(pluginId, route.id),
      }
      const unregister = registerRouteEntry(entry)
      pushLog('debug', pluginId, `注册插件路由: ${route.id}`)
      return track({
        dispose() {
          removeRoute()
          unregister.dispose()
        },
      })
    },
    openPage(routeId: string): void {
      const entry = findRoute(pluginId, routeId)
      if (!entry) {
        pushLog('warn', pluginId, `openPage("${routeId}") 未找到已注册路由`)
        return
      }
      const router = getSharedModule('router')
      router.push({ name: entry.routeName }).catch((e: unknown) => {
        pushLog('error', pluginId, `openPage("${routeId}") 跳转失败: ${String(e)}`)
      })
    },
    goBack(): void {
      getSharedModule('router').back()
    },
    // Android 系统返回键：dev-shell 无系统返回概念，静默降级（回调永不触发），
    // 保持与宿主插件 API 形状一致，避免插件在浏览器环境调用报错
    onBackPressed() {
      return { dispose() {} }
    },
    showDialog(options: PluginDialogOptions): PluginDialogHandle {
      return openGlobalDialog({ ...options, pluginContext: contextRef! })
    },
  }

  // ==================== EventAPI ====================
  const events: EventAPI = {
    on(event: string, handler: (...args: any[]) => void): Disposable {
      return track(onDevEvent(event, handler))
    },
    emit(event: string, ...args: any[]): void {
      emitDevEvent(event, ...args)
    },
  }

  // ==================== StorageAPI ====================
  const storage = {
    async get<T = any>(key: string): Promise<T | undefined> {
      try {
        const raw = localStorage.getItem(storageKey(pluginId, key))
        return raw === null ? undefined : (JSON.parse(raw) as T)
      } catch {
        return undefined
      }
    },
    async set(key: string, value: any): Promise<void> {
      try {
        localStorage.setItem(storageKey(pluginId, key), JSON.stringify(value))
      } catch {
        pushLog('warn', pluginId, `storage.set("${key}") 失败（localStorage 不可用）`)
      }
    },
    async delete(key: string): Promise<void> {
      localStorage.removeItem(storageKey(pluginId, key))
    },
  }

  // ==================== I18nAPI ====================
  const i18n: I18nAPI = {
    registerMessages(locale: string, messages: Record<string, unknown>): void {
      const hostI18n = (window as any).__BEDCODE_SHARED__?.i18n
      if (!hostI18n) return
      const prefixed: Record<string, unknown> = {}
      for (const [key, value] of Object.entries(messages)) {
        prefixed[`${pluginId}.${key}`] = value
      }
      const existing = hostI18n.global.getLocaleMessage(locale)
      hostI18n.global.mergeLocaleMessage(locale, { ...existing, ...prefixed })
    },
    t(key: string, params?: Record<string, unknown>): string {
      const hostI18n = (window as any).__BEDCODE_SHARED__?.i18n
      if (!hostI18n) return key
      return hostI18n.global.t(`${pluginId}.${key}`, params)
    },
  }

  // ==================== LifecycleAPI ====================
  const lifecycle: LifecycleAPI = {
    onAppStartup(handler) {
      return track(onDevEvent('plugin:lifecycle:appStartup', handler))
    },
    onAppShutdown(handler) {
      return track(onDevEvent('plugin:lifecycle:appShutdown', handler))
    },
    onAuthSuccess(handler) {
      return track(onDevEvent('plugin:lifecycle:authSuccess', handler))
    },
    onDisconnect(handler) {
      return track(onDevEvent('plugin:lifecycle:disconnect', (payload: any) => handler(payload.reason)))
    },
    onSessionCreated(handler) {
      return track(onDevEvent('plugin:lifecycle:sessionCreated', (payload: any) => handler(payload.sessionId)))
    },
    onSessionStopped(handler) {
      return track(onDevEvent('plugin:lifecycle:sessionStopped', (payload: any) => handler(payload.sessionId)))
    },
  }

  // ==================== LoggerAPI ====================
  const logger: LoggerAPI = {
    info(message: string) { pushLog('info', pluginId, message) },
    debug(message: string) { pushLog('debug', pluginId, message) },
    warn(message: string) { pushLog('warn', pluginId, message) },
    error(message: string) { pushLog('error', pluginId, message) },
  }

  // ==================== DialogAPI ====================
  const dialogs = {
    showDialog(options: DialogOptions) {
      return dialogService.showDialog(options)
    },
    showConfirm(options: DialogOptions) {
      return dialogService.showConfirm(options)
    },
    showPrompt(options: DialogOptions) {
      return dialogService.showPrompt(options)
    },
    showToast(message: string, type: 'info' | 'success' | 'warning' | 'error' = 'info') {
      dialogService.showToast(message, type)
    },
  }

  // ==================== NotificationAPI ====================
  const notifications: NotificationAPI = {
    async notify(title, body) {
      const msg = body ? `${title}: ${body}` : title
      if ('Notification' in window) {
        if (Notification.permission === 'default') {
          await Notification.requestPermission().catch(() => undefined)
        }
        if (Notification.permission === 'granted') {
          new Notification(title, { body })
          return
        }
      }
      dialogService.showToast(`[通知] ${msg}`, 'info')
    },
  }

  // ==================== StatusAPI ====================
  const status: StatusAPI = {
    async reportReady() {
      const record = getPluginRecord(pluginId)
      if (record) {
        record.state = 'activated'
        record.error = undefined
      }
      pushLog('info', pluginId, 'status.reportReady() 插件已就绪')
    },
    async reportError(error) {
      const record = getPluginRecord(pluginId)
      if (record) {
        record.state = 'error'
        record.error = error
      }
      pushLog('error', pluginId, `status.reportError: ${error}`)
    },
  }

  // ==================== SystemAPI（dev-shell 浏览器环境 no-op，与宿主接口对齐） ====================
  const system = {
    async openFile(_path: string, _displayName?: string): Promise<void> {
      pushLog('info', pluginId, 'system.openFile (mock) 浏览器环境不支持')
    },
    async revealInDir(_path: string): Promise<void> {
      pushLog('info', pluginId, 'system.revealInDir (mock) 浏览器环境不支持')
    },
    async revealReceivedFileLocation(_fileName: string): Promise<void> {
      pushLog('info', pluginId, 'system.revealReceivedFileLocation (mock) 浏览器环境不支持')
    },
    async requestAllFilesAccess(): Promise<boolean> {
      pushLog('info', pluginId, 'system.requestAllFilesAccess (mock) 浏览器环境无此权限')
      return true
    },
    async openDownloadDir(): Promise<void> {
      pushLog('info', pluginId, 'system.openDownloadDir (mock) 浏览器环境不支持')
    },
  }

  const context: PluginContext = {
    id: pluginId,
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
  contextRef = context
  return context
}
