/**
 * @binblink/plugin-sdk-mobile 类型定义
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
  | { state: 'Activating' }
  | { state: 'Activated' }
  | { state: 'Degraded'; error: string }
  | { state: 'NeedsApproval' }
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
  /** 插件图标：emoji、内联 <svg> 标记或相对插件目录的图片路径，缺省时前端生成字母头像回退 */
  icon?: string
  wasmHash?: string
  rustLibrary?: string
  /** 启用前预授权目录（宿主 preauthorize 统一弹窗，支持 ${downloads} 模板） */
  preauthDirs?: string[]
}

/** 移动端扩展点声明 */
export interface MobilePluginContributes {
  commands: CommandContribution[]
  views: ViewContribution[]
  terminal?: TerminalContribution
  navTab?: NavTabContribution
  settings?: SettingsContribution
  /** 动态路由扩展点（activate 时经 ui.registerRoute 注册，宿主 addRoute） */
  routes?: RouteContribution[]
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

/** 路由扩展点声明（manifest 声明，宿主按 id 命名空间挂载动态路由） */
export interface RouteContribution {
  id: string
  title?: string
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
  /** 入口图标：emoji 或 SVG path d 字符串（Heroicons outline 风格，viewBox=0 0 24 24），缺省 🧩 */
  icon?: string
  component: any
  /** 可选：插件自定义入口卡片组件（宿主 ToolboxView 在入口列表内联渲染，
   *  需自带实时状态角标；缺省时宿主用统一卡片）。宿主经 PluginViewHost provide pluginContext。 */
  entry?: any
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

/** 插件路由描述符：整体路由由插件决定（openPage 跳转）；id 即路径段，可含 '/' 支持深路径 */
export interface PluginRouteDescriptor {
  /** 路由 id（路径段），宿主挂到 /mobile/plugins/{pluginId}/{id} */
  id: string
  /** 宿主页头标题（header 为真时展示） */
  title?: string
  component: any
  /** 是否渲染宿主页头（back + title），默认 true；false 时插件自带布局 */
  header?: boolean
}

/** 日志 API */
export interface LoggerAPI {
  info(message: string): void
  debug(message: string): void
  warn(message: string): void
  error(message: string): void
}

// ==================== 移动端宿主能力 ====================

/** HTTP API 结果（与宿主 useHttpApi 同构） */
export interface MobileHttpResult<T = any> {
  code: number
  message: string
  data?: T
}

/** 通用对端 REST 请求选项（宿主注入 JWT / 链路加密 / 错误归一化） */
export interface MobileHttpRequestOptions {
  method?: string
  body?: any
  headers?: Record<string, string>
}

/** 移动端宿主连接/HTTP 能力（共享运行时 mobileApi 模块）
 *
 * 通用能力层：连接状态 + 对端桌面端 REST 请求通道。
 * 具体插件业务端点（任务队列 / 会话模式 / 任务历史 / 定时任务等）
 * 由各插件基于 httpRequest 自行封装，SDK 不感知插件领域细节。
 */
export interface MobileHostApi {
  /** 当前活动会话 id（响应式 ref，可 watch / computed） */
  activeSessionId: import('vue').Ref<string | null>
  /** 活跃会话列表（响应式 ref） */
  activeSessions: import('vue').Ref<any[]>
  /** 会话配置列表（响应式 ref） */
  sessionConfigs: import('vue').Ref<any[]>
  /** 是否已连接对端桌面端（响应式 ref，可 watch / computed） */
  isConnected: import('vue').Ref<boolean>
  /** 通用对端 REST 请求；返回 { code, message, data } 形状 */
  httpRequest<T = any>(path: string, options?: MobileHttpRequestOptions): Promise<MobileHttpResult<T>>
}

// ==================== 对话框 ====================

/** 对话框选项 */
export interface DialogOptions {
  title?: string
  message?: string
  /** 视觉风格（默认 info） */
  variant?: 'info' | 'warning' | 'danger'
  confirmText?: string
  cancelText?: string
  /** showPrompt 时：输入框 placeholder */
  inputPlaceholder?: string
  /** showPrompt 时：输入框默认值 */
  inputValue?: string
  /** 是否可点击背景关闭（默认 false） */
  dismissible?: boolean
}

/** 对话框结果 */
export interface DialogResult {
  action: 'confirm' | 'cancel'
  value?: string
}

/** 对话框 API — 宿主渲染移动端样式弹窗 */
export interface DialogAPI {
  /** 通用对话框：返回用户操作结果 */
  showDialog(options: DialogOptions): Promise<DialogResult>
  /** 确认框：返回是否确认 */
  showConfirm(options: DialogOptions): Promise<boolean>
  /** 输入框：返回输入值；取消返回 null */
  showPrompt(options: DialogOptions): Promise<string | null>
  /** 轻提示（宿主 toast） */
  showToast(message: string, type?: 'info' | 'success' | 'warning' | 'error'): void
}

// ==================== 通知 ====================

/** 系统通知 API — 走宿主 tauri-plugin-notification */
export interface NotificationAPI {
  notify(title: string, body?: string): Promise<void>
}

// ==================== 状态上报 ====================

/** 插件状态上报 API — 启用时通过生命周期函数上报启动成功/失败 */
export interface StatusAPI {
  /** 显式上报启动成功（activate 隐式成功之外的自愈通道：Error → Activated） */
  reportReady(): Promise<void>
  /** 上报启动/运行失败，宿主置 Error 状态并持久化未启用 */
  reportError(error: string): Promise<void>
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
  /** 动态注册插件路由（宿主 addRoute 至 /mobile/plugins/{pluginId}/{id}；Disposable.dispose = removeRoute 撤销） */
  registerRoute(route: PluginRouteDescriptor): Disposable
  /** 整体跳转到本插件已注册路由；返回入口页用 goBack 或宿主页头返回按钮 */
  openPage(routeId: string): void
  /** 返回上一页（router.back） */
  goBack(): void
  /** 监听 Android 系统返回键（仅 Android 真机触发；注册后系统返回不再执行默认的 webview 后退/退出，改由回调接管）。
   *  回调需自行决定行为：目录栈内返回上级，栈顶时可用 payload.canGoBack 恢复默认后退（如 history.back()）。
   *  非 Android（dev-shell / iOS）静默降级为永不触发；Disposable.dispose = 取消监听并恢复默认行为。 */
  onBackPressed(handler: (payload: { canGoBack: boolean }) => void): Disposable
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

// ==================== System API ====================

/** 系统 API — 宿主 OS 级文件操作（需 system:open 权限） */
export interface SystemAPI {
  /** 用系统查看器打开本地文件（传输完成「打开本地文件」；Android ACTION_VIEW） */
  openFile(path: string, displayName?: string): Promise<void>
  /** 用系统文件管理器打开文件所在目录（历史记录「打开所在文件夹」；
   * Android FileProvider 暴露父目录 + ACTION_VIEW，需 system:open 权限） */
  revealInDir(path: string): Promise<void>
  /** 按文件名打开接收文件的所在目录（历史「打开所在文件夹」真机路径；
   * 接收落点不在 wire 上，宿主 MediaStore 公共下载按 displayName 命中 →
   * primary:Download 目录，未命中回退 app 私有下载目录；需 system:open 权限） */
  revealReceivedFileLocation(fileName: string): Promise<void>
}

// ==================== 插件开发期领域数据（dev-shell mock 协议） ====================

/**
 * 插件开发期领域数据（dev-shell mock 协议，仅浏览器 dev 环境消费）。
 *
 * 与"宿主能力 mock"（会话/对话框/事件/HTTP 接口等，固定在 dev-shell 内实现）区分：
 * 本协议只承载各插件自己的业务演示数据，由插件入口导出 devMock，dev-shell
 * 加载插件时按 pluginId 注册、按需取用。
 *
 * SDK 只约定「入口导出 devMock」的通用容器协议，不感知任何插件领域细节：
 * 各插件自有类型（任务队列种子 / 文件传输对等与传输域种子等）
 * 由插件工程自行定义；dev-shell 消费时 cast 到插件自有形状。
 * 真实宿主忽略该字段（多余导出对 activate 无影响），插件无需条件编译。
 */
export type PluginDevMock = Record<string, unknown>

/** 国际化 API */
export interface I18nAPI {
  registerMessages(locale: string, messages: Record<string, unknown>): void
  t(key: string, params?: Record<string, unknown>): string
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
  readonly logger: LoggerAPI
  /** 对话框（弹窗扩展性） */
  readonly dialogs: DialogAPI
  /** 系统通知 */
  readonly notifications: NotificationAPI
  /** 系统 API（需 system:open 权限） */
  readonly system: SystemAPI
  /** 生命周期状态上报（启用成功/失败） */
  readonly status: StatusAPI
  readonly _disposables: Disposable[]
}

/** 插件入口模块约定 */
export interface PluginModule {
  activate(context: PluginContext): Promise<void>
  deactivate?: () => Promise<void>
  /** dev-shell 领域数据（见 PluginDevMock）；真实宿主忽略 */
  devMock?: PluginDevMock
}
