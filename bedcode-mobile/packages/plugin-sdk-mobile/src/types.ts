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

/** 任务队列项（对端桌面端 AutoTask 插件） */
export interface MobileQueueTaskItem {
  id: string
  prompt: string
  position: number
  status: string
  created_at: string
}

/** 移动端宿主连接/HTTP 能力（共享运行时 mobileApi 模块）
 *
 * 经宿主 shared-runtime 暴露，供插件访问当前活动会话与对端桌面端 REST API。
 * 队列接口为 AutoTask 插件专属端点（/api/plugin/com.bedcode.auto-task/...）。
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
  /** 查询任务队列 */
  httpTaskQueueList(sessionId: string): Promise<MobileHttpResult<{
    session_id: string
    tasks: MobileQueueTaskItem[]
    queue_count: number
    /** 当前活动任务（waiting/executing 最前一项；无活动任务时为 null） */
    active_task: (MobileQueueTaskItem & { source?: string }) | null
  }>>
  /** 添加任务到队列 */
  httpTaskQueueAdd(sessionId: string, prompt: string): Promise<MobileHttpResult>
  /** 从队列删除任务 */
  httpTaskQueueRemove(sessionId: string, taskId: string): Promise<MobileHttpResult>
  /** 取消活动队列项（waiting / executing） */
  httpTaskQueueCancel(sessionId: string, taskId: string): Promise<MobileHttpResult>
  /** 清空任务队列 */
  httpTaskQueueClear(sessionId: string): Promise<MobileHttpResult>
  /** 更新队列任务内容 */
  httpTaskQueueUpdate(sessionId: string, taskId: string, prompt: string): Promise<MobileHttpResult>
  /** 重排序任务队列 */
  httpTaskQueueReorder(sessionId: string, taskIds: string[]): Promise<MobileHttpResult>
  /** 查询会话设置（auto_execute / auto_answer） */
  httpSessionSettings(sessionId: string): Promise<MobileHttpResult<{
    session_id: string
    auto_execute: boolean
    auto_answer: boolean
  }>>
  /** 设置会话自动模式 */
  httpSetSessionMode(sessionId: string, autoExecute?: boolean, autoAnswer?: boolean): Promise<MobileHttpResult>
  /** 查询会话当前任务 */
  httpCurrentTask(sessionId: string): Promise<MobileHttpResult<{
    session_id: string
    task: {
      id: string
      description: string | null
      status: string
      auto_approve: number
      created_at: string
    } | null
  }>>
  /** 查询 auto-task 支持的 agent 列表 */
  httpListSupportedAgents(): Promise<MobileHttpResult<{ agents: string[] }>>
  /**
   * 查询任务历史列表（分页 + 筛选）
   *
   * 只拼接已提供的筛选参数；返回 { tasks, total, limit, offset }，
   * 时间字段为 UTC `YYYY-MM-DD HH:MM:SS` 字符串，需前端自行转本地时区。
   */
  httpTaskHistoryList(params?: {
    status?: string
    agent?: string
    source?: string
    since?: string
    until?: string
    limit?: number
    offset?: number
  }): Promise<MobileHttpResult<{
    tasks: {
      id: string
      description: string | null
      status: string
      agent: string | null
      source: string | null
      session_id: string
      claude_sid: string | null
      working_dir: string | null
      auto_approve: number
      exit_reason: string | null
      created_at: string
      started_at: string | null
      completed_at: string | null
      input_tokens: number | null
      output_tokens: number | null
    }[]
    total: number
    limit: number
    offset: number
  }>>
  /** 查询定时任务列表（返回 { jobs }） */
  httpScheduledJobsList(): Promise<MobileHttpResult<{
    jobs: {
      id: string
      name: string | null
      config_id: string
      trigger_at: string
      prompts: string
      status: string
      session_id: string | null
      created_at: string
      executed_at: string | null
      error: string | null
    }[]
  }>>
  /**
   * 创建定时任务
   *
   * trigger_at 为 UTC `YYYY-MM-DD HH:MM:SS`；prompts 为任务 prompt 数组。
   * 后端 400 时 message 含具体缺失字段。
   */
  httpScheduledJobCreate(body: {
    name?: string
    config_id: string
    trigger_at: string
    prompts: string[]
  }): Promise<MobileHttpResult<{ job_id: string }>>
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

// ==================== OCR API Types ====================

/** 文本行包围盒（原图坐标系，Kotlin 解码尺寸） */
export interface OcrBBox {
  x: number
  y: number
  w: number
  h: number
}

/** 单行识别结果 */
export interface OcrLine {
  text: string
  /** rec 模型逐行置信度（0~1），低置信度行 UI 可弱化 */
  confidence: number
  bbox: OcrBBox
}

/** `ocr.recognize` 响应 */
export interface OcrResult {
  engine: string
  durationMs: number
  lines: OcrLine[]
}

/** `ocr.engineStatus` 响应 */
export interface OcrEngineStatus {
  /** 当前 ABI 是否含 onnxruntime + 引擎编译可用 */
  available: boolean
  modelsPresent: boolean
  modelsBytes: number
  /** 识别引擎是否已加载常驻 */
  engineLoaded: boolean
  /** v1 恒 ["offline"] */
  supportedEngines: string[]
}

/** 取图解码产物（pickImage / cameraCapture 返回；用户取消 = null） */
export interface OcrImageSource {
  /** RGBA8 纯像素文件路径（app cache；识别完成后宿主自动清理） */
  path: string
  width: number
  height: number
}

/** OCR API（需 ocr 权限；识别数据不经 WASM，宿主命令直供，见 .scratch/ocr-plugin/spec.md §4.2） */
export interface OcrApi {
  /** 识别图片：RGBA 由 Kotlin 桥产出，engine 缺省 "offline" */
  recognize(input: {
    engine?: string
    image: { rgbaPath: string; width: number; height: number }
    maxSide?: number
  }): Promise<OcrResult>
  /** 引擎状态：模型是否就位/占用字节/引擎加载态/支持引擎列表 */
  engineStatus(): Promise<OcrEngineStatus>
  /** 删除已解压模型（释放空间；先释放常驻引擎 session） */
  deleteModels(): Promise<{ deleted: boolean; freedBytes: number }>
  /** 从 APK assets 恢复模型（幂等） */
  restoreModels(): Promise<{ restored: boolean }>
  /** 相册选图（SAF image/*，零权限）→ RGBA8 临时文件；取消返回 null；非 Android 平台 reject */
  pickImage(): Promise<OcrImageSource | null>
  /** 拍照（CAMERA 运行时权限；拒绝 reject 明确错误）→ RGBA8 临时文件；取消返回 null；非 Android 平台 reject */
  cameraCapture(): Promise<OcrImageSource | null>
}

/** 系统 API — 宿主 OS 级文件操作（需 system:open 权限） */
export interface SystemAPI {
  /** 用系统查看器打开本地文件（传输完成「打开本地文件」；Android ACTION_VIEW） */
  openFile(path: string, displayName?: string): Promise<void>
  /** 用系统文件管理器打开文件所在目录（历史记录「打开所在文件夹」；
   * Android FileProvider 暴露父目录 + ACTION_VIEW，需 system:open 权限） */
  revealInDir(path: string): Promise<void>
}

// ==================== 插件开发期领域数据（dev-shell mock 协议） ====================

/** OCR 识别结果种子（dev-shell ocr.recognize 用；缺省时 mock 宿主返回内置示例行） */
export type OcrLinesSeed = OcrLine[]

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
}

/**
 * 插件开发期领域数据：dev-shell mock 宿主按 pluginId 合并（仅浏览器 dev 环境消费）
 *
 * 与"宿主能力 mock"（会话/对话框/事件/HTTP 接口等，固定在 dev-shell 内实现）
 * 区分：本协议只承载各插件自己的业务演示数据，由插件入口导出 devMock，
 * dev-shell 加载插件时经 registry 注册、createMockContext 按需取用。
 * 真实宿主忽略该字段（多余导出对 activate 无影响），插件无需条件编译。
 */
/** 远端文件浏览种子：根清单 + 目录内容表（key = `${dirId}::${相对路径}`，根目录 path=''） */
export interface RemoteFsDevMock {
  roots: Array<{ id: string; name: string }>
  files?: Record<
    string,
    Array<{ name: string; size: number; mtime: number; isDir: boolean }>
  >
}

/** 本机共享设置种子（roots 为宿主 wire DTO 形状，含 SAF tree_uri） */
export interface TransferSettingsDevMock {
  roots?: Array<{ id: string; name: string; tree_uri: string; builtin?: boolean }>
  /** 缺省 'ask'（协议枚举缺省值） */
  policyMode?: string
  askTimeoutSec?: number
  downloadDir?: string
}

/** file-transfer 传输域种子（远端浏览 / 共享设置；缺省子域回退空态） */
export interface TransferDevMock {
  remoteFs?: RemoteFsDevMock
  settings?: TransferSettingsDevMock
}

export interface PluginDevMock {
  /** 任务队列种子（auto-task：mobileApi 初始队列项，localStorage 无缓存时使用） */
  queueSeed?: MobileQueueTaskItem[]
  /** OCR 识别结果种子（ocr：ocr.recognize 的 mock 返回；空数组演示空结果空态） */
  ocrLinesSeed?: OcrLinesSeed
  /** file-transfer 对等子数据（附近设备面板演示态；见 PeerDevMock） */
  peer?: PeerDevMock
  /** file-transfer 传输域（远端文件浏览、本机共享设置） */
  transfer?: TransferDevMock
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
  /** OCR 引擎 API（需 ocr 权限；识别数据不经 WASM） */
  readonly ocr: OcrApi
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
