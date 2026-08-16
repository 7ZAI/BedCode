/**
 * @bedcode/plugin-sdk-desktop
 *
 * BedCode 插件开发工具包 — 类型定义 + 运行时代理 + 构建工具
 */

// 类型导出
export type {
  Disposable,
  PluginType,
  PluginManifest,
  PluginConfiguration,
  ConfigProperty,
  PluginContributes,
  LifecycleContribution,
  CommandContribution,
  ViewContribution,
  TerminalContribution,
  ToolProviderContribution,
  FileHandlerContribution,
  SidebarPanelDescriptor,
  ToolboxPageDescriptor,
  StatusBarItemDescriptor,
  InputExtensionDescriptor,
  TerminalToolbarItemDescriptor,
  TitleBarItemDescriptor,
  FileHandlerDescriptor,
  RequestHandler,
  CommandRegistry,
  TerminalAPI,
  SessionAPI,
  UIRegistry,
  EventAPI,
  StorageAPI,
  HttpAPI,
  I18nAPI,
  UploadRequestMeta,
  UploadHookDecision,
  TransferRequestMeta,
  MountOptions,
  FileServiceMount,
  PeerMountAnnouncement,
  PeerFileServiceInfo,
  FileServiceAPI,
  SystemAPI,
  PluginContext,
  PluginModule,
  PluginState,
  PluginInfo,
} from './types'

// 运行时代理导出
export {
  getSharedModule,
  getI18n,
  getVue,
  getVueI18n,
  getPinia,
  getRouter,
  getPluginContext,
} from './runtime'

// 配置约定导出
export {
  PLUGIN_CONFIG_STORAGE_KEY,
  defineConfiguration,
} from './config'

// 事件名常量导出（与 Rust SDK constants.rs 同步）
export {
  EVENT_TASK_STATUS_CHANGED,
  EVENT_SESSION_MODE_CHANGED,
  EVENT_TASK_QUEUE_CHANGED,
} from './constants'
