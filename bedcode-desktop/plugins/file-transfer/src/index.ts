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
import { getRouter, type PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import peerDevMock from './devMock'
import { useConsent, type ConsentController } from './composables/useConsent'

// dev-shell 领域种子数据（SDK PluginDevMock 协议；真实宿主忽略）
export const devMock = peerDevMock

// ==================== UI 注册（标题随宿主语言切换重注册） ====================

let sidebarDisposable: { dispose(): void } | null = null
let stopLocaleWatch: (() => void) | null = null
let consentController: ConsentController | null = null
let statusItemDisposable: { dispose(): void } | null = null
let stopConsentWatch: (() => void) | null = null

/** 插件面板在宿主路由中的路径（状态栏项跳转落点，spec 决策 6） */
const PANEL_ROUTE = '/plugin/sidebar/com.bedcode.file-transfer/file-transfer.sidebar'

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

/**
 * 同步首连确认状态栏项：有待确认请求时注册展示计数，清零即注销
 *
 * label 在注册时被静态捕获（同侧边栏标题），待确认数或语言变化时整体重注册。
 * 点击经宿主共享 router 跳转插件面板，跳转后弹窗可见可操作（useConsent 状态
 * 常驻于激活期，不依赖视图挂载）；dev-shell router 无此路由时 push 静默无害。
 */
function syncConsentStatusItem(context: PluginContext, count: number) {
  statusItemDisposable?.dispose()
  statusItemDisposable = null
  if (count <= 0) return
  statusItemDisposable = context.ui.registerStatusBarItem({
    id: 'file-transfer.consent',
    label: context.i18n.t('transfer.consent.statusItem', { n: count }),
    // 盾牌盾勾图标（Heroicons outline shield-check，与宿主图标体系一致）
    icon: 'M9 12.75L11.25 15 15 9.75m-3-7.036A11.959 11.959 0 013.598 6 11.99 11.99 0 003 9.75c0 5.592 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.31-.21-2.57-.598-3.75h-.152c-3.196 0-6.1-1.248-8.25-3.285z',
    onClick: () => {
      try {
        getRouter()?.push(PANEL_ROUTE)
      } catch (e) {
        console.warn('[File Transfer] navigate to panel failed:', e)
      }
    },
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

  // 首连确认编排：激活期常驻订阅（不依赖视图挂载），弹窗渲染在
  // FileTransferView 内，不在面板时经状态栏项跳转处理（spec 决策 6 折衷）
  consentController = useConsent(context)
  consentController.start()
  stopConsentWatch = watch(
    () => [hostI18n?.global?.locale?.value, consentController!.pendingCount.value] as const,
    ([, count]) => syncConsentStatusItem(context, Number(count)),
    { immediate: true },
  )

  console.log('[File Transfer] Plugin activated (wasm mode)')
}

export async function deactivate(): Promise<void> {
  stopLocaleWatch?.()
  stopConsentWatch?.()
  stopConsentWatch = null
  statusItemDisposable?.dispose()
  statusItemDisposable = null
  consentController?.stop()
  consentController = null
  console.log('[File Transfer] Plugin deactivated')
}
