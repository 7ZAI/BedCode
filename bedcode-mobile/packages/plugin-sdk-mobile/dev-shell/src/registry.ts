/**
 * Dev Shell 调试注册表
 * -----------------------------------------------------------------------------
 * 模块级响应式状态（跨组件共享单例）：
 * - 调试记录（内置应用 + 被调试插件：state / error / context）
 * - 插件动态路由注册项（registerRoute 的真源，渲染由 vue-router 承载）
 * - 日志面板数据
 *
 * 与宿主 `src/plugin/registry.ts` 的分工：宿主那份存「插件 UI 扩展点」，dev-shell
 * 原来也照抄了一份（surfaces / slots / capsules / settingsEntries + activeView 视图栈）。
 * 票 2026-10-10 起宿主壳只认 registerSurface 一种运行面形态，且壳内导航是屏幕栈而非
 * 视图栈——那份副本已整面退役：
 *   · 界面贡献 → 改由 `src/shell/registry.ts`（壳注册表）持有，与宿主同构
 *   · 视图栈 → 改由 `src/shell/composables/useShellNavigation.ts`（屏幕栈）承担
 * 保留在这里的只有「调试对象」自身的事实与插件路由注册表。
 */
import { ref } from 'vue'
import type { Disposable, PluginDevMock, PluginRouteDescriptor } from '../../src/types'

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

// ==================== 调试记录（壳应用的数据源） ====================

export type DevPluginState = 'loaded' | 'activated' | 'deactivated' | 'error'

export interface DevPluginRecord {
  id: string
  name: string
  /** plugin.json 原文（内置应用给等价的静态元信息） */
  manifest: Record<string, unknown>
  /** 入口模块（内置应用为 null） */
  entry: any
  state: DevPluginState
  error?: string
  context: any
  /** devMock 注册句柄（deactivate 时清理） */
  devMockDisposable?: Disposable
  /** 内置应用标记（dev-shell 自带、非被调试插件）：壳里显示「官方」徽标 */
  builtin?: boolean
}

const plugins = ref<DevPluginRecord[]>([])

export function getPluginRecord(pluginId: string): DevPluginRecord | undefined {
  return plugins.value.find((p) => p.id === pluginId)
}

// ==================== 插件动态路由（渲染承载在 vue-router） ====================

/**
 * 已注册路由条目
 *
 * 与宿主 `src/plugin/routes.ts` 同构：注册时既写这里（解析真源），也在 vue-router
 * 上 addRoute（页面承载）；dispose 双向撤销。渲染组件见 `shell/components/screens/
 * PluginRoutePage.vue`。
 */
export interface RouteEntry {
  pluginId: string
  route: PluginRouteDescriptor
  /** router 路由名（registerRoute 时 addRoute，dispose 时 removeRoute） */
  routeName: string
}

const routes = ref<RouteEntry[]>([])

/** 登记路由条目（router 侧由调用方 addRoute 后登记；dispose = 撤销登记） */
export function registerRouteEntry(entry: RouteEntry): Disposable {
  routes.value.push(entry)
  return {
    dispose() {
      const idx = routes.value.indexOf(entry)
      if (idx !== -1) routes.value.splice(idx, 1)
    },
  }
}

/** 路由名（守卫与跳转共用，避免字符串漂移） */
export function pluginRouteName(pluginId: string, routeId: string): string {
  return `dev-plugin-route-${pluginId}-${routeId}`
}

/** 取指定插件的路由条目 */
export function findRoute(pluginId: string, routeId: string): RouteEntry | undefined {
  return routes.value.find((r) => r.pluginId === pluginId && r.route.id === routeId)
}

/** 取指定插件的全部路由 */
export function routesOf(pluginId: string): RouteEntry[] {
  return routes.value.filter((r) => r.pluginId === pluginId)
}

export { logs, plugins, routes }