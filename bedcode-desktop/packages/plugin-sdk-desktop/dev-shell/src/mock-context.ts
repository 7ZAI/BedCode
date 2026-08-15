/**
 * Mock PluginContext（桌面端）
 *
 * 与宿主 context.ts 同接口、同语义（事件名 / i18n 前缀 / storage 命名空间），
 * 后端通道替换为浏览器实现：
 * - commands.execute：仅执行前端注册 handler；WASM 后端不在浏览器运行
 * - storage：localStorage 持久化（flush 为空操作）
 * - http.registerEndpoint：仅登记展示（真实宿主由 Rust 服务端挂载，浏览器不可达）
 * - fileService：内存挂载点 + 模拟目录/文件选择（pickFiles 返回数组）
 * - 权限检查跳过（dev-shell 视为全部授予）
 */
import type {
  Disposable,
  FileServiceAPI,
  HttpAPI,
  I18nAPI,
  PluginContext,
  UIRegistry,
} from '../../src/types'
import {
  emitDevEvent,
  onDevEvent,
  sendInputToSession,
  sendOutput,
  sessions,
} from './mock/session'
import { dialogService } from './mock/dialog-service'
import {
  pushLog,
  registerEndpoint,
  registerFileHandler,
  registerInputExtension,
  registerMount,
  registerPageToolbarItem,
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
      return track(onDevEvent('terminal:output', (payload: any) => handler(payload.sessionId, payload.data)))
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
  }

  // ==================== UIRegistry ====================
  const ui: UIRegistry = {
    registerSidebarPanel(panel) {
      return track(registerSidebarPanel(pluginId, panel))
    },
    registerToolboxPage(page) {
      return track(registerToolboxPage(pluginId, page))
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

  // ==================== FileServiceAPI ====================
  const fileService: FileServiceAPI = {
    async mount(options) {
      const handle = registerMount(
        pluginId,
        options.mountPath,
        options.roots,
        options.operations,
      )
      pushLog(
        'info',
        pluginId,
        `fileService.mount "${options.mountPath}" roots=[${options.roots.join(', ')}]`,
      )
      return {
        mountPath: options.mountPath,
        async updateRoots(roots: string[]) {
          handle.updateRoots(roots)
          pushLog('info', pluginId, `fileService.updateRoots "${options.mountPath}" -> [${roots.join(', ')}]`)
        },
        async dispose() {
          handle.dispose()
          pushLog('info', pluginId, `fileService 卸载 "${options.mountPath}"`)
        },
      }
    },
    async getPeerInfo(_peerId) {
      return null
    },
    async pickDirectory() {
      return promptForMockPath(
        '选择目录（dev-shell mock）',
        '浏览器无法调起系统目录选择器，请手动输入模拟目录路径',
        'C:\\mock\\downloads',
      )
    },
    async pickFiles() {
      const value = await promptForMockPath(
        '选择文件（dev-shell mock，逗号分隔多个）',
        '浏览器无法调起系统文件选择器，请手动输入模拟文件路径',
        'C:\\mock\\a.txt, C:\\mock\\b.txt',
      )
      return value
        ? value
            .split(/[,，]/)
            .map((s) => s.trim())
            .filter(Boolean)
        : []
    },
  }

  function promptForMockPath(
    title: string,
    message: string,
    placeholder: string,
  ): Promise<string | null> {
    return dialogService.showPrompt({
      title,
      message,
      inputPlaceholder: placeholder,
      inputValue: placeholder,
    })
  }

  // ==================== I18nAPI ====================
  const i18n: I18nAPI = {
    getI18n() {
      return getSharedModule('i18n')
    },
    registerMessages(locale: string, messages: Record<string, any>): void {
      const hostI18n = getSharedModule('i18n')
      if (!hostI18n) return
      const prefixed: Record<string, any> = {}
      for (const [key, value] of Object.entries(messages)) {
        prefixed[`${pluginId}.${key}`] = value
      }
      const existing = hostI18n.global.getLocaleMessage(locale)
      hostI18n.global.mergeLocaleMessage(locale, { ...existing, ...prefixed })
    },
    t(key: string, params?: Record<string, any>): string {
      const hostI18n = getSharedModule('i18n')
      if (!hostI18n) return key
      return hostI18n.global.t(`${pluginId}.${key}`, params)
    },
  }

  return {
    id: pluginId,
    extensionPath,
    commands,
    terminal,
    session,
    ui,
    events,
    storage,
    http,
    fileService,
    i18n,
    _disposables: disposables,
  }
}
