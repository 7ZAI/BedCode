/**
 * Plugin Route Host
 *
 * 插件动态路由扩展点宿主实现：
 *   - registerPluginRoute：router.addRoute 注册动态路由（/mobile/plugins/{pluginId}/{routeId}），
 *     组件统一为 PluginRouteView（按 meta.pluginRoute 解析插件组件并提供 pluginContext）
 *   - openPluginRoute：整体跳转到插件已注册路由（插件决定走整体路由而非组件内切换）
 *   - 撤销：Disposable.dispose / registry.clearPlugin 经 removeRoute 摘除
 *
 * 不静态 import router，经 window.__BEDCODE_SHARED__.router 获取实例，
 * 避免 router → loader → context → routes → router 的循环依赖。
 */
import type { Disposable, PluginRouteDescriptor } from './types'
import { getSharedModule } from './shared-runtime'
import { getPluginRegistry } from './registry'

/** 插件路由 name（守卫与跳转共用，避免字符串漂移） */
export function pluginRouteName(pluginId: string, routeId: string): string {
  return `plugin-route-${pluginId}-${routeId}`
}

/** 注册插件动态路由；Disposable.dispose = removeRoute + 注册表清理 */
export function registerPluginRoute(pluginId: string, descriptor: PluginRouteDescriptor): Disposable {
  const header = descriptor.header ?? true
  const router: any = getSharedModule('router')
  const removeRoute = router.addRoute({
    path: `/mobile/plugins/${pluginId}/${descriptor.id}`,
    name: pluginRouteName(pluginId, descriptor.id),
    meta: {
      standAlone: true,
      pluginRoute: { pluginId, routeId: descriptor.id, title: descriptor.title, header },
    },
    // 动态 import：组件随主 bundle 按需加载
    component: () => import('@/plugin/components/PluginRouteView.vue'),
  })

  const rec = getPluginRegistry().registerPluginRoute(pluginId, {
    id: descriptor.id,
    title: descriptor.title,
    header,
    component: descriptor.component,
  })
  rec.removeRoute = removeRoute

  return {
    dispose() {
      removeRoute()
      getPluginRegistry().unregisterPluginRoute(pluginId, descriptor.id)
    },
  }
}

/** 整体跳转到插件已注册路由（返回入口页用 goBack） */
export function openPluginRoute(pluginId: string, routeId: string): void {
  const router: any = getSharedModule('router')
  router.push({ name: pluginRouteName(pluginId, routeId) })
}
