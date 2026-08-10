/**
 * AI Chatbox 插件入口
 *
 * 侧边栏 AI 对话面板（纯 AI 对话，供应商配置 + JSONL 对话日志）。
 * cdylib 插件架构：Rust 后端处理 AI 请求与持久化，前端经 PluginContext 调用。
 */
import ChatView from './components/ChatView.vue'
import { messages } from './i18n'
import { watch } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-desktop'
// 仅 dev-shell 生效：浏览器无 Rust 后端，注册命令 mock 展示完整 UI（生产构建自动排除）
import { registerDevMock, disposeDevMock } from './dev-mock'

// ==================== UI 注册（标题随宿主语言切换重注册） ====================

let sidebarDisposable: { dispose(): void } | null = null
let stopLocaleWatch: (() => void) | null = null

/**
 * 注册侧边栏面板
 *
 * 注册时标题被宿主静态捕获（labelKey 非 i18n key，不随 vue-i18n 自动更新），
 * 语言切换时先释放旧注册再重新注册，菜单/路由显示文本即时刷新。
 * PluginViewHost 自动 provide('pluginContext', context)，
 * ChatView 通过 inject('pluginContext') 获取。
 */
function registerPluginUi(context: PluginContext) {
  sidebarDisposable?.dispose()

  sidebarDisposable = context.ui.registerSidebarPanel({
    id: 'ai-chatbox.sidebar',
    title: context.i18n.t('sidebarTitle'),
    component: ChatView,
  })
}

export async function activate(context: PluginContext): Promise<void> {
  // 注册 i18n 消息（自动添加插件 ID 前缀），必须在 UI 注册前完成
  for (const [locale, msgs] of Object.entries(messages)) {
    context.i18n.registerMessages(locale, msgs)
  }

  // dev-shell（vite dev）：注册命令 mock，让无后端环境可预览完整 UI
  if (import.meta.env.DEV) {
    await registerDevMock(context)
  }

  registerPluginUi(context)

  // 宿主语言切换时重注册：标题在注册时被静态捕获，不随 vue-i18n 自动更新
  const hostI18n = context.i18n.getI18n()
  stopLocaleWatch = watch(
    () => hostI18n?.global?.locale?.value,
    () => registerPluginUi(context),
  )

  console.log('[AI Chatbox] Plugin activated (rust-ts mode)')
}

export async function deactivate(): Promise<void> {
  stopLocaleWatch?.()
  disposeDevMock()
  console.log('[AI Chatbox] Plugin deactivated')
}
