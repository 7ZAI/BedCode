/**
 * AI Chatbox 插件入口
 *
 * 侧边栏 AI 对话面板 + 终端提示词优化
 * cdylib 插件架构：Rust 后端处理 AI 请求，前端通过 PluginContext 调用
 */
import ChatView from './components/ChatView.vue'
import { messages } from './i18n'
import { watch } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-desktop'

// ==================== UI 注册（标题随宿主语言切换重注册） ====================

let sidebarDisposable: { dispose(): void } | null = null
let toolbarDisposable: { dispose(): void } | null = null
let stopLocaleWatch: (() => void) | null = null

/**
 * 注册侧边栏面板 + 终端工具栏按钮
 *
 * 注册时标题被宿主静态捕获（labelKey 非 i18n key，不随 vue-i18n 自动更新），
 * 语言切换时先释放旧注册再重新注册，菜单/路由显示文本即时刷新。
 */
function registerPluginUi(context: PluginContext) {
  sidebarDisposable?.dispose()
  toolbarDisposable?.dispose()

  // 注册侧边栏面板
  // PluginViewHost 会自动 provide('pluginContext', context)，
  // ChatView 通过 inject('pluginContext') 获取
  sidebarDisposable = context.ui.registerSidebarPanel({
    id: 'ai-chatbox.sidebar',
    title: context.i18n.t('sidebarTitle'),
    component: ChatView,
  })

  // 注册终端工具栏按钮 — 点击后由 ChatView 内部的 PromptOptimizer 处理
  // 通过事件桥接：工具栏按钮 emit 事件，ChatView 内监听并触发优化流程
  toolbarDisposable = context.ui.registerTerminalToolbarItem({
    id: 'ai-optimize-prompt',
    label: context.i18n.t('toolbarLabel'),
    icon: '✨',
    onClick: () => context.events.emit('ai-chatbox:triggerOptimize'),
  })
}

export async function activate(context: PluginContext): Promise<void> {
  // 注册 i18n 消息（自动添加插件 ID 前缀），必须在 UI 注册前完成
  for (const [locale, msgs] of Object.entries(messages)) {
    context.i18n.registerMessages(locale, msgs)
  }

  registerPluginUi(context)

  // 宿主语言切换时重注册菜单项：标题在注册时被静态捕获，不随 vue-i18n 自动更新
  const hostI18n = context.i18n.getI18n()
  stopLocaleWatch = watch(
    () => hostI18n?.global?.locale?.value,
    () => registerPluginUi(context),
  )

  console.log('[AI Chatbox] Plugin activated (rust-ts mode)')
}

export async function deactivate(): Promise<void> {
  stopLocaleWatch?.()
  console.log('[AI Chatbox] Plugin deactivated')
}
