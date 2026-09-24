/**
 * Plugin Registry
 *
 * 前端扩展点注册表 — 管理插件注册的 Vue 组件、命令处理器和文件处理器
 */

import type { Disposable, PluginContext, PluginState } from './types'
import { ref, shallowRef, type Ref, type ShallowRef } from 'vue'

/** 插件视图默认排序值 — 插件未指定 order 时使用。
 * 600 位于全部内置菜单项（设备 100 / 会话 200 / 服务器 300 保留 / 插件 9998 / 设置 9999）之后，
 * 插件菜单默认排在宿主内置菜单之后（设置之前）；显式指定 order 可插入任意内置项之间。
 * 设置分组贡献面复用同一缺省值（缺省行为与既有贡献面一致） */
const DEFAULT_VIEW_ORDER = 600

/** 插件贡献「生效」的运行态：Activated 正常，Degraded 实例仍在运行（仅启动初始化未完成）。
 * Activating / Loaded / NeedsApproval 尚未注册扩展点；Error / Deactivated 一律视为未生效 */
const ACTIVE_CONTRIBUTION_STATES: ReadonlySet<PluginState['state']> = new Set([
  'Activated',
  'Degraded',
])

/**
 * 判断插件运行态是否使其贡献面生效 —— 宿主「贡献项摘除 / 恢复」的单一判据。
 *
 * 纯函数形态供消费方在响应式依赖（registry.pluginStatesRef）下调用；
 * 注册表方法 `isContributionActive()` 与之共用同一状态集合，杜绝同一事实各判一次。
 */
export function isContributionActiveState(state: PluginState | undefined): boolean {
  return state ? ACTIVE_CONTRIBUTION_STATES.has(state.state) : false
}

/** 注册的视图组件 */
interface RegisteredView {
  pluginId: string
  viewId: string
  viewType: string
  title: string
  icon?: string
  /** 排序值，升序排列（越小越靠前），同值保持注册顺序 */
  order: number
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

/** 注册的页面工具栏项（注入到指定页面的工具栏页头） */
interface RegisteredPageToolbarItem {
  pluginId: string
  id: string
  /** 目标页面标识：sessions / devices / history / plugins / plugin-config / server / settings / terminal */
  target: string
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

/** 注册的设置分组（插件贡献到宿主设置页） */
interface RegisteredSettingsSection {
  pluginId: string
  id: string
  titleKey: string
  icon?: string
  /** 排序值，升序排列（越小越靠前），同值保持注册顺序 */
  order: number
  component: any
}

/** 注册的 HTTP 端点 handler */
interface RegisteredHttpEndpoint {
  pluginId: string
  path: string
  handler: (req: {
    method: string
    path: string
    body: any
    headers: Record<string, string>
  }) => Promise<{ status: number; body: any }>
}

/** 前端插件注册表 */
class PluginRegistryClass {
  private views = new Map<string, RegisteredView>()
  /**
   * 全量视图（含 `page` 型，未经菜单投影）的响应式副本 —— `getViewComponent` 的依赖源。
   *
   * 为什么必须有：`views` Map 本身不是响应式数据，若 `getViewComponent` 直接按键读 Map，
   * 消费方的 `computed` 建立不了任何依赖——「组件挂载时视图尚未注册」得到的 undefined
   * 会被永久缓存，插件随后注册也不可见。独立窗口（如终端窗口）的深链路由正是在
   * `loadAll` 完成前就挂载了视图宿主，表现即白屏 + 「插件视图未找到」（2026-09-24）。
   */
  private readonly viewsIndex: ShallowRef<RegisteredView[]> = shallowRef([])
  private statusBarItem = new Map<string, RegisteredStatusBarItem>()
  private inputExtensions = new Map<string, RegisteredInputExtension>()
  private terminalToolbarItemsMap = new Map<string, RegisteredTerminalToolbarItem>()
  private titleBarItemsMap = new Map<string, RegisteredTitleBarItem>()
  private pageToolbarItemsMap = new Map<string, RegisteredPageToolbarItem>()
  private fileHandlers = new Map<string, RegisteredFileHandler>()
  private httpEndpoints = new Map<string, RegisteredHttpEndpoint>()
  private settingsSections = new Map<string, RegisteredSettingsSection>()
  /** 插件上下文映射，供 PluginViewHost provide 给组件树 */
  private contexts = new Map<string, PluginContext>()
  /** 插件运行态映射 — 贡献面「是否生效」的唯一事实源（宿主摘除/恢复判据，见 isContributionActive） */
  private pluginStates = new Map<string, PluginState>()

  /** 响应式数据供 Vue 组件使用 */
  readonly sidebarViews: Ref<RegisteredView[]> = ref([])
  // shallowRef：条目整体替换即触发更新，避免深响应把 component 包成响应式对象
  // （Vue 会警告「Component that was made a reactive object」）
  readonly settingsSectionsRef: Ref<RegisteredSettingsSection[]> = shallowRef([])
  /** 插件运行态响应式副本（贡献面消费方据此重算） */
  readonly pluginStatesRef: Ref<Record<string, PluginState>> = ref({})
  readonly toolboxViews: Ref<RegisteredView[]> = ref([])
  readonly statusbarItems: Ref<RegisteredStatusBarItem[]> = ref([])
  readonly inputExts: Ref<RegisteredInputExtension[]> = ref([])
  readonly terminalToolbarItems: Ref<RegisteredTerminalToolbarItem[]> = ref([])
  readonly titleBarItems: Ref<RegisteredTitleBarItem[]> = ref([])
  readonly pageToolbarItems: Ref<RegisteredPageToolbarItem[]> = ref([])

  /** 注册视图 */
  registerView(
    pluginId: string,
    viewType: string,
    panel: { id: string; title: string; icon?: string; order?: number; component: any },
  ): Disposable {
    const key = `${pluginId}:${panel.id}`
    const entry: RegisteredView = {
      pluginId,
      viewId: panel.id,
      viewType,
      title: panel.title,
      icon: panel.icon,
      order: panel.order ?? DEFAULT_VIEW_ORDER,
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

  /**
   * 获取视图组件 —— 带响应式语义
   *
   * 读 `viewsIndex` 而非裸 Map：注册 / 注销 / 清插件都会替换该投影，
   * 使消费方（PluginViewHost 的 computed）在「晚注册」与「插件停用」时重算。
   */
  getViewComponent(pluginId: string, viewId: string): any {
    const hit = this.viewsIndex.value.find((v) => v.pluginId === pluginId && v.viewId === viewId)
    return hit?.component
  }

  /** 注册状态栏项 */
  registerStatusBarItem(
    pluginId: string,
    item: { id: string; label: string; icon?: string; onClick?: () => void },
  ): Disposable {
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
  registerInputExtension(
    pluginId: string,
    ext: { id: string; label: string; icon?: string; onActivate?: () => void },
  ): Disposable {
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
  registerTerminalToolbarItem(
    pluginId: string,
    item: { id: string; label: string; icon?: string; onClick?: () => void },
  ): Disposable {
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
  registerTitleBarItem(
    pluginId: string,
    item: { id: string; label: string; icon?: string; onClick?: () => void },
  ): Disposable {
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

  /** 注册页面工具栏项 */
  registerPageToolbarItem(
    pluginId: string,
    item: { target: string; id: string; label: string; icon?: string; onClick?: () => void },
  ): Disposable {
    const key = `${item.target}:${pluginId}:${item.id}`
    const entry: RegisteredPageToolbarItem = {
      pluginId,
      id: item.id,
      target: item.target,
      label: item.label,
      icon: item.icon,
      onClick: item.onClick,
    }
    this.pageToolbarItemsMap.set(key, entry)
    this.updateReactivePageToolbar()
    return {
      dispose: () => {
        this.pageToolbarItemsMap.delete(key)
        this.updateReactivePageToolbar()
      },
    }
  }

  /** 注册文件处理器 */
  registerFileHandler(
    pluginId: string,
    handler: { id: string; extensions: string[]; component: any },
  ): Disposable {
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

  /** 注册设置分组（插件贡献到宿主设置页，需 ui:settings 权限） */
  registerSettingsSection(
    pluginId: string,
    section: { id: string; titleKey: string; icon?: string; order?: number; component: any },
  ): Disposable {
    const key = `${pluginId}:${section.id}`
    const entry: RegisteredSettingsSection = {
      pluginId,
      id: section.id,
      titleKey: section.titleKey,
      icon: section.icon,
      order: section.order ?? DEFAULT_VIEW_ORDER,
      component: section.component,
    }
    this.settingsSections.set(key, entry)
    this.updateReactiveSettingsSections()
    return {
      dispose: () => {
        this.settingsSections.delete(key)
        this.updateReactiveSettingsSections()
      },
    }
  }

  /** 注册 HTTP 端点 handler */
  registerHttpEndpoint(
    pluginId: string,
    path: string,
    handler: RegisteredHttpEndpoint['handler'],
  ): Disposable {
    const key = `${pluginId}:${path}`
    this.httpEndpoints.set(key, { pluginId, path, handler })
    return {
      dispose: () => {
        this.httpEndpoints.delete(key)
      },
    }
  }

  /** 查找 HTTP 端点 handler */
  findHttpEndpoint(pluginId: string, path: string): RegisteredHttpEndpoint | undefined {
    return this.httpEndpoints.get(`${pluginId}:${path}`)
  }

  /** 存储插件上下文（激活时调用） */
  setContext(pluginId: string, context: PluginContext): void {
    this.contexts.set(pluginId, context)
  }

  /** 获取插件上下文（PluginViewHost 使用） */
  getContext(pluginId: string): PluginContext | undefined {
    return this.contexts.get(pluginId)
  }

  /** 记录插件运行态（由 loader 在加载成功 / 标记错误时写入，停用时随 clearPlugin 摘除） */
  setPluginState(pluginId: string, state: PluginState): void {
    this.pluginStates.set(pluginId, state)
    this.updateReactivePluginStates()
  }

  /** 读取插件运行态 */
  getPluginState(pluginId: string): PluginState | undefined {
    return this.pluginStates.get(pluginId)
  }

  /**
   * 判断插件的贡献面是否生效 —— 宿主「贡献项摘除 / 恢复」的唯一判据。
   *
   * 所有消费方（侧边栏菜单让位、设置分组渲染、未来的页面兜底）必须走本函数，
   * 避免同一事实被各判一次导致菜单与页面状态不一致（D7）。
   * 未登记运行态（如测试直接注册、或插件尚未上报）视为未生效。
   */
  isContributionActive(pluginId: string): boolean {
    return isContributionActiveState(this.pluginStates.get(pluginId))
  }

  /** 清理插件的所有注册 */
  clearPlugin(pluginId: string): void {
    this.contexts.delete(pluginId)
    this.pluginStates.delete(pluginId)
    this.updateReactivePluginStates()

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

    for (const key of [...this.pageToolbarItemsMap.keys()]) {
      if (key.startsWith(`${pluginId}:`)) {
        this.pageToolbarItemsMap.delete(key)
      }
    }
    this.updateReactivePageToolbar()

    for (const key of [...this.fileHandlers.keys()]) {
      if (key.startsWith(`${pluginId}:`)) {
        this.fileHandlers.delete(key)
      }
    }

    for (const key of [...this.httpEndpoints.keys()]) {
      if (key.startsWith(`${pluginId}:`)) {
        this.httpEndpoints.delete(key)
      }
    }

    for (const key of [...this.settingsSections.keys()]) {
      if (key.startsWith(`${pluginId}:`)) {
        this.settingsSections.delete(key)
      }
    }
    this.updateReactiveSettingsSections()
  }

  private updateReactiveViews() {
    const views = [...this.views.values()]
    // 先替换全量投影：getViewComponent 的响应式依赖源（见 viewsIndex 注释）
    this.viewsIndex.value = views
    // 按 order 升序排序（sort 为稳定排序，同 order 保持注册先后）
    views.sort((a, b) => a.order - b.order)
    this.sidebarViews.value = views.filter((v) => v.viewType === 'sidebar')
    this.toolboxViews.value = views.filter((v) => v.viewType === 'toolbox')
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

  private updateReactivePageToolbar() {
    this.pageToolbarItems.value = [...this.pageToolbarItemsMap.values()]
  }

  private updateReactiveSettingsSections() {
    const sections = [...this.settingsSections.values()]
    // 按 order 升序排序（sort 为稳定排序，同 order 保持注册先后）
    sections.sort((a, b) => a.order - b.order)
    this.settingsSectionsRef.value = sections
  }

  private updateReactivePluginStates() {
    this.pluginStatesRef.value = Object.fromEntries(this.pluginStates)
  }
}

/** 全局单例 */
const registry = new PluginRegistryClass()

/** 获取全局注册表 */
export function getPluginRegistry(): PluginRegistryClass {
  return registry
}
