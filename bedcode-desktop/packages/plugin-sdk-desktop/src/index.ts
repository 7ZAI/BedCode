/**
 * @binblink/bedcode-plugin-sdk-desktop
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
  PageToolbarItemDescriptor,
  FileHandlerDescriptor,
  PluginDialogAction,
  PluginDialogOptions,
  PluginDialogHandle,
  RequestHandler,
  CommandRegistry,
  TerminalAPI,
  SessionAPI,
  UIRegistry,
  EventAPI,
  StorageAPI,
  HttpAPI,
  I18nAPI,
  SystemAPI,
  PluginContext,
  PluginModule,
  PluginDevMock,
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

// 全局弹窗控制器导出（宿主 / dev-shell 的 ui.showDialog 实现与渲染组件订阅）
export {
  openGlobalDialog,
  closeGlobalDialog,
  updateGlobalDialog,
  getGlobalDialog,
  subscribeGlobalDialog,
  resolveDialogDeadline,
} from './global-dialog'
export type {
  GlobalDialogEntry,
  GlobalDialogListener,
  OpenGlobalDialogInput,
  GlobalDialogHandleOutput,
} from './global-dialog'

// 配置约定导出
export { PLUGIN_CONFIG_STORAGE_KEY, defineConfiguration } from './config'
