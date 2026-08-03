/**
 * Mobile Plugin Types (Host)
 *
 * 基础类型从 @bedcode/plugin-sdk-mobile 导入
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
} from '@bedcode/plugin-sdk-mobile'

/** 插件信息（从后端获取，含 source 字段） */
export interface PluginInfo {
  id: string
  name: string
  version: string
  description: string
  author: string
  main: string
  pluginType: import('@bedcode/plugin-sdk-mobile').PluginType
  permissions: string[]
  state: import('@bedcode/plugin-sdk-mobile').PluginState
  contributes: import('@bedcode/plugin-sdk-mobile').MobilePluginContributes
  source: string
  /** 插件目录路径（含 plugin.json），前端经 asset protocol 加载前端模块 */
  extensionPath: string
}
