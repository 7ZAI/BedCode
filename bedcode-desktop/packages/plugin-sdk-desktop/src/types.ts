/**
 * @binblink/plugin-sdk-desktop 类型定义
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

/** file-transfer 对等领域种子（附近设备面板 / 后续首连确认、可信对端演示数据） */
export interface PeerDevMock {
  /**
   * 发现设备列表（宿主 DiscoveredPeerDto 的 camelCase 子集），
   * 须覆盖在线/未连接两态；fileTransfer=false 节点可见但不可连接
   */
  devices: Array<{
    nodeId: string
    deviceName: string
    addr?: string
    fileTransfer?: boolean
  }>
  /** 初始已连接节点 id（对端已确认的传输会话） */
  connectedNodeIds?: string[]
  /** 初始活跃对端 nodeId（应为 connectedNodeIds 之一） */
  activeNodeId?: string
  /** 拨号行为覆盖：nodeId → 终态；未列出的可传输节点按 unreachable 处理 */
  dialBehavior?: Record<string, 'connected' | 'denied' | 'unreachable'>
  /** 模拟握手耗时 ms（缺省 800） */
  dialLatencyMs?: number
  /**
   * 待确认首连请求种子（dev-shell 延迟逐条推送 consent-requested 事件，
   * 驱动确认弹窗与状态栏计数两路演示；缺省不演示）。多条种子可演示排队：
   * 第一条立即弹窗，后续进入队列待当前项结算后依次展示
   */
  consent?: Array<{
    requestId: string
    nodeId: string
    fingerprintShort: string
    /** 设备名；null 时弹窗以短指纹兑底 + 身份提示展示 */
    deviceName: string | null
  }>
}

// ==================== 传输域种子（file-transfer 任务/远端浏览/共享设置） ====================

/**
 * 传输任务种子内容（mock 组装完整 DTO：peer 取活跃对端设备、时间戳取当前）。
 * state 覆盖全部 8 态：queued / transferring / paused / resumable /
 * completed / cancelled / failed / rejected
 */
export interface TransferTaskSeed {
  id: string
  direction: 'download' | 'upload'
  /** 对端侧路径（共享根内相对路径） */
  remotePath: string
  /** 本机路径（上传任务；缺省空串） */
  localPath?: string
  size: number
  offset: number
  state: string
  reason?: string | null
}

/** 远端文件浏览种子：根清单 + 目录内容表（key = `${dirId}::${相对路径}`，根目录 path=''） */
export interface RemoteFsDevMock {
  roots: Array<{ id: string; name: string }>
  files?: Record<
    string,
    Array<{ name: string; size: number; mtime: number; isDir: boolean }>
  >
}

/** 本机共享设置种子（roots 为宿主 RootItem DTO 形状 {id, name}） */
export interface TransferSettingsDevMock {
  roots: Array<{ id: string; name: string }>
  downloadDir?: string
  concurrency?: number
}

/** file-transfer 传输域种子（任务列表 / 远端浏览 / 共享设置；缺省子域回退空态） */
export interface TransferDevMock {
  tasks?: TransferTaskSeed[]
  remoteFs?: RemoteFsDevMock
  settings?: TransferSettingsDevMock
}

/** 插件 dev-shell 演示数据（后续领域按需扩充子字段） */
export interface PluginDevMock {
  peer?: PeerDevMock
  /** 传输域（任务列表、远端文件浏览、本机共享设置） */
  transfer?: TransferDevMock
}

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
