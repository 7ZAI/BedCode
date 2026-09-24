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
  /**
   * 插件 HTTP 端点清单（`_http_endpoint` 的路径白名单 + 认证档位，票 16 / 票 08）：
   * 条目为不含 `/api/plugin/<插件 id>/` 前缀的相对路径段，可写成 `{ path, auth }`
   * 声明档位。宿主只认声明：未声明路径 404，空/缺省清单 = 该插件没有 HTTP 面
   * （票据 03 的「前缀内 ANY 放行」过渡策略已随票 08 退役）。
   */
  httpEndpoints?: HttpEndpointContribution[]
  /** 配置声明 */
  configuration?: PluginConfiguration
  /** 生命周期钩子声明 */
  lifecycle?: LifecycleContribution
}

/** HTTP 端点认证档位（票 08，与 SDK `EndpointAuthTier` 同形） */
export type EndpointAuthTier = 'none' | 'jwt'

/** 一条 HTTP 端点声明：纯路径段（档位 = 宿主最严缺省 jwt）或 `{ path, auth }` 对象 */
export type HttpEndpointContribution = string | { path: string; auth?: EndpointAuthTier }

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
  pluginType: PluginType
  /** WASM 库文件名（仅 rust-ts 类型插件使用） */
  rustLibrary?: string
  permissions: string[]
  state: PluginState
  extensionPath: string
  contributes: PluginContributes
  /** 插件图标（manifest.icon 透传，可为空） */
  icon?: string
  /** 插件来源：builtin / scanned / wasm / user-installed（zip 安装）——所有来源均可卸载 */
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
   * 与宿主内置菜单（设备配对 100 / 终端会话 200 / 服务器 300 保留不复用 / 插件管理 9998 / 设置 9999）
   * 共用同一排序空间，可指定任意值插入到内置菜单项之间（如 150 位于"设备配对"与"终端会话"之间）；
   * 同值按注册先后排列。
   * 注意：与「设备配对 / 终端会话」内置项**同 order 值**即视为接管该域——宿主内置入口随之让位
   * （本插件 error / 停用后自动恢复）；插在两者之间的新域目录不触发让位 */
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

/** 设置分组描述符（插件往宿主设置页贡献一个分组，需 `ui:settings` 权限） */
export interface SettingsSectionDescriptor {
  id: string
  /** 分组标题的 i18n key，相对插件自身命名空间
   * （宿主按 `${pluginId}.${titleKey}` 解析，与 `context.i18n.t` 同一前缀规则；
   * 未注册该 key 时 vue-i18n 回退显示 key 本身） */
  titleKey: string
  /** SVG path d 属性字符串（Heroicons outline 风格，stroke-width=2，viewBox=0 0 24 24） */
  icon?: string
  /** 排序值，升序排列（越小越靠前），缺省与其余贡献面一致（600）。
   * 与宿主内置分组共用同一排序空间，可指定任意值插入内置分组之间；同值按注册先后排列 */
  order?: number
  /** 分组内容组件：只渲染卡片正文，外层 `<section>` 与标题由宿主统一渲染（保证与内置分组像素一致） */
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
  // 票 08：`sendInput` 已退役（宿主不再替插件导流输入）；
  // 插件写自家会话输入走命令通道 `session.input`。
  onOutput(handler: (sessionId: string, data: string) => void): Disposable
  onInput(handler: (sessionId: string, text: string) => string | null): Disposable
}

/**
 * 会话 API（需 `session:read` 权限）
 *
 * 票 08 / 09 起只剩**宿主窗口事实**：数据面（`list` / `get`）与事件面
 * （`onStatusChange`）已退役——插件读会话走自家命令通道
 * （`session.list` / `session.get`），会话状态由插件自己发布。
 */
export interface SessionAPI {
  /**
   * 预测宿主终端窗口的初始网格（宿主窗口几何 + 字体测量 + 整体缩放）。
   *
   * 供插件在启动会话前把 `cols` / `rows` 交给 `create-with-spec`——终端窗口本体、
   * 创建尺寸与渲染管线留宿主（spec D3），预测规则必须与窗口创建规则同源，故留在
   * 宿主一处实现而不在插件复刻。任一环节不可用（非 Tauri 环境 / 字体未就绪）返回
   * `null`，调用方不传尺寸、由宿主兜底默认网格。
   */
  predictTerminalSize(): Promise<{ cols: number; rows: number } | null>
  /**
   * 打开（或聚焦）宿主终端窗口——宿主既有路由深链 `/terminal-window/:sessionId`。
   *
   * 窗口几何、贴靠、就绪事件与关闭一律留宿主；插件只触发，不自建窗口、不自带渲染管线。
   *
   * @returns `true` = 新建窗口（尚在就绪中，调用方宜显示 loading）；`false` = 既有窗口已聚焦
   */
  openTerminal(session: { id: string; name: string }): Promise<boolean>
  /**
   * 关闭宿主终端窗口（幂等：窗口不存在时静默返回）。
   *
   * 会话停止 / 删除时由调用方触发——「何时该关窗」是插件侧的编排决策。
   */
  closeTerminal(sessionId: string): Promise<void>
  /**
   * 该会话是否已有打开的终端窗口（宿主窗口登记事实，同步）。
   *
   * 调用方据此决定「先聚焦（无需 loading）还是新建（显示就绪 loading）」——
   * 与宿主会话页原行为同口径。
   */
  isTerminalOpen(sessionId: string): boolean
}

/** UI 注册表（需 ui:* 权限） */
export interface UIRegistry {
  registerSidebarPanel(panel: SidebarPanelDescriptor): Disposable
  registerToolboxPage(page: ToolboxPageDescriptor): Disposable
  /**
   * 注册一个可由插件页路由直达、但**不进入侧边栏菜单**的页面
   * （viewType 'page'：不进 sidebarViews/toolboxViews 投影，仅存全量注册表）。
   * 仍经 `/plugin/sidebar/:pluginId/:viewId` 路由渲染（如从设备列表进入连接历史深链），
   * 适合「从某页内进入的二级页面」场景（票 14 收尾：连接历史不再占侧边栏）。
   */
  registerPage(page: SidebarPanelDescriptor): Disposable
  registerStatusBarItem(item: StatusBarItemDescriptor): Disposable
  registerInputExtension(ext: InputExtensionDescriptor): Disposable
  registerTerminalToolbarItem(item: TerminalToolbarItemDescriptor): Disposable
  registerTitleBarItem(item: TitleBarItemDescriptor): Disposable
  registerPageToolbarItem(item: PageToolbarItemDescriptor): Disposable
  registerFileHandler(handler: FileHandlerDescriptor): Disposable
  /** 往宿主设置页贡献一个分组（见 SettingsSectionDescriptor；权限 ui:settings） */
  registerSettingsSection(section: SettingsSectionDescriptor): Disposable
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
  /** 内部：所有 Disposable 收集器 */
  readonly _disposables: Disposable[]
}

/** 插件入口模块约定 */
export interface PluginModule {
  activate(context: PluginContext): Promise<void>
  deactivate?: () => Promise<void>
}
