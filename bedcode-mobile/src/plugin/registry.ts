/**
 * Plugin Registry
 *
 * 前端扩展点注册表 — 管理插件注册的 Vue 组件
 * 响应式数据供宿主 UI 组件消费
 */

import type { Disposable, PluginContext, ToolboxPageDescriptor, NavTabDescriptor, TerminalToolbarItemDescriptor, SettingsSectionDescriptor, PluginRouteDescriptor } from './types'
import { ref, markRaw, type Ref } from 'vue'

/** 注册的工具箱视图 */
interface RegisteredToolboxView {
  pluginId: string
  viewId: string
  title: string
  /** 入口图标：emoji 或 SVG path d 字符串，缺省 🧩 */
  icon?: string
  component: any
  /** 插件自定义入口卡片组件（缺省时宿主用统一卡片渲染） */
  entry?: any
}

/** 注册的导航 Tab */
interface RegisteredNavTab {
  pluginId: string
  id: string
  title: string
  icon: string
  component: any
  order: number
}

/** 注册的终端工具栏项 */
interface RegisteredTerminalToolbarItem {
  pluginId: string
  id: string
  label: string
  icon?: string
  onClick?: () => void
}

/** 注册的设置区域 */
interface RegisteredSettingsSection {
  pluginId: string
  id: string
  section: string
  component: any
}

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
  private toolboxViewsMap = new Map<string, RegisteredToolboxView>()
  private navTabsMap = new Map<string, RegisteredNavTab>()
  private terminalToolbarMap = new Map<string, RegisteredTerminalToolbarItem>()
  private settingsSectionsMap = new Map<string, RegisteredSettingsSection>()
  private routesMap = new Map<string, RegisteredPluginRoute>()
  private contexts = new Map<string, PluginContext>()

  /** 响应式数据供 Vue 组件使用 */
  readonly toolboxViews: Ref<RegisteredToolboxView[]> = ref([])
  readonly navTabs: Ref<RegisteredNavTab[]> = ref([])
  readonly terminalToolbarItems: Ref<RegisteredTerminalToolbarItem[]> = ref([])
  readonly settingsSections: Ref<RegisteredSettingsSection[]> = ref([])
  readonly routes: Ref<RegisteredPluginRoute[]> = ref([])

  /** 注册工具箱页面 */
  registerToolboxPage(pluginId: string, page: ToolboxPageDescriptor): Disposable {
    const key = `${pluginId}:${page.id}`
    this.toolboxViewsMap.set(key, {
      pluginId,
      viewId: page.id,
      title: page.title,
      icon: page.icon,
      // markRaw：组件进入响应式数组会被 Vue 深代理，渲染时触发
      // "Component was made a reactive object" 警告且增加无谓开销
      component: markRaw(page.component),
      // entry 为可选字段（缺省时宿主用统一卡片渲染，见 ToolboxPageDescriptor）：
      // markRaw(undefined) 会抛 TypeError: Cannot convert undefined or null to object
      entry: page.entry ? markRaw(page.entry) : undefined,
    })
    this.updateReactiveToolboxViews()
    return {
      dispose: () => {
        this.toolboxViewsMap.delete(key)
        this.updateReactiveToolboxViews()
      },
    }
  }

  /** 注册导航 Tab */
  registerNavTab(pluginId: string, tab: NavTabDescriptor): Disposable {
    const key = `${pluginId}:${tab.id}`
    this.navTabsMap.set(key, {
      pluginId,
      id: tab.id,
      title: tab.title,
      icon: tab.icon,
      component: markRaw(tab.component),
      order: tab.order,
    })
    this.updateReactiveNavTabs()
    return {
      dispose: () => {
        this.navTabsMap.delete(key)
        this.updateReactiveNavTabs()
      },
    }
  }

  /** 注册终端工具栏项 */
  registerTerminalToolbarItem(pluginId: string, item: TerminalToolbarItemDescriptor): Disposable {
    const key = `${pluginId}:${item.id}`
    this.terminalToolbarMap.set(key, {
      pluginId,
      id: item.id,
      label: item.label,
      icon: item.icon,
      onClick: item.onClick,
    })
    this.updateReactiveTerminalToolbar()
    return {
      dispose: () => {
        this.terminalToolbarMap.delete(key)
        this.updateReactiveTerminalToolbar()
      },
    }
  }

  /** 注册设置区域 */
  registerSettingsSection(pluginId: string, section: SettingsSectionDescriptor): Disposable {
    const key = `${pluginId}:${section.id}`
    this.settingsSectionsMap.set(key, {
      pluginId,
      id: section.id,
      section: section.section,
      component: markRaw(section.component),
    })
    this.updateReactiveSettingsSections()
    return {
      dispose: () => {
        this.settingsSectionsMap.delete(key)
        this.updateReactiveSettingsSections()
      },
    }
  }

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

  /** 获取工具箱视图组件 */
  getToolboxViewComponent(pluginId: string, viewId: string): any {
    return this.toolboxViewsMap.get(`${pluginId}:${viewId}`)?.component
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

    for (const key of [...this.toolboxViewsMap.keys()]) {
      if (key.startsWith(`${pluginId}:`)) this.toolboxViewsMap.delete(key)
    }
    this.updateReactiveToolboxViews()

    for (const key of [...this.navTabsMap.keys()]) {
      if (key.startsWith(`${pluginId}:`)) this.navTabsMap.delete(key)
    }
    this.updateReactiveNavTabs()

    for (const key of [...this.terminalToolbarMap.keys()]) {
      if (key.startsWith(`${pluginId}:`)) this.terminalToolbarMap.delete(key)
    }
    this.updateReactiveTerminalToolbar()

    for (const key of [...this.settingsSectionsMap.keys()]) {
      if (key.startsWith(`${pluginId}:`)) this.settingsSectionsMap.delete(key)
    }
    this.updateReactiveSettingsSections()

    // 插件路由：摘除宿主动态路由（removeRoute）并清理记录
    for (const [key, rec] of [...this.routesMap.entries()]) {
      if (key.startsWith(`${pluginId}:`)) {
        rec.removeRoute?.()
        this.routesMap.delete(key)
      }
    }
    this.updateReactiveRoutes()
  }

  private updateReactiveToolboxViews() {
    this.toolboxViews.value = [...this.toolboxViewsMap.values()]
  }

  private updateReactiveNavTabs() {
    const tabs = [...this.navTabsMap.values()]
    tabs.sort((a, b) => a.order - b.order)
    this.navTabs.value = tabs
  }

  private updateReactiveTerminalToolbar() {
    this.terminalToolbarItems.value = [...this.terminalToolbarMap.values()]
  }

  private updateReactiveSettingsSections() {
    this.settingsSections.value = [...this.settingsSectionsMap.values()]
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
