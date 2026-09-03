/**
 * 计划任务插件前端入口
 *
 * Rust+TS 双层架构：Rust WASM 提供调度引擎与 HTTP 端点，TS 提供只读面板。
 * 侧边栏面板（registerSidebarPanel）→ SchedulerPanelView：任务列表 + 最近执行 + 日志路径。
 */
import type { PluginContext } from '@binblink/plugin-sdk-desktop'
import SchedulerPanelView from './components/SchedulerPanelView.vue'
import { messages } from './i18n'
// 仅 dev-shell 生效：浏览器无 Rust 后端，注册命令 mock 展示完整 UI（生产构建自动排除）
import { registerDevMock, disposeDevMock } from './dev-mock'

/**
 * 是否为真实 Tauri 宿主（tauri:dev / 打包产物）。
 * dev-shell（浏览器 vite）无 __TAURI_INTERNALS__；真实宿主有。
 * 仅 dev-shell 注册命令 mock——真实宿主必须走 Rust/WASM 后端，
 * 否则 mock 会劫持 `_http_endpoint`（调度数据失真）。
 */
function isTauriHost(): boolean {
  return typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__
}

let sidebarDisposable: { dispose(): void } | null = null

/**
 * 注册侧边栏面板
 *
 * 注册时标题被静态捕获（宿主 labelKey 非 i18n key，不随 vue-i18n 自动更新），
 * 语言切换时先释放旧注册再重新注册，菜单/路由显示文本即时刷新。
 */
function registerSidebarPanel(context: PluginContext) {
  sidebarDisposable?.dispose()
  sidebarDisposable = context.ui.registerSidebarPanel({
    id: 'scheduler.panel',
    title: context.i18n.t('panel.title'),
    // 菜单排序：插件区（内置 400）内，位于 auto-task 历史（410）之前
    order: 405,
    icon: 'M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z',
    component: SchedulerPanelView,
  })
}

/** 插件入口：宿主 webview 加载 dist/index.js 后调用 */
export async function activate(context: PluginContext) {
  // 向宿主注册插件级 i18n 消息（SDK 自动加插件 ID 前缀，locale 切换时与宿主翻译合并）
  for (const [locale, msgs] of Object.entries(messages)) {
    context.i18n.registerMessages(locale, msgs)
  }

  // dev-shell（浏览器 vite）：注册命令 mock，让无后端环境可预览完整 UI；
  // 真实 Tauri 宿主（含 tauri:dev）走 Rust/WASM 后端，不注册（mock 会劫持端点）
  if (import.meta.env.DEV && !isTauriHost()) {
    await registerDevMock(context)
  }

  registerSidebarPanel(context)
}

export function deactivate() {
  sidebarDisposable?.dispose()
  sidebarDisposable = null
  disposeDevMock()
}
