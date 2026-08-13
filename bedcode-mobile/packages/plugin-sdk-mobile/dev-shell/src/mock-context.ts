/**
 * Mock PluginContext
 *
 * 与宿主 context.ts 同接口、同语义（事件名 / i18n 前缀 / storage 命名空间 / UI 注册），
 * 但全部后端通道替换为浏览器实现：
 * - commands.execute：仅执行前端注册 handler；WASM 后端不在浏览器运行，未注册命令记日志
 * - storage：localStorage 持久化
 * - terminal/session/lifecycle：接 mock/session.ts 的模拟会话
 * - fileService：内存挂载点 + 模拟目录/文件选择
 * - 权限检查跳过（dev-shell 视为全部授权，README 已说明与真机的差异）
 */
import type {
  DialogOptions,
  Disposable,
  EventAPI,
  FileServiceAPI,
  I18nAPI,
  LifecycleAPI,
  LoggerAPI,
  NotificationAPI,
  PluginContext,
  StatusAPI,
  UIRegistry,
} from '../../src/types'
import {
  authSuccess,
  createSession,
  emitDevEvent,
  onDevEvent,
  sendInputToSession,
  sessions,
  setConnected,
  stopSession,
} from './mock/session'
import { dialogService } from './mock/dialog-service'
import { getDevMock } from './registry'
import {
  getPluginRecord,
  goBackView,
  openActiveView,
  pushLog,
  registerMount,
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

/**
 * dev-shell 的 SAF mock：模拟目录树遍历 + 定时推进的中转复制
 *
 * 与真机语义对齐：listTree 按 documentId 分目录；copyStart 立即返回句柄，
 * 进度由定时器推进（约 4MB/s），取消置位后停止；浏览器无 cache 概念，
 * destPath 用模拟路径。目录树由插件 devMock.safTree 提供（领域数据归插件）。
 */
function createMockSaf(
  tree: Record<string, import('../../src/types').SafTreeEntry[]>,
): import('../../src/types').SafAPI {
  interface MockCopy {
    done: number
    total: number
    finished: boolean
    cancelled: boolean
    timer: ReturnType<typeof setInterval> | null
  }
  const copies = new Map<string, MockCopy>()

  return {
    async listTree(treeUri: string, documentId: string) {
      // 精确目录优先；树根（pick 返回的 documentId 不入树）回退到 mock 根
      const entries =
        tree[documentId] ?? (treeUri.includes('mock') ? (tree['mock:root'] ?? []) : [])
      return entries.map(e => ({
        name: e.name,
        isDir: e.isDir,
        size: e.size,
        mime: e.mime,
        uri: `${treeUri}/document/${e.docId}`,
        documentId: e.docId,
      }))
    },
    async copyStart(uri: string, destName: string) {
      const copyId = `mock-copy-${copies.size + 1}`
      const total = 86_400_000
      const copy: MockCopy = { done: 0, total, finished: false, cancelled: false, timer: null }
      copies.set(copyId, copy)
      copy.timer = setInterval(() => {
        copy.done = Math.min(total, copy.done + 2_400_000)
        if (copy.done >= total || copy.cancelled) {
          copy.finished = true
          if (copy.timer) clearInterval(copy.timer)
          copy.timer = null
        }
      }, 400)
      return { copyId, destPath: `/mock/cache/bedcode_uploads/${destName}` }
    },
    async copyStatus(copyId: string) {
      const copy = copies.get(copyId)
      if (!copy) throw new Error(`unknown copyId ${copyId}`)
      return {
        copyId,
        done: copy.done,
        total: copy.total,
        finished: copy.finished,
        cancelled: copy.cancelled,
        error: null,
        destPath: `/mock/cache/bedcode_uploads/${copyId}.bin`,
      }
    },
    async copyCancel(copyId: string) {
      const copy = copies.get(copyId)
      if (!copy) throw new Error(`unknown copyId ${copyId}`)
      copy.cancelled = true
    },
    async cleanupStaleCopies() {
      // dev-shell：无真实 cache 文件，仅清空内存复制表
      copies.clear()
    },
    async checkAuthorized(_treeUri: string) {
      return true
    },
  }
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
      // 对端（桌面端）信息来自控制面公告，浏览器中不可用
      return null
    },
    // ==================== v2 批量传输批准（dev-shell mock） ====================
    async approveTransferRequest(_batchId) {
      pushLog('info', pluginId, 'fileService.approveTransferRequest (mock) 已批准')
    },
    async rejectTransferRequest(_batchId) {
      pushLog('info', pluginId, 'fileService.rejectTransferRequest (mock) 已拒绝')
    },
    async setApprovalTimeout(mountPath, seconds) {
      pushLog('info', pluginId, `fileService.setApprovalTimeout "${mountPath}" ${seconds}s (mock)`)
    },
    async cancelReceivingSession(sessionId) {
      pushLog('info', pluginId, `fileService.cancelReceivingSession ${sessionId} (mock)`)
    },
    async pickDirectory() {
      const value = await dialogService.showPrompt({
        title: '选择目录（dev-shell mock）',
        message: '浏览器无法调起系统目录选择器，请手动输入模拟目录路径',
        inputPlaceholder: '如 /sdcard/Download',
        inputValue: '/sdcard/Download',
      })
      return value
    },
    async pickFile() {
      const value = await dialogService.showPrompt({
        title: '选择文件（dev-shell mock）',
        message: '浏览器无法调起系统文件选择器，请手动输入模拟文件路径',
        inputPlaceholder: '如 /sdcard/Download/example.txt',
        inputValue: '/sdcard/Download/example.txt',
      })
      return value
    },
    // Android 11+ 分区存储的「所有文件访问权限」：浏览器环境无此概念，
    // 恒返回 false（未授权），插件侧应展示引导 UI 而非报错，与真机行为对齐
    async requestAllFilesAccess() {
      return false
    },
    async pickSharedDirectory() {
      const value = await dialogService.showPrompt({
        title: '选择共享目录（dev-shell mock）',
        message: '浏览器无法调起 SAF 目录树选择器，输入模拟目录名',
        inputPlaceholder: '如 模拟共享目录',
        inputValue: '模拟共享目录',
      })
      if (!value) return null
      return {
        uri: `content://tree/mock-${encodeURIComponent(value)}`,
        documentId: `mock:${encodeURIComponent(value)}`,
        displayName: value,
      }
    },
    async listDir(path: string) {
      // 免授权特殊条目（app 私有下载目录）浏览：条目由插件 devMock.listDirEntries
      // 提供（领域数据归插件）；未注册该插件时返回空列表
      const entries = getDevMock(pluginId)?.listDirEntries
      if (!entries?.length) return []
      return entries.map((e) => ({
        name: e.name,
        isDir: e.isDir,
        size: e.size,
        mime: e.mime,
        uri: `${path}/${e.name}`,
        documentId: '',
      }))
    },
    // SAF 存储访问（dev-shell mock）：目录树由插件 devMock.safTree 提供
    saf: createMockSaf(getDevMock(pluginId)?.safTree ?? {}),
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

  return {
    id: pluginId,
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
