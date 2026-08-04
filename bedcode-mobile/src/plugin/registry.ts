/**
 * Plugin Registry
 *
 * 前端扩展点注册表 — 管理插件注册的 Vue 组件
 * 响应式数据供宿主 UI 组件消费
 */

import type { Disposable, PluginContext, ToolboxPageDescriptor, NavTabDescriptor, TerminalToolbarItemDescriptor, SettingsSectionDescriptor } from './types'
import { ref, type Ref } from 'vue'

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

/** 前端插件注册表 */
class PluginRegistryClass {
  private toolboxViewsMap = new Map<string, RegisteredToolboxView>()
  private navTabsMap = new Map<string, RegisteredNavTab>()
  private terminalToolbarMap = new Map<string, RegisteredTerminalToolbarItem>()
  private settingsSectionsMap = new Map<string, RegisteredSettingsSection>()
  private contexts = new Map<string, PluginContext>()

  /** 响应式数据供 Vue 组件使用 */
  readonly toolboxViews: Ref<RegisteredToolboxView[]> = ref([])
  readonly navTabs: Ref<RegisteredNavTab[]> = ref([])
  readonly terminalToolbarItems: Ref<RegisteredTerminalToolbarItem[]> = ref([])
  readonly settingsSections: Ref<RegisteredSettingsSection[]> = ref([])

  /** 注册工具箱页面 */
  registerToolboxPage(pluginId: string, page: ToolboxPageDescriptor): Disposable {
    const key = `${pluginId}:${page.id}`
    this.toolboxViewsMap.set(key, {
      pluginId,
      viewId: page.id,
      title: page.title,
      icon: page.icon,
      component: page.component,
      entry: page.entry,
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
      component: tab.component,
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
      component: section.component,
    })
    this.updateReactiveSettingsSections()
    return {
      dispose: () => {
        this.settingsSectionsMap.delete(key)
        this.updateReactiveSettingsSections()
      },
    }
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
}

/** 全局单例 */
const registry = new PluginRegistryClass()

/** 获取全局注册表 */
export function getPluginRegistry(): PluginRegistryClass {
  return registry
}
