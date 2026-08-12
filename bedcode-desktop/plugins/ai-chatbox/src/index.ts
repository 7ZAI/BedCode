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

/**
 * 是否为真实 Tauri 宿主（tauri:dev / 打包产物）。
 * dev-shell（浏览器 vite）无 __TAURI_INTERNALS__；真实宿主有。
 * 仅 dev-shell 注册命令 mock——真实宿主必须走 Rust/WASM 后端，
 * 否则 mock 会劫持命令（对话日志不落盘）。
 */
function isTauriHost(): boolean {
  return typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__
}

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
    // 菜单排序：紧跟文件传输（220）之后，位于插件管理（内置 400）之前
    order: 230,
    component: ChatView,
  })
}

export async function activate(context: PluginContext): Promise<void> {
  // 注册 i18n 消息（自动添加插件 ID 前缀），必须在 UI 注册前完成
  for (const [locale, msgs] of Object.entries(messages)) {
    context.i18n.registerMessages(locale, msgs)
  }

  // dev-shell（浏览器 vite）：注册命令 mock，让无后端环境可预览完整 UI；
  // 真实 Tauri 宿主（含 tauri:dev）走 Rust/WASM 后端，不注册（mock 会劫持命令）
  if (import.meta.env.DEV && !isTauriHost()) {
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
