/**
 * Dev Shell 全局注册表（桌面端）
 *
 * 模块级响应式状态：插件记录、UI 注册项（sidebar / toolbox / statusbar /
 * inputExtension / terminalToolbar / titleBar / pageToolbar / fileHandler /
 * http endpoint / mount）、日志、当前打开的插件视图。
 */
import { ref } from 'vue'
import type {
  Disposable,
  FileHandlerDescriptor,
  InputExtensionDescriptor,
  PageToolbarItemDescriptor,
  RequestHandler,
  SidebarPanelDescriptor,
  StatusBarItemDescriptor,
  TerminalToolbarItemDescriptor,
  TitleBarItemDescriptor,
  ToolboxPageDescriptor,
} from '../../src/types'

// ==================== 日志 ====================

export interface DevLogEntry {
  id: number
  ts: string
  pluginId: string
  level: 'debug' | 'info' | 'warn' | 'error'
  message: string
}

const logs = ref<DevLogEntry[]>([])
let nextLogId = 0
const MAX_LOGS = 500

export function pushLog(
  level: DevLogEntry['level'],
  pluginId: string,
  message: string,
): void {
  const entry: DevLogEntry = {
    id: ++nextLogId,
    ts: new Date().toLocaleTimeString('zh-CN', { hour12: false }),
    pluginId,
    level,
    message,
  }
  logs.value.push(entry)
  if (logs.value.length > MAX_LOGS) logs.value.splice(0, logs.value.length - MAX_LOGS)
  const fn = level === 'error' ? console.error : level === 'warn' ? console.warn : console.log
  fn(`[dev-shell][${pluginId}] ${message}`)
}

export function clearLogs(): void {
  logs.value = []
}

// ==================== 插件记录 ====================

export type DevPluginState = 'loaded' | 'activated' | 'deactivated' | 'error'

export interface DevPluginRecord {
  id: string
  name: string
  manifest: Record<string, any>
  entry: any
  dir: string
  state: DevPluginState
  error?: string
  context: any
}

const plugins = ref<DevPluginRecord[]>([])

export function getPluginRecord(pluginId: string): DevPluginRecord | undefined {
  return plugins.value.find((p) => p.id === pluginId)
}

// ==================== UI 注册项 ====================

export interface SidebarPanelEntry {
  pluginId: string
  panel: SidebarPanelDescriptor
}
export interface ToolboxPageEntry {
  pluginId: string
  page: ToolboxPageDescriptor
}
export interface StatusBarEntry {
  pluginId: string
  item: StatusBarItemDescriptor
}
export interface InputExtensionEntry {
  pluginId: string
  ext: InputExtensionDescriptor
}
export interface TerminalToolbarEntry {
  pluginId: string
  item: TerminalToolbarItemDescriptor
}
export interface TitleBarEntry {
  pluginId: string
  item: TitleBarItemDescriptor
}
export interface PageToolbarEntry {
  pluginId: string
  item: PageToolbarItemDescriptor
}
export interface FileHandlerEntry {
  pluginId: string
  handler: FileHandlerDescriptor
}
export interface EndpointEntry {
  pluginId: string
  path: string
}
export interface MountEntry {
  pluginId: string
  mountPath: string
  roots: string[]
  operations: string[]
}

const sidebarPanels = ref<SidebarPanelEntry[]>([])
const toolboxPages = ref<ToolboxPageEntry[]>([])
const statusBarItems = ref<StatusBarEntry[]>([])
const inputExtensions = ref<InputExtensionEntry[]>([])
const terminalToolbarItems = ref<TerminalToolbarEntry[]>([])
const titleBarItems = ref<TitleBarEntry[]>([])
const pageToolbarItems = ref<PageToolbarEntry[]>([])
const fileHandlers = ref<FileHandlerEntry[]>([])
const endpoints = ref<EndpointEntry[]>([])
const mounts = ref<MountEntry[]>([])

function makeDisposable<T>(list: { value: T[] }, entry: T): Disposable {
  return {
    dispose() {
      const idx = list.value.indexOf(entry)
      if (idx !== -1) list.value.splice(idx, 1)
    },
  }
}

export function registerSidebarPanel(pluginId: string, panel: SidebarPanelDescriptor): Disposable {
  const entry: SidebarPanelEntry = { pluginId, panel }
  sidebarPanels.value.push(entry)
  pushLog('debug', pluginId, `注册侧边栏面板: ${panel.title}`)
  return makeDisposable(sidebarPanels, entry)
}

export function registerToolboxPage(pluginId: string, page: ToolboxPageDescriptor): Disposable {
  const entry: ToolboxPageEntry = { pluginId, page }
  toolboxPages.value.push(entry)
  pushLog('debug', pluginId, `注册工具箱页: ${page.title || page.id}`)
  return makeDisposable(toolboxPages, entry)
}

export function registerStatusBarItem(pluginId: string, item: StatusBarItemDescriptor): Disposable {
  const entry: StatusBarEntry = { pluginId, item }
  statusBarItems.value.push(entry)
  pushLog('debug', pluginId, `注册状态栏项: ${item.label}`)
  return makeDisposable(statusBarItems, entry)
}

export function registerInputExtension(pluginId: string, ext: InputExtensionDescriptor): Disposable {
  const entry: InputExtensionEntry = { pluginId, ext }
  inputExtensions.value.push(entry)
  pushLog('debug', pluginId, `注册输入扩展: ${ext.label}`)
  return makeDisposable(inputExtensions, entry)
}

export function registerTerminalToolbarItem(pluginId: string, item: TerminalToolbarItemDescriptor): Disposable {
  const entry: TerminalToolbarEntry = { pluginId, item }
  terminalToolbarItems.value.push(entry)
  pushLog('debug', pluginId, `注册终端工具栏项: ${item.label}`)
  return makeDisposable(terminalToolbarItems, entry)
}

export function registerTitleBarItem(pluginId: string, item: TitleBarItemDescriptor): Disposable {
  const entry: TitleBarEntry = { pluginId, item }
  titleBarItems.value.push(entry)
  pushLog('debug', pluginId, `注册标题栏项: ${item.label}`)
  return makeDisposable(titleBarItems, entry)
}

export function registerPageToolbarItem(pluginId: string, item: PageToolbarItemDescriptor): Disposable {
  const entry: PageToolbarEntry = { pluginId, item }
  pageToolbarItems.value.push(entry)
  pushLog('debug', pluginId, `注册页面工具栏项: ${item.label} -> ${item.target}`)
  return makeDisposable(pageToolbarItems, entry)
}

export function registerFileHandler(pluginId: string, handler: FileHandlerDescriptor): Disposable {
  const entry: FileHandlerEntry = { pluginId, handler }
  fileHandlers.value.push(entry)
  pushLog('debug', pluginId, `注册文件处理器: ${handler.id} (${handler.extensions.join(', ')})`)
  return makeDisposable(fileHandlers, entry)
}

export function registerEndpoint(pluginId: string, path: string): Disposable {
  const entry: EndpointEntry = { pluginId, path }
  endpoints.value.push(entry)
  pushLog('debug', pluginId, `注册 HTTP 端点: ${path}（浏览器中不可达，仅展示）`)
  return makeDisposable(endpoints, entry)
}

export function registerMount(
  pluginId: string,
  mountPath: string,
  roots: string[],
  operations: string[],
): { updateRoots(roots: string[]): void; dispose(): void } {
  const entry: MountEntry = { pluginId, mountPath, roots, operations }
  mounts.value.push(entry)
  return {
    updateRoots(next: string[]) {
      entry.roots = next
    },
    dispose() {
      const idx = mounts.value.indexOf(entry)
      if (idx !== -1) mounts.value.splice(idx, 1)
    },
  }
}

// ==================== 当前打开的插件视图 ====================

export interface ActiveView {
  kind: 'sidebar' | 'toolbox'
  pluginId: string
  title?: string
  component: any
}

const activeView = ref<ActiveView | null>(null)

export function openActiveView(view: ActiveView | null): void {
  activeView.value = view
}

export {
  activeView,
  endpoints,
  fileHandlers,
  inputExtensions,
  logs,
  mounts,
  pageToolbarItems,
  plugins,
  sidebarPanels,
  statusBarItems,
  terminalToolbarItems,
  titleBarItems,
  toolboxPages,
}
