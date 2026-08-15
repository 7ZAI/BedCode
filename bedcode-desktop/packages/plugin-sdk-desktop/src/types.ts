/**
 * @bedcode/plugin-sdk-desktop 类型定义
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
}

/** UI 注册表（需 ui:* 权限） */
export interface UIRegistry {
  registerSidebarPanel(panel: SidebarPanelDescriptor): Disposable
  registerToolboxPage(page: ToolboxPageDescriptor): Disposable
  registerStatusBarItem(item: StatusBarItemDescriptor): Disposable
  registerInputExtension(ext: InputExtensionDescriptor): Disposable
  registerTerminalToolbarItem(item: TerminalToolbarItemDescriptor): Disposable
  registerTitleBarItem(item: TitleBarItemDescriptor): Disposable
  registerFileHandler(handler: FileHandlerDescriptor): Disposable
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

// ==================== File Service API Types ====================

/** 上传策略钩子元信息（宿主 → 插件，与 SDK Rust UploadRequestMeta camelCase 对应） */
export interface UploadRequestMeta {
  /** 目标相对路径（相对挂载根） */
  relativePath: string
  /** 声明的文件大小（字节） */
  size: number
}

/** 批量传输请求元信息（v2 批钩子入参，与 SDK Rust TransferRequestMeta camelCase 对应） */
export interface TransferRequestMeta {
  /** 批 ID（UUID，发送方生成） */
  batchId: string
  /** 批内文件清单 */
  files: UploadRequestMeta[]
  /** 批总大小（字节） */
  totalSize: number
}

/** 上传策略钩子决定（插件 → 宿主；fail-closed 语义，异常一律拒绝）
 * v2 三路化：allow / ask / deny（wire 兼容旧 `{ allow, reason }`） */
export interface UploadHookDecision {
  /** 是否允许上传 */
  allow: boolean
  /** v2：true = 需要用户批准（批上下文）；与 allow 互斥 */
  ask?: boolean
  /** 拒绝原因（如 duplicate-name / policy-denied），允许时为空 */
  reason?: string
}

/** 文件服务挂载选项（与 SDK Rust MountOptions camelCase 对应） */
export interface MountOptions {
  /** 挂载点名称（小写字母数字 -_），暴露为 /api/plugins/{pluginId}/{mountPath}/** */
  mountPath: string
  /** 允许目录根（绝对路径，来自插件 storage 的用户配置） */
  roots: string[]
  /** 允许的操作集合（未声明的操作端点返回 403） */
  operations: ('list' | 'download' | 'upload')[]
  /** 上传策略钩子（可选；提供时以 Webview 钩子目标注册，上传会话创建时调用一次） */
  onUploadRequest?: (meta: UploadRequestMeta) => Promise<UploadHookDecision>
  /** v2：批量传输请求钩子（可选；提供时以 Webview 批钩子目标注册，POST /transfer-request 时调用一次） */
  onTransferRequest?: (meta: TransferRequestMeta) => Promise<UploadHookDecision>
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

/** 对端文件服务信息（与 SDK Rust PeerFileService 对应，控制面公告填充） */
export interface PeerFileServiceInfo {
  /** 对端 IP */
  ip: string
  /** 对端文件服务端口 */
  port: number
  /** 鉴权 Token（移动端服务为 Bearer Token；桌面端走 JWT 时为空） */
  token: string
  /** 对端真实设备名（用户设置名，获取不到时为兜底名；wire 为 snake_case） */
  device_name: string
  /** 对端挂载点列表 */
  mounts: PeerMountAnnouncement[]
}

/** 文件服务 API（需 fileservice 权限） */
export interface FileServiceAPI {
  /** 挂载文件服务端点（插件作为文件服务方），返回挂载句柄 */
  mount(options: MountOptions): Promise<FileServiceMount>
  /** 获取对端文件服务信息（对端 = 移动端；未公告返回 null） */
  getPeerInfo(peerId: string): Promise<PeerFileServiceInfo | null>
  /** 弹出系统目录选择对话框（设置允许目录用；用户取消返回 null） */
  pickDirectory(): Promise<string | null>
  /** 弹出系统多文件选择对话框（上传方向用；用户取消返回空数组） */
  pickFiles(): Promise<string[]>
  /** v2：批准传输批（接收端应答「接受全部」） */
  approveTransferRequest(batchId: string): Promise<void>
  /** v2：拒绝传输批（接收端应答「拒绝全部」） */
  rejectTransferRequest(batchId: string): Promise<void>
  /** v2：设置批准超时（秒，10–600；仅 ask 策略生效，宿主 TTL 扫描用） */
  setApprovalTimeout(mountPath: string, seconds: number): Promise<void>
  /** v2：取消接收中的上传会话（接收端本地取消，session 级） */
  cancelReceivingSession(sessionId: string): Promise<void>
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
  registerMessages(locale: string, messages: Record<string, any>): void
  /** 翻译快捷方法（自动添加插件 ID 前缀） */
  t(key: string, params?: Record<string, any>): string
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
  /** 文件服务 API（需 fileservice 权限） */
  readonly fileService: FileServiceAPI
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

/** 插件运行时状态 */
export type PluginState =
  | { state: 'Loaded' }
  | { state: 'Activated' }
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
