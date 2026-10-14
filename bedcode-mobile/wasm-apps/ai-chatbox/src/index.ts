/**
 * AI Chatbox 插件入口 (Mobile) — 独立页面版（宿主壳运行面）
 *
 * 宿主壳（/mobile/shell）内的独立页面：纯 AI 对话（供应商配置 + JSONL 对话日志）。
 * 不再以导航 Tab 嵌入旧宿主（票 2026-10-09-mobile-host-into-wasm-apps）。
 * 运行面无标题/navTab 语义 → 不再需要语言切换重注册。
 */
import ChatView from './components/ChatView.vue'
import { messages } from './i18n'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
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

/** 激活期捕获的统一日志入口（deactivate 无 context 入参） */
let activeLogger: PluginContext['logger'] | null = null

export async function activate(context: PluginContext): Promise<void> {
  // 注册 i18n 消息（自动添加插件 ID 前缀），必须在 UI 注册前完成；
  // spread 展开：MessageSchema 为 interface 无隐式索引签名，registerMessages 需要 Record<string, unknown>
  for (const [locale, msgs] of Object.entries(messages)) {
    context.i18n.registerMessages(locale, { ...msgs })
  }

  // dev-shell（浏览器 vite）：注册命令 mock，让无后端环境可预览完整 UI；
  // 真实 Tauri 宿主（含 android:dev）走 WASM 后端，不注册（mock 会劫持命令）
  if (import.meta.env.DEV && !isTauriHost()) {
    await registerDevMock(context)
  }

  // 宿主壳运行面：独立页面（appId = 插件 id 由宿主代填）
  context.ui.registerSurface({ component: ChatView })

  // 停用日志需要 logger：deactivate 无 context 入参，激活期捕获（与旧宿主 logger 机制一致）
  activeLogger = context.logger
  activeLogger.info('[AI Chatbox] plugin activated (standalone page, mobile)')
}

export async function deactivate(): Promise<void> {
  disposeDevMock()
  activeLogger?.info('[AI Chatbox] plugin deactivated')
  activeLogger = null
}
