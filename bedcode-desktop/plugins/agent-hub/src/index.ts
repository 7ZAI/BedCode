/**
 * Agent Hub 插件入口
 *
 * 侧边栏面板（变体 B）— cdylib 插件架构：Rust 后端处理探测/后续业务，
 * 前端经 PluginContext 调用
 */
import AgentHubView from './components/AgentHubView.vue'
import { messages } from './i18n'
import styles from './styles.css?inline'
import { watch } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import hubDevMock from './devMock'

// dev-shell 领域种子数据（SDK PluginDevMock 协议；真实宿主忽略）
export const devMock = hubDevMock

// ==================== UI 注册（标题随宿主语言切换重注册） ====================

let sidebarDisposable: { dispose(): void } | null = null
let stopLocaleWatch: (() => void) | null = null

/**
 * 注册侧边栏面板
 *
 * 注册时标题被宿主静态捕获（labelKey 非 i18n key，不随 vue-i18n 自动更新），
 * 语言切换时先释放旧注册再重新注册，菜单显示文本即时刷新。
 * 排序：紧跟 file-transfer（220）之后，位于服务器（内置 300）之前。
 */
function registerPluginUi(context: PluginContext) {
  sidebarDisposable?.dispose()

  sidebarDisposable = context.ui.registerSidebarPanel({
    id: 'agent-hub.sidebar',
    title: context.i18n.t('hub.sidebar.title'),
    order: 240,
    icon: 'M12 2L2 12l10 10 10-10L12 2zm0 5.2l4.8 4.8-4.8 4.8L7.2 12l4.8-4.8z',
    component: AgentHubView,
  })
}

export async function activate(context: PluginContext): Promise<void> {
  // 注册 i18n 消息（自动添加插件 ID 前缀 → com.bedcode.agent-hub.hub.*），
  // 必须在组件 setup 前完成，保证模板取文案可用
  for (const [locale, msgs] of Object.entries(messages)) {
    context.i18n.registerMessages(locale, msgs)
  }

  // 注入插件样式：宿主只加载插件 dist/index.js，SFC 样式与独立 CSS 文件均不会
  // 生效，故运行时注入一次（幂等，插件热重载不重复插入）
  if (!document.getElementById('agent-hub-plugin-style')) {
    const styleEl = document.createElement('style')
    styleEl.id = 'agent-hub-plugin-style'
    styleEl.textContent = styles
    document.head.appendChild(styleEl)
  }

  registerPluginUi(context)

  const hostI18n = context.i18n.getI18n()
  stopLocaleWatch = watch(
    () => hostI18n?.global?.locale?.value,
    () => registerPluginUi(context),
  )

  console.log('[Agent Hub] Plugin activated (wasm mode)')
}

export async function deactivate(): Promise<void> {
  stopLocaleWatch?.()
  stopLocaleWatch = null
  console.log('[Agent Hub] Plugin deactivated')
}
