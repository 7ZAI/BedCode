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
 *
 * **输入框规格引用插件派生的 `--ah-ctl-*` 族**（真源 = 宿主 Input.vue /
 * SDK Select md 的类组合，映射见 styles.css「表单控件统一规格」段）：日期框与
 * 同排的关键词输入框、Agent 下拉因此逐属性同源，不会各写一套数值漂移。
 * vendor 默认里三处会让它「看起来不是一套」的项已就地覆盖：
 * - `--dp-font-family` 是 vendor 自带字体栈（Linux 上落到 sans-serif，
 *   与宿主 'Segoe UI'/system-ui 栈不同字形）→ `font-family: inherit`
 * - `--dp-input-padding: 6px 30px 6px 12px` 左内边距 12px（≠ px-4 的 16px）
 *   → 左 16px；右侧保留 30px 供清除按钮（clearable）落位
 * - `.dp__input::placeholder { opacity: .7 }` 比同排控件淡 30% → 回到 1
 * 明暗两套主题类共用同一组 token（v9 的浅色类名是 `.dp__theme_light`，
 * 原先只覆盖 dark，导致浅色下弹层字号落到 `--dp-font-size: 1rem`）。
 */
const DATEPICKER_THEME_OVERRIDES = `
/* 输入框与插件内其它表单控件保持同一规格（票 12；2026-09-28 改走 --ah-ctl-* 族） */
.dp__main {
  width: 100%;
}
.dp__input_wrap {
  width: 100%;
}
.dp__input {
  height: var(--ah-ctl-height);
  min-height: var(--ah-ctl-height);
  box-sizing: border-box;
  font-family: inherit;
  font-size: var(--ah-ctl-font-size);
  /* 行高回到宿主继承值（vendor 按 --dp-font-size=12px 算 18px，与输入框实际
     字号 14px 脱钩，文字会比同排控件偏上） */
  line-height: 1.5;
  padding: 0 30px 0 var(--ah-ctl-padding-x);
  border-radius: var(--ah-ctl-radius);
  border-color: var(--ah-ctl-border-color);
  background: var(--ah-ctl-bg);
  color: var(--ah-ctl-fg);
  box-shadow: var(--ah-ctl-shadow);
  transition:
    border-color 0.2s,
    background-color 0.2s,
    box-shadow 0.2s;
}
/* vendor 把自带字体栈（Linux 上是 -apple-system/sans-serif，与宿主
   'Segoe UI'/system-ui 栈不同字形）设在 .dp__main 上，.dp__input 的
   font-family: inherit 只会继承到这个栈——所以外层也必须显式 inherit。
   弹层（Teleport 到 body）的三处消费方同理。 */
.dp__main,
.dp__menu,
.dp__time_input,
.dp__action_button,
.dp__overlay {
  font-family: inherit;
}
:root.dark .dp__input {
  box-shadow: none;
}
.dp__input:hover {
  border-color: var(--ah-ctl-border-color);
}
.dp__input:focus {
  border-color: var(--ah-ctl-focus-border);
  box-shadow: var(--ah-ctl-focus-shadow);
}
.dp__input::placeholder {
  color: var(--ah-ctl-placeholder);
  opacity: 1;
}
.dp__theme_light,
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
  --dp-secondary-color: var(--ah-text-data);
  --dp-success-color: var(--color-primary);
  --dp-icon-color: var(--ah-text-data);
  --dp-disabled-color: var(--text-tertiary);
  --dp-disabled-border-color: var(--border);
  /* 字体不再靠 token：--dp-font-family 的每个消费方都在本文件里显式写
     font-family: inherit（自定义属性写 inherit 只是取父级的同名 token，
     等于没改——这是自定义属性与真实属性的关键差别） */
  --dp-border-radius: 6px;
  --dp-font-size: var(--font-size-label);
  --dp-preview-font-size: var(--font-size-label);
  --dp-time-picker-height: 170px;
}
.dp__menu {
  font-size: var(--font-size-label);
}
`

// ==================== UI 注册（标题随宿主语言切换重注册） ====================

/**
 * 侧边栏槽位：紧跟 Agent任务（terminal-session 210）之后。
 * 宿主 useSidebarMenu 把内置项与各插件贡献目录按 order 统一升序排布，
 * 此值须与 plugin.json `contributes.views[0].order` 同源（宿主 Rust 侧
 * 也按 manifest 登记视图，两处漂移会让菜单顺序与工具箱面板列表不一致）。
 */
export const AGENT_HUB_SIDEBAR_ORDER = 215

let sidebarDisposable: { dispose(): void } | null = null
let stopLocaleWatch: (() => void) | null = null

/**
 * 注册侧边栏面板
 *
 * 注册时标题被宿主静态捕获（labelKey 非 i18n key，不随 vue-i18n 自动更新），
 * 语言切换时先释放旧注册再重新注册，菜单显示文本即时刷新。
 * 排序：紧跟 Agent任务（terminal-session 槽位 210）之后，取 215 独占槽位
 * （10 的倍数已被占用：200 终端会话 / 210 Agent任务 / 220 file-transfer /
 * 230 ai-chatbox），位于其余插件目录之前。
 */
function registerPluginUi(context: PluginContext) {
  sidebarDisposable?.dispose()

  sidebarDisposable = context.ui.registerSidebarPanel({
    id: 'agent-hub.sidebar',
    title: context.i18n.t('hub.sidebar.title'),
    order: AGENT_HUB_SIDEBAR_ORDER,
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
