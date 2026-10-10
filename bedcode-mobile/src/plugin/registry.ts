/**
 * Plugin Registry
 *
 * 前端扩展点注册表 — 管理插件注册的 Vue 组件
 * 响应式数据供宿主 UI 组件消费
 *
 * 票 2026-10-10 批次 C2：工具箱页 / 导航 Tab / 终端工具栏项 / 终端主视图四个
 * 旧嵌入扩展点的存储与读写面整面退役（宿主壳只认 registerSurface 一种运行面形态）。
 * 本表此后只承载两件事：插件上下文（PluginContext）+ 插件动态路由。
 * 防回接锁见 src/__tests__/shell/retiredHostUIRetirementLocks.test.ts（R4）。
 */

import type { Disposable, PluginContext, PluginRouteDescriptor } from './types'
import { ref, markRaw, type Ref } from 'vue'

/** 注册的插件路由（宿主 addRoute 至 /mobile/plugins/{pluginId}/{routeId}） */
interface RegisteredPluginRoute {
  pluginId: string
  routeId: string
  title?: string
  header: boolean
  component: any
  /** vue-router removeRoute 闭包（由 route-host 注入），clearPlugin/Disposable 时摘除动态路由 */
  removeRoute?: () => void
}

/** 前端插件注册表 */
class PluginRegistryClass {
  private routesMap = new Map<string, RegisteredPluginRoute>()
  private contexts = new Map<string, PluginContext>()

  /** 响应式数据供 Vue 组件使用 */
  readonly routes: Ref<RegisteredPluginRoute[]> = ref([])

  /** 注册插件路由（路由表由 route-host addRoute，此处仅存记录；返回记录供注入 removeRoute） */
  registerPluginRoute(pluginId: string, route: PluginRouteDescriptor): RegisteredPluginRoute {
    const key = `${pluginId}:${route.id}`
    const rec: RegisteredPluginRoute = {
      pluginId,
      routeId: route.id,
      title: route.title,
      header: route.header ?? true,
      component: markRaw(route.component),
    }
    this.routesMap.set(key, rec)
    this.updateReactiveRoutes()
    return rec
  }

  /** 撤销插件路由记录（动态路由摘除由调用方负责 removeRoute） */
  unregisterPluginRoute(pluginId: string, routeId: string): void {
    this.routesMap.delete(`${pluginId}:${routeId}`)
    this.updateReactiveRoutes()
  }

  /** 获取插件路由记录 */
  getPluginRoute(pluginId: string, routeId: string): RegisteredPluginRoute | undefined {
    return this.routesMap.get(`${pluginId}:${routeId}`)
  }

  /** 存储插件上下文 */
  setContext(pluginId: string, context: PluginContext): void {
    this.contexts.set(pluginId, context)
  }

  /** 获取插件上下文 */
  getContext(pluginId: string): PluginContext | undefined {
    return this.contexts.get(pluginId)
  }

  /** 清理插件的所有注册 */
  clearPlugin(pluginId: string): void {
    this.contexts.delete(pluginId)

    // 插件路由：摘除宿主动态路由（removeRoute）并清理记录
    for (const [key, rec] of [...this.routesMap.entries()]) {
      if (key.startsWith(`${pluginId}:`)) {
        rec.removeRoute?.()
        this.routesMap.delete(key)
      }
    }
    this.updateReactiveRoutes()
  }

  private updateReactiveRoutes() {
    this.routes.value = [...this.routesMap.values()]
  }
}

/** 全局单例 */
const registry = new PluginRegistryClass()

/** 获取全局注册表 */
export function getPluginRegistry(): PluginRegistryClass {
  return registry
}
