/**
 * Plugin Types
 *
 * 插件系统类型定义 — manifest、context、扩展点描述符
 *
 * 注意：本文件与 `packages/plugin-sdk-desktop/src/types.ts` 为双写副本，
 * 修改任一文件时必须同步另一份（SDK 包供插件引用，宿主副本供宿主内部引用）
 */

/** Disposable 接口 — 用于资源清理 */
export interface Disposable {
  dispose(): void
}

/** 插件类型 */
export type PluginType = 'rust' | 'rust-ts' | 'ts-only'

/** 插件描述文件 (plugin.json) 结构 */
export interface PluginManifest {
  id: string
  name: string
  version: string
  description: string
  author: string
  main: string
  sandbox: 'inline' | 'isolated'
  pluginType: PluginType
  /** WASM 库文件名（仅 rust-ts 类型插件使用） */
  rustLibrary?: string
  /** 插件图标：图片路径（相对插件目录）或内联 SVG 标记 */
  icon?: string
  permissions: string[]
  contributes: PluginContributes
}

/** 插件配置声明 */
export interface PluginConfiguration {
  /** 配置区域标题 */
  title: string
  /** 配置属性映射（key → 属性定义） */
  properties: Record<string, ConfigProperty>
}

/** 配置属性定义 */
export interface ConfigProperty {
  /** 属性类型 */
  type: 'string' | 'number' | 'boolean'
  /** 显示标题 */
  title: string
  /** 帮助描述 */
  description?: string
  /** 默认值 */
  default?: any
  /** 枚举选项（type 为 string 时使用） */
  enum?: string[]
  /** 数值范围（type 为 number 时使用；带范围时配置页渲染为滑块） */
  minimum?: number
  maximum?: number
}

/** 插件扩展点声明 */
export interface PluginContributes {
  commands: CommandContribution[]
  views: ViewContribution[]
  terminal?: TerminalContribution
  toolProviders: ToolProviderContribution[]
  fileHandlers: FileHandlerContribution[]
  /** 配置声明 */
  configuration?: PluginConfiguration
  /** 生命周期钩子声明 */
  lifecycle?: LifecycleContribution
}

/** 生命周期扩展点声明 */
export interface LifecycleContribution {
  /** 是否注册 onStartup 回调 */
  onStartup?: boolean
  /** 是否注册 onShutdown 回调 */
  onShutdown?: boolean
}

/** 命令扩展点 */
export interface CommandContribution {
  id: string
  title: string
  icon?: string
}

/** 视图扩展点（statusbar 项无静态 title/component，运行时注册动态 label） */
export interface ViewContribution {
  id: string
  type: 'sidebar' | 'toolbox' | 'statusbar'
  title?: string
  component?: string
  icon?: string
}

/** 终端扩展点 */
export interface TerminalContribution {
  inputHandlers: string[]
  outputParsers: string[]
}

/** 外部工具扩展点 */
export interface ToolProviderContribution {
  id: string
  name: string
  endpoint: string
}

/** 文件处理扩展点 */
export interface FileHandlerContribution {
  id: string
  extensions: string[]
  viewer: string
  icon?: string
}

/** 插件运行时状态
 *
 * 线协议形状与 Rust serde 一一对应（tag = "state", content = "error"）：
 * 单元变体 → `{ state: "Loaded" }`；newtype 变体 → `{ state: "Degraded", error: "..." }`。
 * Rust 源：packages/plugin-sdk-desktop/rust/src/types.rs 的 PluginState
 */
export type PluginState =
  | { state: 'Loaded' }
  /** 激活进行中（auto-activation / 手动激活期间的瞬时中间态） */
  | { state: 'Activating' }
  | { state: 'Activated' }
  /** activate 成功但 on_startup 失败：实例可用、扩展点已注册，启动初始化未完成 */
  | { state: 'Degraded'; error: string }
  | { state: 'NeedsApproval' }
  | { state: 'Error'; error: string }
  | { state: 'Deactivated' }

/** 插件信息（从后端获取） */
export interface PluginInfo {
  id: string
  name: string
  version: string
  description: string
  author: string
  main: string
  sandbox: string
  pluginType: PluginType
  /** WASM 库文件名（仅 rust-ts 类型插件使用） */
  rustLibrary?: string
  permissions: string[]
  state: PluginState
  extensionPath: string
  contributes: PluginContributes
  /** 插件图标（manifest.icon 透传，可为空） */
  icon?: string
  /** 插件来源：builtin / scanned / wasm */
  source: string
  /** 插件目录总大小（字节） */
  sizeBytes: number
  /** 安装时间（unix 毫秒，plugin.json mtime） */
  installedAt?: number
}

/** 侧边栏面板描述符 */
export interface SidebarPanelDescriptor {
  id: string
  title: string
  /** SVG path d 属性字符串（Heroicons outline 风格，stroke-width=2，viewBox=0 0 24 24）
   * 与宿主内置菜单共用同一图标体系，可包含多个 M 子路径组合成完整图标 */
  icon?: string
  /** 菜单排序值，升序排列（越小越靠前），缺省 600。
   * 与宿主内置菜单（终端会话 100 / 服务器 200 / 设备 300 / 插件 400 / 设置 700）共用同一排序空间，
   * 可指定任意值插入到内置菜单项之间（如 150 位于"终端会话"与"服务器"之间）；
   * 同值按注册先后排列 */
  order?: number
  component: any
}

/** 工具箱页面描述符 */
export interface ToolboxPageDescriptor {
  id: string
  title: string
  /** SVG path d 属性字符串（Heroicons outline 风格，stroke-width=2，viewBox=0 0 24 24） */
  icon?: string
  /** 菜单排序值，升序排列（越小越靠前），缺省 600。
   * 与宿主内置菜单共用同一排序空间，可插入任意内置项之间 */
  order?: number
  component: any
}

/** 状态栏项描述符 */
export interface StatusBarItemDescriptor {
  id: string
  label: string
  icon?: string
  onClick?: () => void
}

// ==================== 全局弹窗（宿主通用能力，SDK types.ts 双写副本） ====================

/** 弹窗动作按钮（预设模式，Label 文案由插件经自身 i18n 生成后传入） */
export interface PluginDialogAction {
  label: string
  /** 视觉变体：默认 / 主按钮 / 危险 / 幽灵 */
  kind?: 'default' | 'primary' | 'danger' | 'ghost'
  /** 点击回调（可异步）；成功返回后自动关闭弹窗（抛错则保持打开以便重试） */
  onClick?: () => void | Promise<void>
  /** 点击后经宿主 router 跳转的目标路由（先关弹窗再跳转） */
  navigateTo?: string
  /** 动作进行中禁用（如提交中防重复点击）；经 handle.update() 联动 */
  disabled?: boolean
}

/**
 * 全局弹窗选项（宿主统一渲染：遮罩/卡片/z-index/定时关闭/按钮/路由跳转）
 *
 * 两种模式：
 * - 预设模式：title + message + icon + actions，常见「确认/拒绝」类请求开箱即用；
 * - 组件模式：content 传入任意 Vue 组件（经 provide('pluginContext') 渲染），
 *   props 可经 handle.update() 热更新。
 * 定时关闭为可选能力：仅当 timeoutSec / deadlineAt 之一提供时生效。
 */
export interface PluginDialogOptions {
  /** 自定义内容组件（组件模式）；缺省用 title/message/icon/actions 预设渲染 */
  content?: any
  /** 内容组件 props（组件模式；handle.update() 可热更新） */
  props?: Record<string, unknown>
  /** 预设模式：标题 */
  title?: string
  /** 预设模式：正文 */
  message?: string
  /** 预设模式：图标（SVG path d，随文字颜色渲染） */
  icon?: string
  /** 预设模式：动作按钮组（缺省无按钮；顺序即展示顺序） */
  actions?: PluginDialogAction[]
  /** 定时自动关闭（可选）：相对秒数，从弹窗弹出起算 */
  timeoutSec?: number
  /** 定时自动关闭（可选）：绝对截止时间戳 ms（迟到打开 / 排队续算更准确，优先于 timeoutSec） */
  deadlineAt?: number
  /** 倒计时文案模板，{seconds} 占位（如 '{seconds} 秒后自动拒绝'）；缺省不显示倒计时 */
  countdownLabel?: string
  /** 超时回调（自动关闭前触发）；缺省仅关闭 */
  onTimeout?: () => void
  /** 是否可手动关闭（右上角关闭按钮 / Escape / 遮罩点击），默认 true */
  closable?: boolean
  /** 点击遮罩是否关闭，默认 true（closable=false 时无效） */
  closeOnBackdrop?: boolean
  /** 卡片宽度（Tailwind max-w-* 类），默认 max-w-md */
  widthClass?: string
  /** 内容区 padding（Tailwind 类），默认 p-0（组件模式自带内距，预设模式内部处理） */
  bodyClass?: string
  /** 关闭后回调（按钮 / 遮罩 / Escape / 超时 / close() / 排队项被取消 均触发） */
  onClose?: () => void
}

/** 全局弹窗句柄：调用方据此关闭或热更新当前弹窗 */
export interface PluginDialogHandle {
  /** 关闭当前弹窗（幂等）；队列中下一个弹窗自动接替 */
  close(): void
  /** 热更新选项（props / 文案 / 按钮 / 倒计时等） */
  update(options: Partial<PluginDialogOptions>): void
}

/** 输入扩展描述符 */
export interface InputExtensionDescriptor {
  id: string
  label: string
  icon?: string
  onActivate?: () => void
}

/** 终端工具栏项描述符 */
export interface TerminalToolbarItemDescriptor {
  id: string
  label: string
  icon?: string
  onClick?: () => void
}

/** 标题栏项描述符 */
export interface TitleBarItemDescriptor {
  id: string
  label: string
  icon?: string
  onClick?: () => void
}

/** 页面工具栏项描述符 — 注入到指定页面的工具栏页头右操作区 */
export interface PageToolbarItemDescriptor {
  /** 目标页面标识：sessions / devices / history / plugins / plugin-config / server / settings / terminal */
  target:
    | 'sessions'
    | 'devices'
    | 'history'
    | 'plugins'
    | 'plugin-config'
    | 'server'
    | 'settings'
    | 'terminal'
  id: string
  label: string
  icon?: string
  onClick?: () => void
}

/** 文件处理器描述符 */
export interface FileHandlerDescriptor {
  id: string
  extensions: string[]
  component: any
}

/** HTTP 请求处理器 */
export interface RequestHandler {
  (req: { method: string; path: string; body: any; headers: Record<string, string> }): Promise<{
    status: number
    body: any
  }>
}

// ==================== PluginContext API Types ====================

/** 命令注册表 */
export interface CommandRegistry {
  register(id: string, handler: (...args: any[]) => any): Disposable
  execute(id: string, ...args: any[]): Promise<any>
}

/** 终端 API（需 terminal:* 权限） */
export interface TerminalAPI {
  sendInput(sessionId: string, text: string): Promise<void>
  onOutput(handler: (sessionId: string, data: string) => void): Disposable
  onInput(handler: (sessionId: string, text: string) => string | null): Disposable
}

/** 会话 API（需 session:* 权限） */
export interface SessionAPI {
  list(): Promise<any[]>
  get(sessionId: string): Promise<any>
  onStatusChange(handler: (event: any) => void): Disposable
}

/** UI 注册表（需 ui:* 权限） */
export interface UIRegistry {
  registerSidebarPanel(panel: SidebarPanelDescriptor): Disposable
  registerToolboxPage(page: ToolboxPageDescriptor): Disposable
  registerStatusBarItem(item: StatusBarItemDescriptor): Disposable
  registerInputExtension(ext: InputExtensionDescriptor): Disposable
  registerTerminalToolbarItem(item: TerminalToolbarItemDescriptor): Disposable
  registerTitleBarItem(item: TitleBarItemDescriptor): Disposable
  registerPageToolbarItem(item: PageToolbarItemDescriptor): Disposable
  registerFileHandler(handler: FileHandlerDescriptor): Disposable
  /** 全局弹窗（宿主统一渲染遮罩/卡片/倒计时/按钮/路由跳转；见 PluginDialogOptions） */
  showDialog(options: PluginDialogOptions): PluginDialogHandle
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
  flush(): Promise<void>
}

/** HTTP API（需 network:http 权限） */
export interface HttpAPI {
  registerEndpoint(path: string, handler: RequestHandler): Disposable
}

/** 系统 API — 宿主 OS 级文件操作（需 system:open 权限） */
export interface SystemAPI {
  /** 在系统文件管理器中显示文件/目录（Windows 资源管理器选中、macOS Finder Reveal） */
  revealInDir(path: string): Promise<void>
}

/** 国际化 API — 插件访问宿主 i18n 能力 */
export interface I18nAPI {
  /** 获取宿主 i18n 实例（vue-i18n I18n 对象） */
  getI18n(): any
  /** 注册插件翻译到宿主 i18n（自动添加插件 ID 前缀隔离） */
  registerMessages(locale: string, messages: Record<string, unknown>): void
  /** 翻译快捷方法（自动添加插件 ID 前缀） */
  t(key: string, params?: Record<string, unknown>): string
}

/** 插件上下文 — 插件访问宿主能力的唯一通道 */
export interface PluginContext {
  readonly id: string
  readonly extensionPath: string
  readonly commands: CommandRegistry
  readonly terminal: TerminalAPI
  readonly session: SessionAPI
  readonly ui: UIRegistry
  readonly events: EventAPI
  readonly storage: StorageAPI
  readonly http: HttpAPI
  /** 国际化 API */
  readonly i18n: I18nAPI
  /** 系统 API（需 system:open 权限） */
  readonly system: SystemAPI
  /** 内部：所有 Disposable 收集器 */
  readonly _disposables: Disposable[]
}

/** 插件入口模块约定 */
export interface PluginModule {
  activate(context: PluginContext): Promise<void>
  deactivate?: () => Promise<void>
}
