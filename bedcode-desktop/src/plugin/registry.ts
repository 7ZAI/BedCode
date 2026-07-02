/**
 * Plugin Registry
 *
 * 前端扩展点注册表 — 管理插件注册的 Vue 组件、命令处理器和文件处理器
 */

import type { Disposable, PluginContext } from './types'
import { ref, type Ref } from 'vue'

/** 注册的视图组件 */
interface RegisteredView {
  pluginId: string
  viewId: string
  viewType: string
  title: string
  component: any
}

/** 注册的状态栏项 */
interface RegisteredStatusBarItem {
  pluginId: string
  id: string
  label: string
  icon?: string
  onClick?: () => void
}

/** 注册的输入扩展 */
interface RegisteredInputExtension {
  pluginId: string
  id: string
  label: string
  icon?: string
  onActivate?: () => void
}

/** 注册的终端工具栏项 */
interface RegisteredTerminalToolbarItem {
  pluginId: string
  id: string
  label: string
  icon?: string
  onClick?: () => void
}

/** 注册的标题栏项 */
interface RegisteredTitleBarItem {
  pluginId: string
  id: string
  label: string
  icon?: string
  onClick?: () => void
}

/** 注册的文件处理器 */
interface RegisteredFileHandler {
  pluginId: string
  id: string
  extensions: string[]
  component: any
}

/** 前端插件注册表 */
class PluginRegistryClass {
  private views = new Map<string, RegisteredView>()
  private statusBarItem = new Map<string, RegisteredStatusBarItem>()
  private inputExtensions = new Map<string, RegisteredInputExtension>()
  private terminalToolbarItemsMap = new Map<string, RegisteredTerminalToolbarItem>()
  private titleBarItemsMap = new Map<string, RegisteredTitleBarItem>()
  private fileHandlers = new Map<string, RegisteredFileHandler>()
  /** 插件上下文映射，供 PluginViewHost provide 给组件树 */
  private contexts = new Map<string, PluginContext>()

  /** 响应式数据供 Vue 组件使用 */
  readonly sidebarViews: Ref<RegisteredView[]> = ref([])
  readonly toolboxViews: Ref<RegisteredView[]> = ref([])
  readonly statusbarViews: Ref<RegisteredView[]> = ref([])
  readonly statusbarItems: Ref<RegisteredStatusBarItem[]> = ref([])
  readonly inputExts: Ref<RegisteredInputExtension[]> = ref([])
  readonly terminalToolbarItems: Ref<RegisteredTerminalToolbarItem[]> = ref([])
  readonly titleBarItems: Ref<RegisteredTitleBarItem[]> = ref([])

  /** 注册视图 */
  registerView(pluginId: string, viewType: string, panel: { id: string; title: string; component: any }): Disposable {
    const key = `${pluginId}:${panel.id}`
    const entry: RegisteredView = {
      pluginId,
      viewId: panel.id,
      viewType,
      title: panel.title,
      component: panel.component,
    }
    this.views.set(key, entry)
    this.updateReactiveViews()
    return {
      dispose: () => {
        this.views.delete(key)
        this.updateReactiveViews()
      },
    }
  }

  /** 获取视图组件 */
  getViewComponent(pluginId: string, viewId: string): any {
    return this.views.get(`${pluginId}:${viewId}`)?.component
  }

  /** 注册状态栏项 */
  registerStatusBarItem(pluginId: string, item: { id: string; label: string; icon?: string; onClick?: () => void }): Disposable {
    const key = `${pluginId}:${item.id}`
    const entry: RegisteredStatusBarItem = {
      pluginId,
      id: item.id,
      label: item.label,
      icon: item.icon,
      onClick: item.onClick,
    }
    this.statusBarItem.set(key, entry)
    this.updateReactiveStatusBar()
    return {
      dispose: () => {
        this.statusBarItem.delete(key)
        this.updateReactiveStatusBar()
      },
    }
  }

  /** 注册输入扩展 */
  registerInputExtension(pluginId: string, ext: { id: string; label: string; icon?: string; onActivate?: () => void }): Disposable {
    const key = `${pluginId}:${ext.id}`
    const entry: RegisteredInputExtension = {
      pluginId,
      id: ext.id,
      label: ext.label,
      icon: ext.icon,
      onActivate: ext.onActivate,
    }
    this.inputExtensions.set(key, entry)
    this.updateReactiveInputExts()
    return {
      dispose: () => {
        this.inputExtensions.delete(key)
        this.updateReactiveInputExts()
      },
    }
  }

  /** 注册终端工具栏项 */
  registerTerminalToolbarItem(pluginId: string, item: { id: string; label: string; icon?: string; onClick?: () => void }): Disposable {
    const key = `${pluginId}:${item.id}`
    const entry: RegisteredTerminalToolbarItem = {
      pluginId,
      id: item.id,
      label: item.label,
      icon: item.icon,
      onClick: item.onClick,
    }
    this.terminalToolbarItemsMap.set(key, entry)
    this.updateReactiveTerminalToolbar()
    return {
      dispose: () => {
        this.terminalToolbarItemsMap.delete(key)
        this.updateReactiveTerminalToolbar()
      },
    }
  }

  /** 注册标题栏项 */
  registerTitleBarItem(pluginId: string, item: { id: string; label: string; icon?: string; onClick?: () => void }): Disposable {
    const key = `${pluginId}:${item.id}`
    const entry: RegisteredTitleBarItem = {
      pluginId,
      id: item.id,
      label: item.label,
      icon: item.icon,
      onClick: item.onClick,
    }
    this.titleBarItemsMap.set(key, entry)
    this.updateReactiveTitleBarItems()
    return {
      dispose: () => {
        this.titleBarItemsMap.delete(key)
        this.updateReactiveTitleBarItems()
      },
    }
  }

  /** 注册文件处理器 */
  registerFileHandler(pluginId: string, handler: { id: string; extensions: string[]; component: any }): Disposable {
    const key = `${pluginId}:${handler.id}`
    const entry: RegisteredFileHandler = {
      pluginId,
      id: handler.id,
      extensions: handler.extensions,
      component: handler.component,
    }
    this.fileHandlers.set(key, entry)
    return {
      dispose: () => {
        this.fileHandlers.delete(key)
      },
    }
  }

  /** 查找文件处理器 */
  findFileHandler(extension: string): RegisteredFileHandler | undefined {
    for (const handler of this.fileHandlers.values()) {
      if (handler.extensions.includes(extension)) {
        return handler
      }
    }
    return undefined
  }

  /** 存储插件上下文（激活时调用） */
  setContext(pluginId: string, context: PluginContext): void {
    this.contexts.set(pluginId, context)
  }

  /** 获取插件上下文（PluginViewHost 使用） */
  getContext(pluginId: string): PluginContext | undefined {
    return this.contexts.get(pluginId)
  }

  /** 清理插件的所有注册 */
  clearPlugin(pluginId: string): void {
    this.contexts.delete(pluginId)

    for (const key of [...this.views.keys()]) {
      if (key.startsWith(`${pluginId}:`)) {
        this.views.delete(key)
      }
    }
    this.updateReactiveViews()

    for (const key of [...this.statusBarItem.keys()]) {
      if (key.startsWith(`${pluginId}:`)) {
        this.statusBarItem.delete(key)
      }
    }
    this.updateReactiveStatusBar()

    for (const key of [...this.inputExtensions.keys()]) {
      if (key.startsWith(`${pluginId}:`)) {
        this.inputExtensions.delete(key)
      }
    }
    this.updateReactiveInputExts()

    for (const key of [...this.terminalToolbarItemsMap.keys()]) {
      if (key.startsWith(`${pluginId}:`)) {
        this.terminalToolbarItemsMap.delete(key)
      }
    }
    this.updateReactiveTerminalToolbar()

    for (const key of [...this.titleBarItemsMap.keys()]) {
      if (key.startsWith(`${pluginId}:`)) {
        this.titleBarItemsMap.delete(key)
      }
    }
    this.updateReactiveTitleBarItems()

    for (const key of [...this.fileHandlers.keys()]) {
      if (key.startsWith(`${pluginId}:`)) {
        this.fileHandlers.delete(key)
      }
    }
  }

  private updateReactiveViews() {
    const views = [...this.views.values()]
    this.sidebarViews.value = views.filter(v => v.viewType === 'sidebar')
    this.toolboxViews.value = views.filter(v => v.viewType === 'toolbox')
    this.statusbarViews.value = views.filter(v => v.viewType === 'statusbar')
  }

  private updateReactiveStatusBar() {
    this.statusbarItems.value = [...this.statusBarItem.values()]
  }

  private updateReactiveInputExts() {
    this.inputExts.value = [...this.inputExtensions.values()]
  }

  private updateReactiveTerminalToolbar() {
    this.terminalToolbarItems.value = [...this.terminalToolbarItemsMap.values()]
  }

  private updateReactiveTitleBarItems() {
    this.titleBarItems.value = [...this.titleBarItemsMap.values()]
  }
}

/** 全局单例 */
const registry = new PluginRegistryClass()

/** 获取全局注册表 */
export function getPluginRegistry(): PluginRegistryClass {
  return registry
}
