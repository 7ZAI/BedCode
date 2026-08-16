/**
 * File Transfer 插件入口
 *
 * 内网文件传输 — 侧边栏面板
 * cdylib 插件架构：Rust 后端处理传输逻辑，前端通过 PluginContext 调用
 */
import FileTransferView from './components/FileTransferView.vue'
import { messages } from './i18n'
import styles from './styles.css?inline'
import { watch } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-desktop'

// ==================== UI 注册（标题随宿主语言切换重注册） ====================

let sidebarDisposable: { dispose(): void } | null = null
let stopLocaleWatch: (() => void) | null = null

/**
 * 注册侧边栏面板
 *
 * 注册时标题被宿主静态捕获（labelKey 非 i18n key，不随 vue-i18n 自动更新），
 * 语言切换时先释放旧注册再重新注册，菜单显示文本即时刷新。
 */
function registerPluginUi(context: PluginContext) {
  sidebarDisposable?.dispose()

  sidebarDisposable = context.ui.registerSidebarPanel({
    id: 'file-transfer.sidebar',
    title: context.i18n.t('transfer.sidebar.title'),
    // 菜单排序：紧跟 agent 任务（auto-task 210）之后，位于服务器（内置 300）之前
    order: 220,
    icon: 'M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z',
    component: FileTransferView,
  })
}

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

  // 注册侧边栏面板（标题随宿主语言切换重注册，见 registerPluginUi）
  registerPluginUi(context)

  // 宿主语言切换时重注册面板：标题在注册时被静态捕获，不随 vue-i18n 自动更新，
  // 需监听 locale 变化后重新注册刷新菜单/路由显示文本
  const hostI18n = context.i18n.getI18n()
  stopLocaleWatch = watch(
    () => hostI18n?.global?.locale?.value,
    () => registerPluginUi(context),
  )

  console.log('[File Transfer] Plugin activated (wasm mode)')
}

export async function deactivate(): Promise<void> {
  stopLocaleWatch?.()
  console.log('[File Transfer] Plugin deactivated')
}
