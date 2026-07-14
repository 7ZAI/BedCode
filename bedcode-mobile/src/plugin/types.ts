/**
 * Mobile Plugin Types
 *
 * 移动端插件系统所有公开类型
 */

/** Disposable 接口 — 用于资源清理 */
export interface Disposable {
  dispose(): void
}

/** 插件类型 */
export type PluginType = 'rust' | 'rust-ts' | 'ts-only'

/** 插件清单结构 */
export interface PluginManifest {
  id: string
  name: string
  version: string
  description: string
  author: string
  /** 前端入口模块路径（如 "plugins/com.bedcode.ai-chatbox/index.js"） */
  main: string
  pluginType: PluginType
  permissions: string[]
  contributes: MobilePluginContributes
}

/** 移动端扩展点声明 */
export interface MobilePluginContributes {
  commands: CommandContribution[]
  views: ViewContribution[]
  terminal?: TerminalContribution
  navTab?: NavTabContribution
  settings?: SettingsContribution
  configuration?: PluginConfiguration
  lifecycle?: LifecycleContribution
}

/** 视图扩展点 */
export interface ViewContribution {
  id: string
  type: 'toolbox'
  title: string
  component: string
}

/** 底部导航 Tab 扩展点 */
export interface NavTabContribution {
  id: string
  title: string
  icon: string
  component: string
  order?: number
}

/** 设置页扩展点 */
export interface SettingsContribution {
  section: string
  component: string
}

/** 终端扩展点 */
export interface TerminalContribution {
  inputHandlers: string[]
  outputParsers: string[]
  toolbarItems: TerminalToolbarItemContribution[]
}

/** 终端工具栏按钮 */
export interface TerminalToolbarItemContribution {
  id: string
  title: string
  icon: string
}

/** 命令扩展点 */
export interface CommandContribution {
  id: string
  title: string
  icon?: string
}

/** 插件配置声明 */
export interface PluginConfiguration {
  title: string
  properties: Record<string, ConfigProperty>
}

/** 配置属性 */
export interface ConfigProperty {
  type: 'string' | 'number' | 'boolean'
  title: string
  description?: string
  default?: any
}

/** 生命周期声明 */
export interface LifecycleContribution {
  onStartup?: boolean
  onShutdown?: boolean
}

// ==================== UI 描述符 ====================

/** 工具箱页面描述符 */
export interface ToolboxPageDescriptor {
  id: string
  title: string
  component: any
}

/** 导航 Tab 描述符 */
export interface NavTabDescriptor {
  id: string
  title: string
  icon: string
  component: any
  order: number
}

/** 终端工具栏项描述符 */
export interface TerminalToolbarItemDescriptor {
  id: string
  label: string
  icon?: string
  onClick?: () => void
}

/** 设置区域描述符 */
export interface SettingsSectionDescriptor {
  id: string
  pluginId: string
  section: string
  component: any
}

// ==================== PluginContext API ====================

/** 命令注册表 */
export interface CommandRegistry {
  register(id: string, handler: (...args: any[]) => any): Disposable
  execute(id: string, ...args: any[]): Promise<any>
}

/** 终端 API（需 terminal:* 权限） */
export interface TerminalAPI {
  sendInput(sessionId: string, text: string): Promise<void>
  onOutput(handler: (sessionId: string, data: string) => void): Disposable
}

/** 会话 API（需 session:* 权限） */
export interface SessionAPI {
  list(): Promise<any[]>
  get(sessionId: string): Promise<any>
  onStatusChange(handler: (event: any) => void): Disposable
}

/** UI 注册表（需 ui:* 权限） */
export interface UIRegistry {
  registerToolboxPage(page: ToolboxPageDescriptor): Disposable
  registerNavTab(tab: NavTabDescriptor): Disposable
  registerTerminalToolbarItem(item: TerminalToolbarItemDescriptor): Disposable
  registerSettingsSection(section: SettingsSectionDescriptor): Disposable
}

/** 事件 API */
export interface EventAPI {
  on(event: string, handler: (...args: any[]) => void): Disposable
  emit(event: string, ...args: any[]): void
}

/** 存储 API（默认授予） */
export interface StorageAPI {
  get<T = any>(key: string): Promise<T | undefined>
  set(key: string, value: any): Promise<void>
  delete(key: string): Promise<void>
}

/** 国际化 API */
export interface I18nAPI {
  registerMessages(locale: string, messages: Record<string, any>): void
  t(key: string, params?: Record<string, any>): string
}

/** 插件上下文 — 插件访问宿主能力的唯一通道 */
export interface PluginContext {
  readonly id: string
  readonly commands: CommandRegistry
  readonly terminal: TerminalAPI
  readonly session: SessionAPI
  readonly ui: UIRegistry
  readonly events: EventAPI
  readonly storage: StorageAPI
  readonly i18n: I18nAPI
  /** 内部：所有 Disposable 收集器 */
  readonly _disposables: Disposable[]
}

/** 插件入口模块约定 */
export interface PluginModule {
  activate(context: PluginContext): Promise<void>
  deactivate?: () => Promise<void>
}

/** 插件运行时状态 */
export type PluginState =
  | { state: 'Loaded' }
  | { state: 'Activated' }
  | { state: 'Deactivated' }
  | { state: 'Error'; error: string }

/** 插件信息（从后端获取） */
export interface PluginInfo {
  id: string
  name: string
  version: string
  description: string
  author: string
  main: string
  pluginType: PluginType
  permissions: string[]
  state: PluginState
  contributes: MobilePluginContributes
}
