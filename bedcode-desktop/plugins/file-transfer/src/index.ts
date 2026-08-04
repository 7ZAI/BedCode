/**
 * File Transfer 插件入口
 *
 * 内网文件传输 — 侧边栏面板
 * cdylib 插件架构：Rust 后端处理传输逻辑，前端通过 PluginContext 调用
 */
import FileTransferView from './components/FileTransferView.vue'
import { messages } from './i18n'
import styles from './styles.css?inline'
import type { PluginContext } from '@bedcode/plugin-sdk-desktop'

export async function activate(context: PluginContext): Promise<void> {
  // 注册 i18n 消息（自动添加插件 ID 前缀 → com.bedcode.file-transfer.transfer.*），
  // 必须在组件 setup 前完成，保证模板取文案可用
  for (const [locale, msgs] of Object.entries(messages)) {
    context.i18n.registerMessages(locale, msgs)
  }

  // 注入插件样式：宿主只加载插件 dist/index.js，SFC 样式与独立 CSS 文件均不会
  // 生效，故运行时注入一次（幂等，插件热重载不重复插入）
  if (!document.getElementById('file-transfer-plugin-style')) {
    const styleEl = document.createElement('style')
    styleEl.id = 'file-transfer-plugin-style'
    styleEl.textContent = styles
    document.head.appendChild(styleEl)
  }

  // 注册侧边栏面板（标题经 i18n 解析，随宿主语言切换）
  context.ui.registerSidebarPanel({
    id: 'file-transfer.sidebar',
    title: context.i18n.t('transfer.sidebar.title'),
    icon: 'M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z',
    component: FileTransferView,
  })

  console.log('[File Transfer] Plugin activated (wasm mode)')
}

export async function deactivate(): Promise<void> {
  console.log('[File Transfer] Plugin deactivated')
}
