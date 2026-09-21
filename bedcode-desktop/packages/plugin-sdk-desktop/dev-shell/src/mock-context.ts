/**
 * Mock PluginContext（桌面端）
 *
 * 与宿主 context.ts 同接口、同语义（事件名 / i18n 前缀 / storage 命名空间），
 * 后端通道替换为浏览器实现：
 * - commands.execute：仅执行前端注册 handler；WASM 后端不在浏览器运行
 * - storage：localStorage 持久化（flush 为空操作）
 * - http.registerEndpoint：仅登记展示（真实宿主由 Rust 服务端挂载，浏览器不可达）
 * - 权限检查跳过（dev-shell 视为全部授予）
 */
import type {
  Disposable,
  HttpAPI,
  I18nAPI,
  PluginContext,
  PluginDialogHandle,
  PluginDialogOptions,
  UIRegistry,
} from '../../src/types'
import { openGlobalDialog } from '../../src/global-dialog'
import { emitDevEvent, onDevEvent, sendInputToSession, sessions } from './mock/session'
import {
  pushLog,
  registerEndpoint,
  registerFileHandler,
  registerInputExtension,
  registerPage,
  registerPageToolbarItem,
  registerSettingsSection,
  registerSidebarPanel,
  registerStatusBarItem,
  registerTerminalToolbarItem,
  registerTitleBarItem,
  registerToolboxPage,
} from './registry'
import { getSharedModule } from './shared-runtime'

function storageKey(pluginId: string, key: string): string {
  return `bedcode-dev-shell:${pluginId}:${key}`
}

/** 创建插件的 PluginContext */
export function createMockContext(pluginId: string, extensionPath: string): PluginContext {
  const disposables: Disposable[] = []

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
        `command "${id}" 未注册前端 handler——Rust 后端不在浏览器运行，请注册前端 handler 或到真机验证`,
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
      return track(
        onDevEvent('terminal:output', (payload: any) => handler(payload.sessionId, payload.data)),
      )
    },
    onInput(handler: (sessionId: string, text: string) => string | null): Disposable {
      return track(
        onDevEvent('terminal:input', (payload: any) => {
          const result = handler(payload.sessionId, payload.text)
          // 返回非 null 视为改写后的输入，记入日志便于排查
          if (typeof result === 'string') {
            pushLog('debug', pluginId, `terminal.onInput 改写: "${payload.text}" -> "${result}"`)
          }
        }),
      )
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
    // ==================== 宿主 SessionAPI 扩展（终端窗口原语，mock 为浏览器 no-op） ====================
    async predictTerminalSize(): Promise<{ cols: number; rows: number } | null> {
      // 浏览器环境无桌面窗口/字体测量，返回 null（调用方按不可知处理）
      return null
    },
    async openTerminal(_session: { id: string; name: string }): Promise<boolean> {
      pushLog('info', pluginId, 'session.openTerminal (mock) 浏览器环境无终端窗口')
      return false
    },
    async closeTerminal(_sessionId: string): Promise<void> {
      // no-op：无窗口概念
    },
    isTerminalOpen(_sessionId: string): boolean {
      return false
    },
  }

  // ==================== UIRegistry ====================
  const ui: UIRegistry = {
    registerSidebarPanel(panel) {
      return track(registerSidebarPanel(pluginId, panel))
    },
    registerToolboxPage(page) {
      return track(registerToolboxPage(pluginId, page))
    },
    registerPage(page) {
      return track(registerPage(pluginId, page))
    },
    registerStatusBarItem(item) {
      return track(registerStatusBarItem(pluginId, item))
    },
    registerInputExtension(ext) {
      return track(registerInputExtension(pluginId, ext))
    },
    registerTerminalToolbarItem(item) {
      return track(registerTerminalToolbarItem(pluginId, item))
    },
    registerTitleBarItem(item) {
      return track(registerTitleBarItem(pluginId, item))
    },
    registerPageToolbarItem(item) {
      return track(registerPageToolbarItem(pluginId, item))
    },
    registerFileHandler(handler) {
      return track(registerFileHandler(pluginId, handler))
    },
    registerSettingsSection(section) {
      return track(registerSettingsSection(pluginId, section))
    },
    showDialog(options: PluginDialogOptions): PluginDialogHandle {
      // context 在本函数尾部组装；惰性引用（showDialog 调用时已初始化），供内容组件 provide
      return openGlobalDialog({ ...options, pluginContext: context })
    },
  }

  // ==================== EventAPI ====================
  const events = {
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
    async flush(): Promise<void> {
      // localStorage 同步写入，无需刷新
    },
  }

  // ==================== HttpAPI ====================
  const http: HttpAPI = {
    registerEndpoint(path: string): Disposable {
      // 浏览器中端点不可达（真实宿主由 Rust 服务端挂载），仅登记展示
      return track(registerEndpoint(pluginId, path))
    },
  }

  // ==================== I18nAPI ====================
  const i18n: I18nAPI = {
    getI18n() {
      return getSharedModule('i18n')
    },
    registerMessages(locale: string, messages: Record<string, unknown>): void {
      const hostI18n = getSharedModule('i18n')
      if (!hostI18n) return
      const prefixed: Record<string, unknown> = {}
      for (const [key, value] of Object.entries(messages)) {
        prefixed[`${pluginId}.${key}`] = value
      }
      const existing = hostI18n.global.getLocaleMessage(locale)
      hostI18n.global.mergeLocaleMessage(locale, { ...existing, ...prefixed })
    },
    t(key: string, params?: Record<string, unknown>): string {
      const hostI18n = getSharedModule('i18n')
      if (!hostI18n) return key
      return hostI18n.global.t(`${pluginId}.${key}`, params)
    },
  }

  // 注：原 `system.revealInDir` 插件 API 已退役——定位改为内核原语
  // `host-platform.reveal-in-dir`（ABI v22），dev-shell 无对应 mock（插件在 WASM 侧调用）

  const context = {
    id: pluginId,
    extensionPath,
    commands,
    terminal,
    session,
    ui,
    events,
    storage,
    http,
    i18n,
    _disposables: disposables,
  }
  return context
}
