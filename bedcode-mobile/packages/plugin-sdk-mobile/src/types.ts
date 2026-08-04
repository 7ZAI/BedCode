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
  /** 插件图标：emoji、内联 <svg> 标记或相对插件目录的图片路径，缺省时前端生成字母头像回退 */
  icon?: string
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

/** 日志 API */
export interface LoggerAPI {
  info(message: string): void
  debug(message: string): void
  warn(message: string): void
  error(message: string): void
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

// ==================== File Service API Types ====================

/** 上传策略钩子元信息（宿主 → 插件，与 SDK Rust UploadRequestMeta camelCase 对应） */
export interface UploadRequestMeta {
  /** 目标相对路径（相对挂载根） */
  relativePath: string
  /** 声明的文件大小（字节） */
  size: number
}

/** 上传策略钩子决定（插件 → 宿主；fail-closed 语义，异常一律拒绝） */
export interface UploadHookDecision {
  /** 是否允许上传 */
  allow: boolean
  /** 拒绝原因（如 duplicate-name），允许时为空 */
  reason?: string
}

/** 文件服务挂载选项（与 SDK Rust MountOptions camelCase 对应） */
export interface MountOptions {
  /** 挂载点名称（小写字母数字 -_），暴露为 /{pluginId}/{mountPath}/**（移动端无 /api 前缀） */
  mountPath: string
  /** 允许目录根（绝对路径，来自插件 storage 的用户配置） */
  roots: string[]
  /** 允许的操作集合（未声明的操作端点返回 403） */
  operations: ('list' | 'download' | 'upload')[]
  /** 上传策略钩子（可选；提供时以 Webview 钩子目标注册，上传会话创建时调用一次） */
  onUploadRequest?: (meta: UploadRequestMeta) => Promise<UploadHookDecision>
}

/** 挂载句柄（fileService.mount 返回值） */
export interface FileServiceMount {
  /** 挂载点名称 */
  mountPath: string
  /** 更新允许目录根（目录变更即时生效） */
  updateRoots(roots: string[]): Promise<void>
  /** 摘除挂载点（插件 deactivate 时应一并调用） */
  dispose(): Promise<void>
}

/** 对端挂载点信息（与 SDK Rust PeerMountAnnouncement camelCase 对应） */
export interface PeerMountAnnouncement {
  /** 挂载所属插件 ID（URL 第一段） */
  pluginId: string
  /** 挂载点名称（URL 第二段） */
  mountPath: string
  /** 该挂载支持的操作集合 */
  operations: ('list' | 'download' | 'upload')[]
}

/** 对端文件服务信息（与 SDK Rust PeerFileService camelCase 对应，控制面公告填充） */
export interface PeerFileServiceInfo {
  /** 对端 IP */
  ip: string
  /** 对端文件服务端口 */
  port: number
  /** 鉴权 Token（移动端服务为 Bearer Token；桌面端走 JWT 时为空） */
  token: string
  /** 对端挂载点列表 */
  mounts: PeerMountAnnouncement[]
}

/** 文件服务 API（需 fileservice 权限） */
export interface FileServiceAPI {
  /** 挂载文件服务端点（插件作为文件服务方），返回挂载句柄 */
  mount(options: MountOptions): Promise<FileServiceMount>
  /** 获取对端文件服务信息（对端 = 桌面端；未公告返回 null） */
  getPeerInfo(peerId: string): Promise<PeerFileServiceInfo | null>
  /** 弹出系统目录选择对话框（设置允许目录用；用户取消返回 null）。
   * 注意：Android/iOS 无目录选择能力，此方法会 reject（错误文案含 fall back 提示），
   * 插件应捕获后改用手动路径输入（如 dialogs.showPrompt） */
  pickDirectory(): Promise<string | null>
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
  /** 文件服务 API（需 fileservice 权限） */
  readonly fileService: FileServiceAPI
  readonly i18n: I18nAPI
  readonly lifecycle: LifecycleAPI
  readonly logger: LoggerAPI
  /** 对话框（弹窗扩展性） */
  readonly dialogs: DialogAPI
  /** 系统通知 */
  readonly notifications: NotificationAPI
  /** 生命周期状态上报（启用成功/失败） */
  readonly status: StatusAPI
  readonly _disposables: Disposable[]
}

/** 插件入口模块约定 */
export interface PluginModule {
  activate(context: PluginContext): Promise<void>
  deactivate?: () => Promise<void>
}
