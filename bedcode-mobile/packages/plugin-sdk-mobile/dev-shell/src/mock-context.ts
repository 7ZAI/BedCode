/**
 * Mock PluginContext
 *
 * 与宿主 context.ts 同接口、同语义（事件名 / i18n 前缀 / storage 命名空间 / UI 注册），
 * 但全部后端通道替换为浏览器实现：
 * - commands.execute：仅执行前端注册 handler；WASM 后端不在浏览器运行，未注册命令记日志
 * - storage：localStorage 持久化
 * - terminal/session/lifecycle：接 mock/session.ts 的模拟会话
 * - 权限检查跳过（dev-shell 视为全部授权，README 已说明与真机的差异）
 */
import type {
  DialogOptions,
  Disposable,
  EventAPI,
  I18nAPI,
  LifecycleAPI,
  LoggerAPI,
  NotificationAPI,
  OcrApi,
  OcrEngineStatus,
  PluginContext,
  StatusAPI,
  UIRegistry,
} from '../../src/types'
import {
  emitDevEvent,
  onDevEvent,
  sendInputToSession,
  sessions,
} from './mock/session'
import { dialogService } from './mock/dialog-service'
import { getDevMock } from './registry'
import {
  getPluginRecord,
  goBackView,
  openActiveView,
  pushLog,
  registerNavTab,
  registerRoute,
  registerSettingsSection,
  registerTerminalToolbarItem,
  registerToolboxPage,
  routes as routeEntries,
} from './registry'

/** 存储命名空间（与宿主插件 storage 的 per-plugin 隔离一致） */
function storageKey(pluginId: string, key: string): string {
  return `bedcode-dev-shell:${pluginId}:${key}`
}

/** 创建插件的 PluginContext */
export function createMockContext(pluginId: string): PluginContext {
  const disposables: Disposable[] = []

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

  // ==================== TerminalAPI ====================
  const terminal = {
    async sendInput(sessionId: string, text: string): Promise<void> {
      sendInputToSession(sessionId, text)
    },
    onOutput(handler: (sessionId: string, data: string) => void): Disposable {
      return track(onDevEvent('terminal:output', (payload: any) => handler(payload.sessionId, payload.data)))
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
  const ui: UIRegistry = {
    registerToolboxPage(page) {
      return track(registerToolboxPage(pluginId, page))
    },
    registerNavTab(tab) {
      return track(registerNavTab(pluginId, tab))
    },
    registerTerminalToolbarItem(item) {
      return track(registerTerminalToolbarItem(pluginId, item))
    },
    registerSettingsSection(section) {
      return track(registerSettingsSection(pluginId, section))
    },
    registerRoute(route) {
      const disposable = track(registerRoute(pluginId, route))
      // 宿主在 vue-router 上 addRoute；dev-shell 直接用注册表驱动（openPage 经全局 activeView）
      return disposable
    },
    openPage(routeId: string): void {
      const entry = routeEntries.value.find(
        (r) => r.pluginId === pluginId && r.route.id === routeId,
      )
      if (!entry) {
        pushLog('warn', pluginId, `openPage("${routeId}") 未找到已注册路由`)
        return
      }
      openActiveView({
        kind: 'route',
        pluginId,
        title: entry.route.title,
        header: entry.route.header ?? true,
        component: entry.route.component,
      })
    },
    goBack(): void {
      goBackView()
    },
    // Android 系统返回键：dev-shell 无系统返回概念，静默降级（回调永不触发），
    // 保持与宿主插件 API 形状一致，避免插件在浏览器环境调用报错
    onBackPressed() {
      return { dispose() {} }
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
    registerMessages(locale: string, messages: Record<string, any>): void {
      const hostI18n = (window as any).__BEDCODE_SHARED__?.i18n
      if (!hostI18n) return
      const prefixed: Record<string, any> = {}
      for (const [key, value] of Object.entries(messages)) {
        prefixed[`${pluginId}.${key}`] = value
      }
      const existing = hostI18n.global.getLocaleMessage(locale)
      hostI18n.global.mergeLocaleMessage(locale, { ...existing, ...prefixed })
    },
    t(key: string, params?: Record<string, any>): string {
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
    onTerminalInput(handler) {
      return track(onDevEvent('plugin:lifecycle:terminalInput', (payload: any) => handler(payload.sessionId, payload.data)))
    },
    onTerminalOutput(handler) {
      return track(onDevEvent('plugin:lifecycle:terminalOutput', (payload: any) => handler(payload.sessionId, payload.data)))
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

  // ==================== OcrAPI（dev-shell mock） ====================

  /** mock 引擎状态：模型在位、引擎常驻（dev-shell 恒就绪） */
  const mockEngineStatus: OcrEngineStatus = {
    available: true,
    modelsPresent: true,
    modelsBytes: 17_417_728,
    engineLoaded: true,
    supportedEngines: ['offline'],
  }

  const ocr: OcrApi = {
    async recognize(_input) {
      // 识别结果种子由插件 devMock.ocrLinesSeed 持有（dev-shell 不含业务示例数据）；
      // 空数组/缺省均可演示「未识别到文字」空态
      const seed = getDevMock(pluginId)?.ocrLinesSeed
      pushLog('info', pluginId, `ocr.recognize (mock) 返回 ${seed?.length ?? 0} 行`)
      return {
        engine: 'offline',
        durationMs: 86,
        lines: seed ?? [],
      }
    },
    async engineStatus() {
      pushLog('info', pluginId, 'ocr.engineStatus (mock) 引擎就绪')
      return mockEngineStatus
    },
    async deleteModels() {
      // 内存态：删除后 modelsPresent=false，演示「模型缺失引导恢复」链路
      mockEngineStatus.modelsPresent = false
      mockEngineStatus.modelsBytes = 0
      mockEngineStatus.engineLoaded = false
      pushLog('info', pluginId, 'ocr.deleteModels (mock) 已删除')
      return { deleted: true, freedBytes: 17_417_728 }
    },
    async restoreModels() {
      mockEngineStatus.modelsPresent = true
      mockEngineStatus.modelsBytes = 17_417_728
      mockEngineStatus.engineLoaded = true
      pushLog('info', pluginId, 'ocr.restoreModels (mock) 已恢复')
      return { restored: true }
    },
    async pickImage() {
      pushLog('info', pluginId, 'ocr.pickImage (mock) 模拟相册选图')
      return { path: '/mock/cache/ocr/ocr_mock_pick.rgba', width: 1000, height: 750 }
    },
    async cameraCapture() {
      pushLog('info', pluginId, 'ocr.cameraCapture (mock) 模拟拍照（权限已授予）')
      return { path: '/mock/cache/ocr/ocr_mock_camera.rgba', width: 1200, height: 900 }
    },
  }

  return {
    id: pluginId,
    commands,
    terminal,
    session,
    ui,
    events,
    storage,
    ocr,
    i18n,
    lifecycle,
    logger,
    dialogs,
    notifications,
    status,
    _disposables: disposables,
  }
}
