/**
 * @binblink/bedcode-plugin-sdk-mobile 类型定义
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

// ==================== 全局弹窗（宿主通用能力，与桌面端 SDK 同构） ====================

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
  /** 全局弹窗（宿主统一渲染遮罩/卡片/倒计时/按钮/路由跳转；见 PluginDialogOptions） */
  showDialog(options: PluginDialogOptions): PluginDialogHandle
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
  /** 引导开启「所有文件访问」权限（打开系统公共 Download 目录需要
   * MANAGE_EXTERNAL_STORAGE，无运行时弹窗只能跳系统设置页手动开启；
   * 未授权时跳转系统授权页并在无该页面的 ROM 兑底应用详情页），返回
   * 跳转前的授权状态；授权后用户重试 revealReceivedFileLocation 即达；
   * 需 system:open 权限） */
  requestAllFilesAccess(): Promise<boolean>
  /** 打开系统公共下载目录（设置页下载目录区「打开」，核对文件是否落盘；
   * 目标为系统 Download 目录（ExternalStorageProvider 树 URI），未授予
   * 「所有文件访问」时报 needs_all_files_access，前端据此引导授权；
   * 需 system:open 权限） */
  openDownloadDir(): Promise<void>
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
