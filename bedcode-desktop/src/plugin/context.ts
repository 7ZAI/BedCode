/**
 * Plugin Context
 *
 * 为每个插件创建 PluginContext 实例 — 插件访问宿主能力的唯一通道
 */

import type {
  PluginContext,
  PluginInfo,
  Disposable,
  CommandRegistry,
  TerminalAPI,
  SessionAPI,
  UIRegistry,
  EventAPI,
  StorageAPI,
  HttpAPI,
  I18nAPI,
  SidebarPanelDescriptor,
  ToolboxPageDescriptor,
  StatusBarItemDescriptor,
  InputExtensionDescriptor,
  TerminalToolbarItemDescriptor,
  TitleBarItemDescriptor,
  PageToolbarItemDescriptor,
  FileHandlerDescriptor,
  SettingsSectionDescriptor,
  PluginDialogOptions,
  PluginDialogHandle,
} from './types'
import { hasPermissionForApi } from './permission'
import * as pluginCmds from './commands'
import * as pluginEvents from './events'
import { getPluginRegistry } from './registry'
// 控制器放 SDK 包内（宿主/dev-shell 共用同一实现）；相对导入直连源码，
// 不依赖已发布的 dist 构建（packages 在 vite fs.allow 与 TS include 之外，
// 经相对路径显式拉入编译）
import { openGlobalDialog } from '../../packages/plugin-sdk-desktop/src/global-dialog'
// 终端窗口管理器（票 13）：`session.isTerminalOpen` 是同步查询，需静态导入；
// 其余三个方法沿用同一单例（模块级 windows Map，与宿主会话页共享状态）
import { useSessionWindows } from '@/composables/useSessionWindows'

/**
 * 会话中心插件 ID（终端窗口视图由该插件贡献；票 05 起宿主不留终端兜底）。
 * 改名票 06 将集中化宿主侧插件常量，此处与 TerminalWindowHostView 同值先行。
 */
const SESSION_PLUGIN_ID = 'com.bedcode.terminal-session'

/** 终端窗口域激活门禁：session 插件停用 / Error 后终端视图不可渲染，宿主不再
 *  降级代办——`openTerminal` / `closeTerminal` / `isTerminalOpen` 显性报错
 *  （同配对 / QR 退役后模式：插件未激活时宿主命令面显性报错，不做静默兜底）。 */
function requireSessionPluginActive(): void {
  if (!getPluginRegistry().isContributionActive(SESSION_PLUGIN_ID)) {
    throw new Error(`session plugin ${SESSION_PLUGIN_ID} is not active`)
  }
}

/**
 * 创建插件的 PluginContext
 *
 * 异步：先向宿主换取**本插件的前端通道令牌**（审计票 06）——插件面命令的身份由令牌绑定，
 * 参数里的 plugin_id 只作目标，宿主按令牌反查并校验一致性。令牌保存在本函数闭包里，
 * 不挂全局、不进 context 的公开字段（同 realm 理论上仍可窥探，见裁决 2 删掉 `isolated`
 * 的说明：前端不是隔离边界，本票关闭的是「自报 plugin_id」这条通道）。
 */
export async function createPluginContext(info: PluginInfo): Promise<PluginContext> {
  const disposables: Disposable[] = []
  const permissions = info.permissions
  /** 本插件的前端通道令牌（宿主签发；停用即回收，页面加载后重新签发） */
  const channelToken = await pluginCmds.pluginChannelToken(info.id)

  /** 快速失败：检查权限 */
  function requirePermission(apiMethod: string): void {
    if (!hasPermissionForApi(permissions, apiMethod)) {
      throw new Error(`Plugin ${info.id} lacks permission for ${apiMethod}`)
    }
  }

  // ==================== CommandRegistry ====================
  const commandHandlers = new Map<string, (...args: any[]) => any>()

  const commands: CommandRegistry = {
    register(id: string, handler: (...args: any[]) => any): Disposable {
      commandHandlers.set(id, handler)
      const disposable = {
        dispose() {
          commandHandlers.delete(id)
        },
      }
      disposables.push(disposable)
      return disposable
    },
    async execute(id: string, ...args: any[]): Promise<any> {
      // 先查找前端注册的本地命令
      const handler = commandHandlers.get(id)
      if (handler) {
        return handler(...args)
      }
      // 尝试调用 Rust 插件的 command（通过 plugin_invoke 路由）
      // WASM 插件 invoke_command 的约定与 manifest contributes.commands 一致，
      // 使用全名（如 "session.task.history-list"）；插件侧 `_ =>` 兜底按全名匹配，
      // 不能去前缀，否则落入 Unknown command（registry/命令面板/插件视图均传全名）
      try {
        return await pluginCmds.pluginInvoke(
          info.id,
          id,
          args.length === 1 ? args[0] : args,
          channelToken,
        )
      } catch (e) {
        // 保留底层错误信息，避免把真实失败原因（如 WASM trap、插件未激活）
        // 统一掩盖成 "Command not found"，便于定位问题。
        // 注意：Rust AppError 经 Tauri IPC 以纯字符串 reject（无 .message），需按类型提取
        const raw = e instanceof Error ? e.message : typeof e === 'string' ? e : ''
        const detail = raw ? ` (${raw})` : ''
        throw new Error(`Command not found: ${id}${detail}`)
      }
    },
  }

  // ==================== TerminalAPI ====================
  //
  // 票 08：`sendInput` **已退役**——宿主不再有「替插件把输入导流到 PTY」的桥
  // （`plugin_terminal_send_input` 注销）。插件写自己会话的输入走自家命令通道
  // （`context.commands.execute('session.input', {sessionId, data})`），提交行重建
  // 与任务域观察都在插件 WASM 内完成。留下的 `onOutput` / `onInput` 是**观察面**
  // （宿主终端渲染管道的输出投递与输入修饰链观察点），与写入面分属两侧。
  const terminal: TerminalAPI = {
    onOutput(handler: (sessionId: string, data: string) => void): Disposable {
      requirePermission('terminal.onOutput')
      const disposable = pluginEvents.on(info.id, 'terminal:output', handler as any)
      disposables.push(disposable)
      return disposable
    },
    onInput(handler: (sessionId: string, text: string) => string | null): Disposable {
      requirePermission('terminal.onInput')
      const disposable = pluginEvents.on(info.id, 'terminal:input', handler as any)
      disposables.push(disposable)
      return disposable
    },
  }

  // ==================== SessionAPI ====================
  //
  // 票 08：**数据面（`list` / `get`）已退役**。宿主不再有会话数据命令面
  // （`list_sessions` / `get_session` 连壳删除）；插件读自己的会话事实走自家
  // 命令通道（`context.commands.execute('session.list' | 'session.get')`，
  // 与互调 api 同实现）——比经宿主 context 转一手少一跳，也不再需要
  // 「宿主替插件读会话」这条通道。
  //
  // 留下的是**宿主窗口事实**（终端窗口的开关 / 存在性 / 初始网格）：窗口本体
  // 与字体测量都在宿主（spec D3），插件无法自行实现，属裁剪线允许的原语。
  const session: SessionAPI = {
    onStatusChange(handler: (event: any) => void): Disposable {
      requirePermission('session.onStatusChange')
      const disposable = pluginEvents.on(info.id, 'session:statusChange', handler)
      disposables.push(disposable)
      return disposable
    },
    async predictTerminalSize(): Promise<{ cols: number; rows: number } | null> {
      requirePermission('session.predictTerminalSize')
      // 窗口几何与字体测量都在宿主（终端窗口本体留宿主，spec D3）：字体大小取自
      // 宿主设置，widthRatio 取窗口创建规则，插件无需感知宿主设置形状
      const [{ computeDesktopInitialTerminalSize, TERMINAL_WINDOW_WIDTH_RATIO }, { useSettingsStore }] =
        await Promise.all([import('@/utils/terminalInitialSize'), import('@/stores/settings')])
      const fontSize = useSettingsStore().settings.ui.terminal_font_size
      return computeDesktopInitialTerminalSize(fontSize, {
        widthRatio: TERMINAL_WINDOW_WIDTH_RATIO,
      })
    },
    async openTerminal(target: { id: string; name: string }): Promise<boolean> {
      requirePermission('session.openTerminal')
      requireSessionPluginActive()
      return useSessionWindows().openTerminalWindow(target)
    },
    async closeTerminal(sessionId: string): Promise<void> {
      requirePermission('session.closeTerminal')
      requireSessionPluginActive()
      await useSessionWindows().closeTerminalWindow(sessionId)
    },
    isTerminalOpen(sessionId: string): boolean {
      requirePermission('session.isTerminalOpen')
      requireSessionPluginActive()
      // 同步查询：调用方（插件会话页）在打开窗口前决定是否显示就绪 loading，
      // 与宿主会话页原行为同口径
      return useSessionWindows().hasTerminalWindow(sessionId)
    },
  }

  // ==================== UIRegistry ====================
  const ui: UIRegistry = {
    registerSidebarPanel(panel: SidebarPanelDescriptor): Disposable {
      requirePermission('ui.registerSidebarPanel')
      const registry = getPluginRegistry()
      const disposable = registry.registerView(info.id, 'sidebar', panel)
      disposables.push(disposable)
      return disposable
    },
    registerToolboxPage(page: ToolboxPageDescriptor): Disposable {
      requirePermission('ui.registerToolboxPage')
      const registry = getPluginRegistry()
      const disposable = registry.registerView(info.id, 'toolbox', page)
      disposables.push(disposable)
      return disposable
    },
    registerPage(page: SidebarPanelDescriptor): Disposable {
      requirePermission('ui.registerPage')
      const registry = getPluginRegistry()
      // viewType 'page'：全量注册表可见（路由可渲染），sidebar/toolbox 投影不含
      // （不进侧边栏菜单）——供「页内二级页面 / 深链直达」场景（票 14 收尾）
      const disposable = registry.registerView(info.id, 'page', page)
      disposables.push(disposable)
      return disposable
    },
    registerStatusBarItem(item: StatusBarItemDescriptor): Disposable {
      requirePermission('ui.registerStatusBarItem')
      const registry = getPluginRegistry()
      const disposable = registry.registerStatusBarItem(info.id, item)
      disposables.push(disposable)
      return disposable
    },
    registerInputExtension(ext: InputExtensionDescriptor): Disposable {
      requirePermission('ui.registerInputExtension')
      const registry = getPluginRegistry()
      const disposable = registry.registerInputExtension(info.id, ext)
      disposables.push(disposable)
      return disposable
    },
    registerTerminalToolbarItem(item: TerminalToolbarItemDescriptor): Disposable {
      requirePermission('ui.registerTerminalToolbarItem')
      const registry = getPluginRegistry()
      const disposable = registry.registerTerminalToolbarItem(info.id, item)
      disposables.push(disposable)
      return disposable
    },
    registerTitleBarItem(item: TitleBarItemDescriptor): Disposable {
      requirePermission('ui.registerTitleBarItem')
      const registry = getPluginRegistry()
      const disposable = registry.registerTitleBarItem(info.id, item)
      disposables.push(disposable)
      return disposable
    },
    registerPageToolbarItem(item: PageToolbarItemDescriptor): Disposable {
      requirePermission('ui.registerPageToolbarItem')
      const registry = getPluginRegistry()
      const disposable = registry.registerPageToolbarItem(info.id, item)
      disposables.push(disposable)
      return disposable
    },
    registerFileHandler(handler: FileHandlerDescriptor): Disposable {
      requirePermission('ui.registerFileHandler')
      const registry = getPluginRegistry()
      const disposable = registry.registerFileHandler(info.id, handler)
      disposables.push(disposable)
      return disposable
    },
    registerSettingsSection(section: SettingsSectionDescriptor): Disposable {
      requirePermission('ui.registerSettingsSection')
      const registry = getPluginRegistry()
      const disposable = registry.registerSettingsSection(info.id, section)
      disposables.push(disposable)
      return disposable
    },
    showDialog(options: PluginDialogOptions): PluginDialogHandle {
      requirePermission('ui.showDialog')
      // context 对象在本函数尾部组装；此处惰性引用（showDialog 调用时已初始化），
      // 供内容组件 provide('pluginContext') 使用
      return openGlobalDialog({ ...options, pluginContext: context })
    },
  }

  // ==================== EventAPI ====================
  const events: EventAPI = {
    on(event: string, handler: (...args: any[]) => void): Disposable {
      const disposable = pluginEvents.on(info.id, event, handler)
      disposables.push(disposable)
      return disposable
    },
    emit(event: string, ...args: any[]): void {
      pluginEvents.emit(event, ...args)
    },
  }

  // ==================== StorageAPI ====================
  const storage: StorageAPI = {
    async get<T = any>(key: string): Promise<T | undefined> {
      const val = await pluginCmds.pluginStorageGet(info.id, key, channelToken)
      return val as T | undefined
    },
    async set(key: string, value: any): Promise<void> {
      return pluginCmds.pluginStorageSet(info.id, key, value, channelToken)
    },
    async delete(key: string): Promise<void> {
      return pluginCmds.pluginStorageDelete(info.id, key, channelToken)
    },
    async flush(): Promise<void> {
      // 存储是即时写入的，flush 为 no-op
    },
  }

  // ==================== HttpAPI ====================
  const http: HttpAPI = {
    registerEndpoint(path: string, handler): Disposable {
      requirePermission('http.registerEndpoint')
      const registry = getPluginRegistry()
      const disposable = registry.registerHttpEndpoint(info.id, path, handler)
      disposables.push(disposable)
      return disposable
    },
  }

  // 注：原 `system.revealInDir` 插件 API（+ `system:open` 权限）已退役——
  // 定位能力改为内核原语 `host-platform.reveal-in-dir`（ABI v22），插件在
  // WASM 侧经 SDK `platform_reveal_in_dir` 调用，不经前端 context。

  // ==================== I18nAPI ====================
  const i18n: I18nAPI = {
    getI18n(): any {
      return (window as any).__BEDCODE_SHARED__?.i18n
    },
    registerMessages(locale: string, messages: Record<string, unknown>): void {
      const hostI18n = (window as any).__BEDCODE_SHARED__?.i18n
      if (!hostI18n) return
      // 用插件 ID 作为命名空间前缀，避免 key 冲突
      const prefixed: Record<string, unknown> = {}
      for (const [key, value] of Object.entries(messages)) {
        prefixed[`${info.id}.${key}`] = value
      }
      // 直接合并新消息，vue-i18n 会自动与现有消息深度合并
      hostI18n.global.mergeLocaleMessage(locale, prefixed)
    },
    t(key: string, params?: Record<string, unknown>): string {
      const hostI18n = (window as any).__BEDCODE_SHARED__?.i18n
      if (!hostI18n) return key
      // 自动添加插件 ID 前缀
      const fullKey = `${info.id}.${key}`
      return hostI18n.global.t(fullKey, params)
    },
  }

  const context = {
    id: info.id,
    extensionPath: info.extensionPath,
    commands,
    terminal,
    session,
    ui,
    events,
    storage,
    http,
    i18n,
    _disposables: disposables,
  }
  return context
}
