/**
 * @bedcode/plugin-sdk-mobile 类型定义
 *
 * 移动端插件开发者可用的所有公开类型
 */

// ==================== 基础类型 ====================

/** Disposable 接口 */
export interface Disposable {
  dispose(): void
}

/** 插件类型 */
export type PluginType = 'rust' | 'rust-ts' | 'ts-only' | 'wasm'

/** 插件运行时状态 */
export type PluginState =
  | { state: 'Loaded' }
  | { state: 'Activated' }
  | { state: 'Deactivated' }
  | { state: 'Error'; error: string }

/** 插件描述文件结构 */
export interface PluginManifest {
  id: string
  name: string
  version: string
  description: string
  author: string
  main: string
  pluginType: PluginType
  permissions: string[]
  contributes: MobilePluginContributes
  wasmHash?: string
  rustLibrary?: string
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

/** 命令扩展点 */
export interface CommandContribution {
  id: string
  title: string
  icon?: string
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
  onAuthSuccess?: boolean
  onDisconnect?: boolean
  onSessionCreated?: boolean
  onSessionStopped?: boolean
  onTerminalInput?: boolean
  onTerminalOutput?: boolean
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

/** 终端 API */
export interface TerminalAPI {
  sendInput(sessionId: string, text: string): Promise<void>
  onOutput(handler: (sessionId: string, data: string) => void): Disposable
}

/** 会话 API */
export interface SessionAPI {
  list(): Promise<any[]>
  get(sessionId: string): Promise<any>
  onStatusChange(handler: (event: any) => void): Disposable
}

/** UI 注册表 */
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

/** 存储 API */
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

/** 生命周期 API */
export interface LifecycleAPI {
  onAppStartup(handler: () => void): Disposable
  onAppShutdown(handler: () => void): Disposable
  onAuthSuccess(handler: () => void): Disposable
  onDisconnect(handler: (reason: string) => void): Disposable
  onSessionCreated(handler: (sessionId: string) => void): Disposable
  onSessionStopped(handler: (sessionId: string) => void): Disposable
  onTerminalInput(handler: (sessionId: string, data: string) => void): Disposable
  onTerminalOutput(handler: (sessionId: string, data: string) => void): Disposable
}

/** 插件上下文 */
export interface PluginContext {
  readonly id: string
  readonly commands: CommandRegistry
  readonly terminal: TerminalAPI
  readonly session: SessionAPI
  readonly ui: UIRegistry
  readonly events: EventAPI
  readonly storage: StorageAPI
  readonly i18n: I18nAPI
  readonly lifecycle: LifecycleAPI
  readonly _disposables: Disposable[]
}

/** 插件入口模块约定 */
export interface PluginModule {
  activate(context: PluginContext): Promise<void>
  deactivate?: () => Promise<void>
}
