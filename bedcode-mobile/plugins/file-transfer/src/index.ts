/**
 * File Transfer 插件入口 (Mobile)
 *
 * activate：
 *   1. 注册 i18n 消息（key 自动加插件 id 前缀）
 *   2. 注入插件全局样式（宿主不加载插件 dist/style.css，运行时注入一次）
 *   3. 注册工具箱视图（component=浏览主页面 + entry=入口卡片带状态角标）
 *   4. 注册设置区（宿主 registerSettingsSection；同时插件内设置页复用）
 *
 * 事件监听由组件内的 useTasks.start() 注册（入口卡与浏览页各自启动），
 * 组件卸载/插件停用时经 context._disposables 与组件 onUnmounted 清理。
 */
import FileTransferView from './components/FileTransferView.vue'
import ToolboxEntry from './components/ToolboxEntry.vue'
import SettingsSection from './components/SettingsSection.vue'
import { messages } from './i18n'
import styles from './styles.css?inline'
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'

const STYLE_ID = 'file-transfer-plugin-style'

export async function activate(context: PluginContext): Promise<void> {
  // 1. 注册 i18n 消息（必须在组件 setup 前完成，保证模板取文案可用）
  for (const [locale, msgs] of Object.entries(messages)) {
    context.i18n.registerMessages(locale, msgs)
  }

  // 2. 注入插件样式（幂等，热重载不重复插入）
  if (!document.getElementById(STYLE_ID)) {
    const styleEl = document.createElement('style')
    styleEl.id = STYLE_ID
    styleEl.textContent = styles
    document.head.appendChild(styleEl)
  }

  // 3. 工具箱视图：component 为浏览主页面，entry 为带状态角标的入口卡片
  context.ui.registerToolboxPage({
    id: 'file-transfer.toolbox',
    title: context.i18n.t('transfer.toolbox.title'),
    icon: 'M8 7h12m0 0l-4-4m4 4l-4 4m0 6H4m0 0l4 4m-4-4l4-4',
    component: FileTransferView,
    entry: ToolboxEntry,
  })

  // 4. 设置区贡献（宿主 registry 支持；当前 SettingsView 尚未渲染插件设置区，
  //    插件内 FileTransferView 顶部齿轮亦入口 SettingsSection）
  context.ui.registerSettingsSection({
    id: 'file-transfer.settings',
    pluginId: context.id,
    section: 'file-transfer',
    component: SettingsSection,
  })

  context.logger.info('File Transfer plugin activated (wasm mode, mobile)')
}

export async function deactivate(): Promise<void> {
  // 样式保留（幂等），组件级监听已在卸载时清理；
  // 注册表/事件由宿主 loader 依据 context._disposables 统一摘除
  console.log('[File Transfer] Plugin deactivated')
}
