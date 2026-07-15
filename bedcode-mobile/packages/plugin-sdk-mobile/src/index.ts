/**
 * @bedcode/plugin-sdk-mobile
 *
 * BedCode 移动端插件开发工具包
 */

export type {
  Disposable,
  PluginType,
  PluginState,
  PluginManifest,
  PluginConfiguration,
  ConfigProperty,
  MobilePluginContributes,
  LifecycleContribution,
  CommandContribution,
  ViewContribution,
  NavTabContribution,
  SettingsContribution,
  TerminalContribution,
  TerminalToolbarItemContribution,
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
  PluginContext,
  PluginModule,
} from './types'

export {
  getSharedModule,
  getI18n,
  getVue,
  getVueI18n,
  getPinia,
  getRouter,
  getPluginContext,
} from './runtime'
