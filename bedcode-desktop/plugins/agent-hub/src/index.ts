/**
 * Agent Hub 插件入口
 *
 * 侧边栏面板（变体 B）— cdylib 插件架构：Rust 后端处理探测/后续业务，
 * 前端经 PluginContext 调用
 */
import AgentHubView from './components/AgentHubView.vue'
import { messages } from './i18n'
import styles from './styles.css?inline'
import datepickerCss from '@vuepic/vue-datepicker/dist/main.css?inline'
import { watch } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import hubDevMock from './devMock'

// dev-shell 领域种子数据（SDK PluginDevMock 协议；真实宿主忽略）
export const devMock = hubDevMock

/**
 * @vuepic/vue-datepicker 主题覆盖：全部映射宿主 CSS 变量（跟随明暗主题）。
 * 与 auto-task 插件同源（会话日志与定时任务共用同一日期组件外观）。
 * 输入框规格与宿主表单控件一致（高 32px、圆角 6px、focus 主色描边）。
 */
const DATEPICKER_THEME_OVERRIDES = `
/* 输入框与宿主控件保持一致（同 auto-task：高 32px、圆角 6px、跟随设计变量） */
.dp__main {
  width: 100%;
}
.dp__input_wrap {
  width: 100%;
}
.dp__input {
  height: 32px;
  min-height: 32px;
  font-size: 12px;
  border-radius: 6px;
  border-color: var(--border-input);
  background: var(--bg-input);
  color: var(--text-primary);
}
.dp__input:hover {
  border-color: var(--border-input);
}
.dp__input:focus {
  border-color: var(--color-primary);
}
.dp__input::placeholder {
  color: var(--text-tertiary);
}
.dp__theme_dark {
  --dp-background-color: var(--bg-card);
  --dp-text-color: var(--text-primary);
  --dp-hover-color: var(--bg-hover);
  --dp-hover-text-color: var(--text-primary);
  --dp-hover-icon-color: var(--text-primary);
  --dp-border-color: var(--border);
  --dp-border-color-hover: var(--border-input);
  --dp-primary-color: var(--color-primary);
  --dp-primary-disabled-color: var(--color-primary);
  /* 底部操作按钮（确认/取消/现在）：文字色跟随主题对比色（深色下为深色文字），
     避免浅色 primary 背景 + 白字导致按钮不可见 */
  --dp-primary-text-color: var(--color-primary-contrast);
  --dp-secondary-color: var(--text-tertiary);
  --dp-success-color: var(--color-primary);
  --dp-icon-color: var(--text-secondary);
  --dp-disabled-color: var(--text-tertiary);
  --dp-disabled-border-color: var(--border);
  --dp-font-family: inherit;
  --dp-border-radius: 6px;
  --dp-font-size: 12px;
  --dp-preview-font-size: 12px;
  --dp-time-picker-height: 170px;
}
.dp__menu {
  font-size: 12px;
}
`

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

  // 注入会话日志日期选择器样式（@vuepic/vue-datepicker + 宿主变量主题覆盖），
  // 与 auto-task 同模式；幂等守卫防热重载重复插入
  if (!document.getElementById('agent-hub-datepicker-style')) {
    const dpStyleEl = document.createElement('style')
    dpStyleEl.id = 'agent-hub-datepicker-style'
    dpStyleEl.textContent = datepickerCss + DATEPICKER_THEME_OVERRIDES
    document.head.appendChild(dpStyleEl)
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
  // 清理会话日志日期选择器样式（与 activate 注入配对）
  document.getElementById('agent-hub-datepicker-style')?.remove()
  console.log('[Agent Hub] Plugin deactivated')
}
