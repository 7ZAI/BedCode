/**
 * @binblink/bedcode-plugin-sdk-desktop 类型定义
 *
 * 插件系统所有公开类型 — 插件通过此包引用，无需依赖宿主源码
 *
 * 注意：本文件与宿主 `bedcode-desktop/src/plugin/types.ts` 为双写副本，
 * 修改任一文件时必须同步另一份
 */

// ==================== 基础类型 ====================

/** Disposable 接口 — 用于资源清理 */
export interface Disposable {
  dispose(): void
}

/** 插件类型 */
export type PluginType = 'rust' | 'rust-ts' | 'ts-only'

/** 组件装配角色（manifest `type` 字段，core-plugin-manager）：系统组件 / 应用插件 */
export type PluginKind = 'system' | 'application'

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
  /**
   * 组件装配角色：`system`（系统组件）/ `application`（应用插件，缺省）
   *
   * 系统组件内置、默认启用、只停不删、先于应用插件激活，其导出的 host-*
   * 同形接口注册为能力提供者。缺省 application，旧插件零迁移。
   */
  type?: PluginKind
  /**
   * 能力依赖声明（应用插件消费的能力名，WIT host-* 接口名）
   *
   * 宿主激活时校验：每个依赖必须已有提供者（宿主原语或已激活系统组件），
   * 缺失即激活失败并指明能力名。缺省空数组 = 无依赖。
   */
  dependencies?: string[]
  /**
   * 单插件 Store 资源上限覆盖请求（core-config × core-security）
   *
   * 重型插件可请求更大的燃料预算/线性内存；未设置的字段继承内核配置。
   * 最终值由宿主安全模块仲裁：逐字段取 min（请求值、内核配置值、编译期
   * 硬上限）——插件只能自我收紧，放宽请求被钳回上限。缺省 = 全部继承。
   */
  resourceOverrides?: ResourceOverrides
}

/**
 * 单插件 Store 资源上限覆盖请求（manifest `resourceOverrides`）
 *
 * 字段可选：未设置即继承内核配置；设置即请求值，宿主仲裁后生效
 * （见 `PluginManifest.resourceOverrides`）。
 */
export interface ResourceOverrides {
  /** 单次导出调用燃料预算（指令数） */
  fuelPerCall?: number
  /** 线性内存上限（字节） */
  maxMemoryBytes?: number
  /** 表元素上限 */
  maxTableEntries?: number
  /** 单 Store 核心实例数上限 */
  maxInstances?: number
  /** 单 Store 线性内存数量上限 */
  maxMemories?: number
  /** 单 Store 表数量上限 */
  maxTables?: number
}

/** 插件配置声明 */
export interface PluginConfiguration {
  title: string
  properties: Record<string, ConfigProperty>
}

/** 配置属性定义 */
export interface ConfigProperty {
  type: 'string' | 'number' | 'boolean'
  title: string
  description?: string
  default?: any
  enum?: string[]
}

/** 插件扩展点声明 */
export interface PluginContributes {
  commands: CommandContribution[]
  views: ViewContribution[]
  terminal?: TerminalContribution
  toolProviders: ToolProviderContribution[]
  fileHandlers: FileHandlerContribution[]
  /**
   * 插件 HTTP 端点清单（`_http_endpoint` 的路径白名单）。
   * 条目为相对路径段（不含 `/api/plugin/<插件 id>/` 前缀）。声明后宿主对未注册路径
   * 返回 404；缺省（空清单）视为「未声明」，宿主保持前缀内 ANY 放行的过渡策略。
   */
  httpEndpoints?: string[]
  configuration?: PluginConfiguration
  lifecycle?: LifecycleContribution
}

/** 生命周期扩展点声明 */
export interface LifecycleContribution {
  onStartup?: boolean
  onShutdown?: boolean
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
  type: 'sidebar' | 'toolbox' | 'statusbar'
  title: string
  component: string
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

// ==================== UI 描述符 ====================

/** 侧边栏面板描述符 */
export interface SidebarPanelDescriptor {
  id: string
  title: string
  /** SVG path d 属性字符串（Heroicons outline 风格，stroke-width=2，viewBox=0 0 24 24）
   * 与宿主内置菜单共用同一图标体系，可包含多个 M 子路径组合成完整图标 */
  icon?: string
  /** 菜单排序值，升序排列（越小越靠前），缺省 600。
   * 宿主内置菜单只剩恒最末的插件管理(9998) / 设置(9999)（业务槽位已随票 13/14
   * 下沉插件，宿主无内置业务入口）；插件目录按自身 order 排布，同域其余项取 +1/+10。
   * 同值按注册先后排列。 */
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

// ==================== 全局弹窗（桌面宿主通用能力） ====================

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
  onInput(handler: (sessionId: string, text: string) => string | null): Disposable
}

/** 会话 API（需 session:* 权限） */
export interface SessionAPI {
  list(): Promise<any[]>
  get(sessionId: string): Promise<any>
  onStatusChange(handler: (event: any) => void): Disposable
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
   * （viewType 'page'，宿主全量注册表可见、sidebar/toolbox 菜单投影不含）。
   * 仍经 `/plugin/sidebar/:pluginId/:viewId` 路由渲染——适合「从某页内进入的
   * 二级页面 / 深链直达」场景（票 14 收尾：连接历史不再占侧边栏，改经设备列表入口）。
   * order 字段在非菜单页面无排序意义，可省略。
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
  /** dev-shell 领域种子数据（见 PluginDevMock）；真实宿主忽略，无需条件编译 */
  devMock?: PluginDevMock
}

// ==================== devMock 协议（dev-shell 领域种子数据） ====================
//
// 种子数据由插件工程持有（入口导出 devMock），dev-shell 只做通用接线：
// loader 按 pluginId 注册，mock 命令实现消费种子返回演示值。
// 真实宿主忽略多余导出，与移动端 SDK 同构。
//
// SDK 只约定「入口导出 devMock」的通用容器协议，不感知任何插件领域细节：
// 各插件自有类型（文件传输对等/传输域种子等）由插件工程自行定义，
// dev-shell 消费时 cast 到插件自有形状。

/** 插件 dev-shell 演示数据（通用容器；具体子域类型由各插件工程持有） */
export type PluginDevMock = Record<string, unknown>

/** 插件运行时状态
 *
 * 与宿主侧 `bedcode-desktop/src/plugin/types.ts` 为双写副本，修改任一文件必须同步另一份。
 * 线协议形状与 Rust serde 一一对应（tag = "state", content = "error"）。
 */
export type PluginState =
  | { state: 'Loaded' }
  | { state: 'Activating' }
  | { state: 'Activated' }
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
