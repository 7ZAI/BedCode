/**
 * Mobile Plugin Types (Host)
 *
 * 基础类型从 @binblink/plugin-sdk-mobile 导入
 * 仅保留宿主运行时特有类型
 */

export type {
  Disposable,
  PluginType,
  PluginState,
  PluginManifest,
  MobilePluginContributes,
  LifecycleContribution,
  CommandContribution,
  ViewContribution,
  NavTabContribution,
  SettingsContribution,
  TerminalContribution,
  TerminalToolbarItemContribution,
  PluginConfiguration,
  ConfigProperty,
  ToolboxPageDescriptor,
  NavTabDescriptor,
  TerminalToolbarItemDescriptor,
  SettingsSectionDescriptor,
  PluginRouteDescriptor,
  RouteContribution,
  CommandRegistry,
  TerminalAPI,
  SessionAPI,
  UIRegistry,
  EventAPI,
  StorageAPI,
  I18nAPI,
  LifecycleAPI,
  LoggerAPI,
  DialogAPI,
  DialogOptions,
  DialogResult,
  NotificationAPI,
  StatusAPI,
  PluginContext,
  PluginModule,
  UploadRequestMeta,
  UploadHookDecision,
  TransferRequestMeta,
  MountOptions,
  FileServiceMount,
  PeerMountAnnouncement,
  PeerFileServiceInfo,
  FileServiceAPI,
  SystemAPI,
  SafEntry,
  SafCopyHandle,
  SafCopyStatus,
  PickedSharedDirectory,
} from '@binblink/plugin-sdk-mobile'

/** 插件信息（从后端获取，含 source 字段） */
export interface PluginInfo {
  id: string
  name: string
  version: string
  description: string
  author: string
  main: string
  pluginType: import('@binblink/plugin-sdk-mobile').PluginType
  permissions: string[]
  state: import('@binblink/plugin-sdk-mobile').PluginState
  contributes: import('@binblink/plugin-sdk-mobile').MobilePluginContributes
  source: string
  /** 插件目录路径（含 plugin.json），前端经 asset protocol 加载前端模块 */
  extensionPath: string
  /** 插件图标：emoji 或相对插件目录的图片路径，缺省时前端生成字母头像回退 */
  icon?: string
  /** 插件目录总大小（字节） */
  sizeBytes: number
  /** 安装时间（unix 毫秒），内置插件可能为 null */
  installedAt?: number
}
