/**
 * AI Chatbox 插件入口 (Mobile)
 *
 * 工具箱 AI 对话面板 + 底部导航 Tab（纯 AI 对话：供应商配置 + JSONL 对话日志）。
 * 标题随宿主语言切换重注册（注册时标题被宿主静态捕获，不随 vue-i18n 自动更新）。
 */
import ChatView from './components/ChatView.vue'
import { messages } from './i18n'
import { watch } from 'vue'
import { getI18n } from '@bedcode/plugin-sdk-mobile'
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'
// 仅 dev-shell 生效：浏览器无 WASM 后端，注册命令 mock 展示完整 UI（生产构建自动排除）
import { registerDevMock, disposeDevMock } from './dev-mock'

/**
 * 是否为真实 Tauri 宿主（android:dev / 打包产物）。
 * dev-shell（浏览器 vite）无 __TAURI_INTERNALS__；真实宿主有。
 * 仅 dev-shell 注册命令 mock——真实宿主必须走 WASM 后端，
 * 否则 mock 会劫持命令（对话日志不落盘）。
 */
function isTauriHost(): boolean {
  return typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__
}

// ==================== UI 注册（标题随宿主语言切换重注册） ====================

let toolboxDisposable: { dispose(): void } | null = null
let navTabDisposable: { dispose(): void } | null = null
let stopLocaleWatch: (() => void) | null = null

/** 注册工具箱页 + 导航 Tab（语言切换时先释放旧注册再重注册） */
function registerPluginUi(context: PluginContext): void {
  toolboxDisposable?.dispose()
  navTabDisposable?.dispose()

  toolboxDisposable = context.ui.registerToolboxPage({
    id: 'ai-chatbox.toolbox',
    title: context.i18n.t('toolboxTitle'),
    component: ChatView,
  })

  navTabDisposable = context.ui.registerNavTab({
    id: 'ai-chatbox.navtab',
    title: context.i18n.t('navTitle'),
    icon: '💬',
    component: ChatView,
    order: 10,
  })
}

export async function activate(context: PluginContext): Promise<void> {
  // 注册 i18n 消息（自动添加插件 ID 前缀），必须在 UI 注册前完成
  for (const [locale, msgs] of Object.entries(messages)) {
    context.i18n.registerMessages(locale, msgs)
  }

  // dev-shell（浏览器 vite）：注册命令 mock，让无后端环境可预览完整 UI；
  // 真实 Tauri 宿主（含 android:dev）走 WASM 后端，不注册（mock 会劫持命令）
  if (import.meta.env.DEV && !isTauriHost()) {
    await registerDevMock(context)
  }

  registerPluginUi(context)

  // 宿主语言切换时重注册：标题在注册时被静态捕获，不随 vue-i18n 自动更新
  const hostI18n = getI18n()
  stopLocaleWatch = watch(
    () => hostI18n?.global?.locale?.value,
    () => registerPluginUi(context),
  )

  console.log('[AI Chatbox] Plugin activated (wasm mode, mobile)')
}

export async function deactivate(): Promise<void> {
  stopLocaleWatch?.()
  disposeDevMock()
  console.log('[AI Chatbox] Plugin deactivated')
}
