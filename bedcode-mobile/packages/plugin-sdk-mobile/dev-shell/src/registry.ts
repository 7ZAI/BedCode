/**
 * Dev Shell 全局注册表
 *
 * 模块级响应式状态（跨组件共享单例）：
 * - 插件记录（state / error / context）
 * - 插件 UI 注册项（壳运行面 / 快捷卡片 / 胶囊项 / 设置入口 / 路由）
 * - 日志面板数据
 * - 当前打开的插件视图（activeView，由 AppShell 渲染）
 *
 * 与宿主 plugin/registry.ts 的职责对应，但只服务浏览器 dev-shell 场景。
 *
 * 票 2026-10-10 批次 C2：工具箱页 / 底部导航 Tab / 终端工具栏项 / 终端主视图
 * 四个旧嵌入扩展点已随宿主壳改纯 surface 形态整面退役，dev-shell 与宿主同口径——
 * 否则插件在 dev-shell 里能跑通、在真机宿主上却报错，两种形态必须一致。
 */
import { ref } from 'vue'
import type {
  Disposable,
  PluginDevMock,
  PluginRouteDescriptor,
  ShellCapsuleItem,
  ShellSettingsEntry,
  ShellSlotContribution,
  ShellSurfaceContribution,
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
  manifest: Record<string, unknown>
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

export interface RouteEntry {
  pluginId: string
  route: PluginRouteDescriptor
  /** router 路由名（registerRoute 时 addRoute，dispose 时 removeRoute） */
  routeName: string
}
// 壳注册桥四面（与宿主 context.ts / types.ts 同形状；dev-shell 无壳 UI，注册只记录 + 回收，
// 供 dev-shell 后续按需渲染与插件开发自检）
export interface SurfaceEntry {
  pluginId: string
  surface: ShellSurfaceContribution
}
export interface SlotEntry {
  pluginId: string
  slot: ShellSlotContribution
}
export interface CapsuleEntry {
  pluginId: string
  item: ShellCapsuleItem
}
export interface SettingsEntry {
  pluginId: string
  entry: ShellSettingsEntry
}
const routes = ref<RouteEntry[]>([])
const surfaces = ref<SurfaceEntry[]>([])
const slots = ref<SlotEntry[]>([])
const capsules = ref<CapsuleEntry[]>([])
const settingsEntries = ref<SettingsEntry[]>([])

/** 从列表中移除条目（dispose 回调） */
function makeDisposable<T>(list: { value: T[] }, entry: T): Disposable {
  return {
    dispose() {
      const idx = list.value.indexOf(entry)
      if (idx !== -1) list.value.splice(idx, 1)
    },
  }
}

export function registerSurface(pluginId: string, surface: ShellSurfaceContribution): Disposable {
  const entry: SurfaceEntry = { pluginId, surface }
  surfaces.value.push(entry)
  // ShellSurfaceContribution 无 id（只有 component + 可选 accent），日志用 pluginId 区分
  pushLog('debug', pluginId, '注册壳运行面')
  return makeDisposable(surfaces, entry)
}

export function registerSlot(pluginId: string, slot: ShellSlotContribution): Disposable {
  const entry: SlotEntry = { pluginId, slot }
  slots.value.push(entry)
  pushLog('debug', pluginId, `注册壳快捷卡片: ${slot.id}`)
  return makeDisposable(slots, entry)
}

export function registerCapsuleItem(pluginId: string, item: ShellCapsuleItem): Disposable {
  const entry: CapsuleEntry = { pluginId, item }
  capsules.value.push(entry)
  pushLog('debug', pluginId, `注册胶囊菜单项: ${item.label}`)
  return makeDisposable(capsules, entry)
}

export function registerSettingsEntry(pluginId: string, entry: ShellSettingsEntry): Disposable {
  const registered: SettingsEntry = { pluginId, entry }
  settingsEntries.value.push(registered)
  pushLog('debug', pluginId, `注册设置入口: ${entry.id}`)
  return makeDisposable(settingsEntries, registered)
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

// ==================== 当前打开的插件视图（视图栈，与宿主路由栈语义一致） ====================

export interface ActiveView {
  kind: 'surface' | 'settings' | 'route'
  pluginId: string
  title?: string
  /** 是否渲染宿主页头（back + title），缺省 true */
  header?: boolean
  component: any
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

export { activeView, logs, plugins, routes, surfaces, slots, capsules, settingsEntries }
