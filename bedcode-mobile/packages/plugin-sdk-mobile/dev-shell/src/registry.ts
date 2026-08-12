/**
 * Dev Shell 全局注册表
 *
 * 模块级响应式状态（跨组件共享单例）：
 * - 插件记录（state / error / context）
 * - 插件 UI 注册项（toolbox 页 / navTab / 终端工具栏 / 设置区 / 路由 / 挂载点）
 * - 日志面板数据
 * - 当前打开的插件视图（activeView，由 AppShell 渲染）
 *
 * 与宿主 plugin/registry.ts 的职责对应，但只服务浏览器 dev-shell 场景。
 */
import { reactive, ref } from 'vue'
import type {
  Disposable,
  NavTabDescriptor,
  PluginDevMock,
  PluginRouteDescriptor,
  SettingsSectionDescriptor,
  TerminalToolbarItemDescriptor,
  ToolboxPageDescriptor,
} from '../../src/types'

// ==================== 插件 devMock（领域数据注册） ====================

/** 按 pluginId 注册的开发期领域数据（loader 在 activate 前调用，deactivate 时清理） */
const devMocks = new Map<string, PluginDevMock>()

export function registerDevMock(pluginId: string, mock: PluginDevMock): Disposable {
  devMocks.set(pluginId, mock)
  return {
    dispose() {
      devMocks.delete(pluginId)
    },
  }
}

/** 取指定插件的领域数据（createMockContext 按 pluginId 合并用） */
export function getDevMock(pluginId: string): PluginDevMock | undefined {
  return devMocks.get(pluginId)
}

/** 全部已注册领域数据（mobileApi 等全局单例能力按序合并用，如队列种子） */
export function getAllDevMocks(): PluginDevMock[] {
  return [...devMocks.values()]
}

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

/** 记录日志（同步到 console，供浏览器 devtools 与日志面板双通道排查） */
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
  state: DevPluginState
  error?: string
  context: any
  /** devMock 注册句柄（deactivate 时清理） */
  devMockDisposable?: Disposable
}

const plugins = ref<DevPluginRecord[]>([])

export function getPluginRecord(pluginId: string): DevPluginRecord | undefined {
  return plugins.value.find((p) => p.id === pluginId)
}

// ==================== UI 注册项 ====================

export interface ToolboxPageEntry {
  pluginId: string
  page: ToolboxPageDescriptor
}
export interface NavTabEntry {
  pluginId: string
  tab: NavTabDescriptor
}
export interface TerminalToolbarEntry {
  pluginId: string
  item: TerminalToolbarItemDescriptor
}
export interface SettingsSectionEntry {
  pluginId: string
  section: SettingsSectionDescriptor
}
export interface RouteEntry {
  pluginId: string
  route: PluginRouteDescriptor
  /** router 路由名（registerRoute 时 addRoute，dispose 时 removeRoute） */
  routeName: string
}
export interface MountEntry {
  pluginId: string
  mountPath: string
  roots: string[]
  operations: string[]
}

const toolboxPages = ref<ToolboxPageEntry[]>([])
const navTabs = ref<NavTabEntry[]>([])
const terminalToolbarItems = ref<TerminalToolbarEntry[]>([])
const settingsSections = ref<SettingsSectionEntry[]>([])
const routes = ref<RouteEntry[]>([])
const mounts = ref<MountEntry[]>([])

/** 从列表中移除条目（dispose 回调） */
function makeDisposable<T>(list: { value: T[] }, entry: T): Disposable {
  return {
    dispose() {
      const idx = list.value.indexOf(entry)
      if (idx !== -1) list.value.splice(idx, 1)
    },
  }
}

export function registerToolboxPage(pluginId: string, page: ToolboxPageDescriptor): Disposable {
  const entry: ToolboxPageEntry = { pluginId, page }
  toolboxPages.value.push(entry)
  pushLog('debug', pluginId, `注册工具箱页: ${page.title || page.id}`)
  return makeDisposable(toolboxPages, entry)
}

export function registerNavTab(pluginId: string, tab: NavTabDescriptor): Disposable {
  const entry: NavTabEntry = { pluginId, tab }
  navTabs.value.push(entry)
  pushLog('debug', pluginId, `注册底部导航 Tab: ${tab.title}`)
  return makeDisposable(navTabs, entry)
}

export function registerTerminalToolbarItem(
  pluginId: string,
  item: TerminalToolbarItemDescriptor,
): Disposable {
  const entry: TerminalToolbarEntry = { pluginId, item }
  terminalToolbarItems.value.push(entry)
  pushLog('debug', pluginId, `注册终端工具栏项: ${item.label}`)
  return makeDisposable(terminalToolbarItems, entry)
}

export function registerSettingsSection(
  pluginId: string,
  section: SettingsSectionDescriptor,
): Disposable {
  const entry: SettingsSectionEntry = { pluginId, section }
  settingsSections.value.push(entry)
  pushLog('debug', pluginId, `注册设置区: ${section.section}`)
  return makeDisposable(settingsSections, entry)
}

export function registerRoute(pluginId: string, route: PluginRouteDescriptor): Disposable {
  const entry: RouteEntry = {
    pluginId,
    route,
    routeName: `dev-plugin-route-${routes.value.length}-${Date.now()}`,
  }
  routes.value.push(entry)
  pushLog('debug', pluginId, `注册插件路由: ${route.id}`)
  return {
    dispose() {
      const idx = routes.value.indexOf(entry)
      if (idx !== -1) routes.value.splice(idx, 1)
    },
  }
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

// ==================== 当前打开的插件视图（视图栈，与宿主路由栈语义一致） ====================

export interface ActiveView {
  kind: 'toolbox' | 'navtab' | 'settings' | 'route'
  pluginId: string
  title?: string
  /** 是否渲染宿主页头（back + title），缺省 true */
  header?: boolean
  component: any
  /** navTab 去重标记（内部） */
  _tabId?: string
}

const viewStack = ref<ActiveView[]>([])
const activeView = ref<ActiveView | null>(null)

/** 打开视图（压栈；null = 清空回 Tab 内容） */
export function openActiveView(view: ActiveView | null): void {
  if (view === null) {
    viewStack.value = []
    activeView.value = null
    return
  }
  viewStack.value.push(view)
  activeView.value = view
}

/** 返回上一视图（对应宿主 router.back()；无上层时回 Tab 内容） */
export function goBackView(): void {
  viewStack.value.pop()
  activeView.value = viewStack.value[viewStack.value.length - 1] ?? null
}

export { activeView, logs, plugins, toolboxPages, navTabs, terminalToolbarItems, settingsSections, routes, mounts }
