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

// ==================== File Service API Types ====================

/** 上传策略钩子元信息（宿主 → 插件，与 SDK Rust UploadRequestMeta camelCase 对应） */
export interface UploadRequestMeta {
  /** 目标相对路径（相对挂载根） */
  relativePath: string
  /** 声明的文件大小（字节） */
  size: number
}

/** 批量传输请求元信息（宿主 → 插件批钩子入参，v2；与 SDK Rust TransferRequestMeta 对应） */
export interface TransferRequestMeta {
  /** 批 ID（发送方生成，跨端唯一标识一次「发送」动作） */
  batchId: string
  /** 批内文件清单（相对路径 + 大小） */
  files: { relativePath: string; size: number }[]
  /** 批内文件总大小（字节） */
  totalSize: number
}

/** 上传策略钩子决定（插件 → 宿主；fail-closed 语义，异常一律拒绝）
 *
 * v2 三路化：allow / ask（请求用户批准，批上下文）/ deny。
 * wire 兼容：旧插件返回 `{ allow: false }` → deny；`{ allow: true }` → allow。 */
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
  /** 挂载点名称（小写字母数字 -_），暴露为 /{pluginId}/{mountPath}/**（移动端无 /api 前缀） */
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

/** SAF 目录树条目（listTree 返回；真实路径条目列表复用，uri 承载绝对路径） */
export interface SafEntry {
  name: string
  isDir: boolean
  /** 文件大小（字节；目录/未知为 0） */
  size: number
  /** MIME 类型（可空串） */
  mime: string
  /** 条目 document URI（content://.../document/...；真实路径条目为绝对路径） */
  uri: string
  /** 条目 document id（子目录遍历用；真实路径条目为空串） */
  documentId: string
}

/** 中转复制启动结果 */
export interface SafCopyHandle {
  /** 复制句柄 id（copyStatus / copyCancel 用） */
  copyId: string
  /** cache 落盘绝对路径（复制完成后即 enqueue 的 localPath） */
  destPath: string
}

/** 中转复制进度快照（「准备中」进度条数据源） */
export interface SafCopyStatus {
  copyId: string
  /** 已复制字节数 */
  done: number
  /** 总字节数（未知大小（流式 provider）为 0） */
  total: number
  /** 复制是否已结束（成功/失败/取消三者其一） */
  finished: boolean
  /** 是否被用户取消 */
  cancelled: boolean
  /** 失败原因（仅失败时非空） */
  error: string | null
  /** cache 落盘绝对路径 */
  destPath: string
}

/** 系统目录树选择结果（添加共享目录条目用；Kotlin SafPickerPlugin 返回） */
export interface PickedSharedDirectory {
  /** content://tree URI（条目 id） */
  uri: string
  /** 树根 document id（子目录遍历起点） */
  documentId: string
  /** 目录展示名 */
  displayName: string
}

/** SAF 存储访问 API（需 fileservice 权限；非 Android 平台 reject） */
export interface SafAPI {
  /** 列出目录树子条目（共享目录 App 内遍历，免系统选择器） */
  listTree(treeUri: string, documentId: string): Promise<SafEntry[]>
  /** 启动中转复制（Relay Copy）：SAF 源 → app 私有 cache，立即返回句柄 */
  copyStart(uri: string, destName: string): Promise<SafCopyHandle>
  /** 轮询中转复制进度 */
  copyStatus(copyId: string): Promise<SafCopyStatus>
  /** 取消中转复制（复制方删除半成品后结束，无残留） */
  copyCancel(copyId: string): Promise<void>
  /** 清扫中转复制残留（插件激活时调用，删除缓存 staging 目录全部文件） */
  cleanupStaleCopies(): Promise<void>
  /** 检测树授权是否仍有效（失效标记 → 提示重新授权） */
  checkAuthorized(treeUri: string): Promise<boolean>
}

/** 文件服务 API（需 fileservice 权限） */
export interface FileServiceAPI {
  /** 挂载文件服务端点（插件作为文件服务方），返回挂载句柄 */
  mount(options: MountOptions): Promise<FileServiceMount>
  /** 获取对端文件服务信息（对端 = 桌面端；未公告返回 null） */
  getPeerInfo(peerId: string): Promise<PeerFileServiceInfo | null>
  /** v2：批准传输批（接收端应答「接受全部」） */
  approveTransferRequest(batchId: string): Promise<void>
  /** v2：拒绝传输批（接收端应答「拒绝全部」） */
  rejectTransferRequest(batchId: string): Promise<void>
  /** v2：设置批准超时（秒，10–600） */
  setApprovalTimeout(mountPath: string, seconds: number): Promise<void>
  /** v2：取消接收中的上传会话（本地取消） */
  cancelReceivingSession(sessionId: string): Promise<void>
  /** 弹出系统目录选择对话框（设置允许目录用；用户取消返回 null）。
   * Android 使用 SAF 目录树选择器并解析为真实路径；不支持的 provider
   * （云盘/SD 卡等）或 iOS 会 reject，插件应捕获后改用手动路径输入（如 dialogs.showPrompt） */
  pickDirectory(): Promise<string | null>
  /** 弹出系统文件选择对话框（上传本地文件用；用户取消返回 null）。
   * Android 使用 SAF 文件选择器并解析为真实路径；不支持的 provider 或 iOS 会 reject，
   * 插件应捕获后改用手动路径输入 */
  pickFile(): Promise<string | null>
  /** 弹系统目录树选择器，返回 SAF 树元数据（添加共享目录条目用；
   * 持久化授权由宿主完成，重启仍有效；用户取消返回 null；非 Android 平台 reject） */
  pickSharedDirectory(): Promise<PickedSharedDirectory | null>
  /** 列出真实路径目录条目（免授权特殊条目「app 私有下载目录」浏览用；
   * 仅允许该目录及其子目录；非 Android 平台 reject） */
  listDir(path: string): Promise<SafEntry[]>
  /** SAF 存储访问（共享目录遍历 + 中转复制；非 Android 平台 reject） */
  readonly saf: SafAPI
  /** 引导授予「所有文件访问权限」（Android 11+ 分区存储下，非媒体集合的顶层
   * 自定义目录 read_dir 会被 FUSE 过滤为空，需该权限才能经真实路径读取；
   * 无运行时弹窗，宿主跳转系统授权页）。返回当前是否已授权；非 Android 平台 reject */
  requestAllFilesAccess(): Promise<boolean>
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

/** SAF 目录树条目（dev-shell safTree 用；docId 为子目录遍历 key） */
export interface SafTreeEntry {
  name: string
  isDir: boolean
  /** 文件大小（字节；目录/未知为 0） */
  size: number
  /** MIME 类型（可空串） */
  mime: string
  /** 子目录遍历 key（对应 safTree 下一级键；目录条目必填，文件条目忽略） */
  docId: string
}

/** 免授权真实路径目录浏览条目种子（dev-shell listDir 用；uri/documentId 由 mock 宿主拼装） */
export type SafEntrySeed = Omit<SafEntry, 'uri' | 'documentId'>

/**
 * 插件开发期领域数据：dev-shell mock 宿主按 pluginId 合并（仅浏览器 dev 环境消费）
 *
 * 与"宿主能力 mock"（会话/对话框/事件/HTTP 接口等，固定在 dev-shell 内实现）
 * 区分：本协议只承载各插件自己的业务演示数据，由插件入口导出 devMock，
 * dev-shell 加载插件时经 registry 注册、createMockContext 按需取用。
 * 真实宿主忽略该字段（多余导出对 activate 无影响），插件无需条件编译。
 */
export interface PluginDevMock {
  /** 任务队列种子（auto-task：mobileApi 初始队列项，localStorage 无缓存时使用） */
  queueSeed?: MobileQueueTaskItem[]
  /** 免授权真实路径目录浏览条目（file-transfer：fileService.listDir 的返回） */
  listDirEntries?: SafEntrySeed[]
  /** SAF 目录树（file-transfer：documentId → 条目，saf.listTree 遍历用） */
  safTree?: Record<string, SafTreeEntry[]>
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
