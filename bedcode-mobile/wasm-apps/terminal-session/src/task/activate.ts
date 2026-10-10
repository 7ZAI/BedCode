/**
 * 任务域激活编排（原 auto-task 插件入口，票 16 并入本 app 的 task 域）
 *
 * 承载：终端工具栏入口（按当前会话 agent 白名单动态可见）+ 自渲染任务队列面板
 * （AutoTaskPanelHost，createApp 挂载到 document.body，与桌面端 AutoTaskModal 一致）
 * + 工具箱「任务记录 / 定时任务」页注册 + i18n 注册。
 *
 * 数据面（ADR 0012「手机看、桌面管」）：本域不落本地业务真源，全部经
 * `task/api.ts` 的 HTTP 客户端访问桌面端 `com.bedcode.terminal-session` 端点；
 * WS 三事件（scheduled / queue / status 变更）驱动刷新。
 *
 * 文案口径（spec D6 强制⑥）：本 app 是「远程终端控制端」，任务域是桌面任务域的
 * 只读投影视图，不自称会话/任务权威。
 */
import { createApp, type App, watch } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import { getMobileApi } from '@binblink/bedcode-plugin-sdk-mobile'
import { getAutoTaskApi } from './api'
import AutoTaskPanelHost from './components/AutoTaskPanelHost.vue'
import AutoTaskToolboxView from './components/AutoTaskToolboxView.vue'
import { autoTaskPanelVisible } from './state'
import { messages } from './i18n'
import panelCss from './panel.css?inline'
import toolboxCss from './toolbox.css?inline'
// 开源 Vue3 日期/时间选择组件（替代原生 datetime-local 控件，样式可随主题定制）
import datepickerCss from '@vuepic/vue-datepicker/dist/main.css?inline'

// ==================== Datepicker 主题定制 ====================

// 日期选择器与宿主主题融合：跟随移动端设计 token（--mobile-*），
// 深色模式由 Datepicker 的 dark prop 切换 .dp__theme_dark，此处覆盖其默认变量。
// 弹层整体覆盖为移动端 bottom-sheet（原因见下方「移动端弹层适配」注释）。
const DATEPICKER_THEME_OVERRIDES = `
/* ==================== 输入框 ==================== */
/* 与宿主控件保持一致（高度 44px、圆角 10px，与自绘输入框同规格） */
.dp__main {
  width: 100%;
}
.dp__input_wrap {
  width: 100%;
}
.dp__input {
  height: 44px;
  min-height: 44px;
  font-size: 14px;
  border-radius: 10px;
  border-color: var(--mobile-border);
  background: var(--mobile-bg-primary);
  color: var(--mobile-text-primary);
}
.dp__input:hover {
  border-color: var(--mobile-border);
}
.dp__input:focus {
  border-color: var(--mobile-accent);
}
.dp__input::placeholder {
  color: var(--mobile-text-disabled);
}

/* ==================== 主题变量（深浅色统一走 token） ==================== */
.dp__theme_dark,
.dp__theme_light {
  --dp-background-color: var(--mobile-bg-card);
  --dp-text-color: var(--mobile-text-primary);
  --dp-hover-color: var(--mobile-bg-tertiary);
  --dp-hover-text-color: var(--mobile-text-primary);
  --dp-hover-icon-color: var(--mobile-text-primary);
  --dp-border-color: var(--mobile-border);
  --dp-menu-border-color: var(--mobile-border);
  --dp-border-color-hover: var(--mobile-border-hover);
  --dp-border-color-focus: var(--mobile-accent);
  --dp-primary-color: var(--mobile-accent);
  --dp-primary-disabled-color: var(--mobile-accent);
  /* 底部操作按钮（确认/取消/现在）：文字色跟随主题对比色（深色下为深色文字），
     避免浅色 accent 背景 + 白字导致按钮不可见 */
  --dp-primary-text-color: var(--mobile-text-on-accent);
  --dp-secondary-color: var(--mobile-text-muted);
  --dp-success-color: var(--mobile-accent);
  --dp-icon-color: var(--mobile-text-secondary);
  --dp-disabled-color: var(--mobile-text-disabled);
  --dp-disabled-border-color: var(--mobile-border);
  --dp-font-family: inherit;
  --dp-border-radius: 10px;
  --dp-font-size: 14px;
  --dp-preview-font-size: 13px;
  --dp-time-picker-height: 200px;
  /* 触控目标加大：日历格 40px、操作按钮 40px、月年行 44px（Apple 44pt 建议） */
  --dp-cell-size: 40px;
  --dp-cell-padding: 6px;
  --dp-month-year-row-height: 44px;
  --dp-action-button-height: 40px;
  --dp-action-buttons-padding: 4px 14px;
  --dp-time-inc-dec-button-size: 36px;
}

/* ==================== 移动端弹层适配：bottom-sheet ====================
   库默认把菜单绝对定位在输入框附近，小屏（视口高度不足 / 输入框贴近屏幕
   边缘）时菜单会被裁出视口：顶部月历表头（时间切换按钮）或底部操作行
   （确认/取消）不可达，表现为「无法选择时间 / 点击确定无效」。
   覆盖为全屏固定遮罩 + 底部卡片：
   - 永不裁切：菜单高度受限、内部滚动，操作行固定可见
   - 遮罩空白点击关闭由组件侧 document click 处理器实现
     （库的 onClickOutside 以遮罩元素为界，点击遮罩本身不会关闭）
   - 遮罩类 .fixed.inset-0 语义被宿主 swipe 容器识别为弹窗（禁用页面滑动）
   注意：v9.0.3 中外层容器与菜单元素都带 .dp--menu-wrapper 类，
   遮罩规则必须限定 .dp__outer_menu_wrap，卡片规则限定 .dp__menu。 */
.dp__outer_menu_wrap.dp--menu-wrapper {
  position: fixed !important;
  top: 0 !important;
  right: 0 !important;
  bottom: 0 !important;
  left: 0 !important;
  display: flex !important;
  align-items: flex-end !important;
  justify-content: center !important;
  padding: 0 !important;
  background: var(--mobile-overlay-heavy);
}
.dp--menu-wrapper.dp__menu {
  width: 100% !important;
  max-width: 30rem;
  margin: 0 auto;
  max-height: min(80dvh, 640px);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border-radius: 1.25rem 1.25rem 0 0;
  border-bottom: none;
  box-shadow: 0 -8px 32px rgba(0, 0, 0, 0.18);
  padding-bottom: env(safe-area-inset-bottom);
}
/* 内容区（日历/时间列）独立滚动，操作行固定在卡片底部 */
.dp--menu-wrapper.dp__menu > div {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}
.dp--menu-wrapper.dp__menu > .dp__action_row {
  flex: none;
  flex-shrink: 0;
}
.dp__instance_calendar {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}
/* 底部卡片不需要定位箭头 */
.dp__arrow_top,
.dp__arrow_bottom {
  display: none !important;
}
.dp__action_button {
  min-width: 4.5rem;
  border-radius: 0.625rem;
  font-size: 14px;
}
.dp__menu {
  font-size: 14px;
}
`

let _ctx: PluginContext

// ==================== 面板挂载管理 ====================

let modalApp: App | null = null
let modalContainer: HTMLElement | null = null

/** 挂载自动任务面板（常驻 document.body，可见性由共享 ref 控制） */
function mountPanel(context: PluginContext) {
  if (modalApp) return
  modalContainer = document.createElement('div')
  document.body.appendChild(modalContainer)
  modalApp = createApp(AutoTaskPanelHost)
  // 与 PluginViewHost 保持一致：通过 provide/inject 传递插件 context
  modalApp.provide('pluginContext', context)
  modalApp.mount(modalContainer)
}

function unmountPanel() {
  modalApp?.unmount()
  modalContainer?.remove()
  modalApp = null
  modalContainer = null
}

/** 注入面板与工具箱样式（宿主不加载插件 dist/style.css，运行时注入一次） */
function injectPanelStyle() {
  if (!document.getElementById('terminal-session-task-panel-style')) {
    const styleEl = document.createElement('style')
    styleEl.id = 'terminal-session-task-panel-style'
    styleEl.textContent = panelCss
    document.head.appendChild(styleEl)
  }
  if (!document.getElementById('terminal-session-task-toolbox-style')) {
    const styleEl = document.createElement('style')
    styleEl.id = 'terminal-session-task-toolbox-style'
    styleEl.textContent = toolboxCss
    document.head.appendChild(styleEl)
  }
  // 日期选择器样式（基础样式 + 主题覆盖，同样运行时注入）
  if (!document.getElementById('terminal-session-task-datepicker-style')) {
    const styleEl = document.createElement('style')
    styleEl.id = 'terminal-session-task-datepicker-style'
    styleEl.textContent = datepickerCss + DATEPICKER_THEME_OVERRIDES
    document.head.appendChild(styleEl)
  }
}

/** 壳内任务页动态路由（票 2026-10-10 批次 C2：这是应用内子页的正路，需 manifest `ui:route` 权限） */
let routeDisposable: { dispose(): void } | null = null
/** 壳内胶囊菜单项：从应用菜单直达任务页 */
let capsuleDisposable: { dispose(): void } | null = null
let stopConnectionWatch: (() => void) | null = null

// ==================== 激活 ====================

/** 任务域激活（插件入口 activate 调用；与终端域（票 15 迁入）并列） */
export async function activateTaskDomain(context: PluginContext): Promise<void> {
  _ctx = context
  context.logger.info('Task domain activating...')

  // 注册 i18n 消息（自动添加插件 ID 前缀），必须在面板组件 setup 前完成
  for (const [locale, msgs] of Object.entries(messages)) {
    context.i18n.registerMessages(locale, msgs)
  }

  // 从后端获取适配 agent 白名单（权威来源桌面端 Rust AGENT_PROFILES），缓存后供任务面板判定使用
  const mobileApi = getMobileApi()
  const api = getAutoTaskApi()

  /** 拉取适配 agent 白名单并缓存（未连接时对端不可达，失败保持旧值） */
  async function refreshSupportedAgents() {
    try {
      const result = await api.httpListSupportedAgents()
      if (result.code === 0 && result.data) {
        supportedAgents = result.data.agents || []
      }
    } catch (e) {
      // 机制对齐：失败必须经 logger（禁止 console/静默吞错）
      context.logger.warn(`[task] load supported agents failed: ${e instanceof Error ? e.message : e}`)
    }
  }

  // 启动时未连接则跳过首次请求（对空 baseUrl 发起 REST 调用只会命中宿主的
  // "No base URL set" 错误日志）；连接建立后再拉取
  if (mobileApi.isConnected?.value) {
    await refreshSupportedAgents()
  }

  // 连接建立后白名单才有意义：监听连接状态，连上时拉取
  // （避免 connect 后 supportedAgents 仍为空）
  stopConnectionWatch = watch(
    () => mobileApi.isConnected?.value,
    (connected) => {
      if (!connected) return
      void refreshSupportedAgents()
    },
  )

  // 挂载面板（i18n 已注册，组件 setup 可正常取文案）
  injectPanelStyle()
  mountPanel(context)

  // 票 2026-10-10 批次 C2：`registerToolboxPage` 已整面退役。工具箱界面（任务记录 +
  // 定时任务两页签）改由 app 域运行面的「工具箱」页签直接渲染 AutoTaskToolboxView
  // （见 ../app/AppRoot.vue），不再作为独立扩展点注册。

  // 壳内任务页：动态路由（host 页头模式自带返回）+ 应用胶囊菜单入口。
  // 权限未授予时注册抛错，不影响插件其余能力（与工具箱页同一口径）。
  try {
    routeDisposable = context.ui.registerRoute({
      id: 'tasks',
      title: context.i18n.t('toolboxTitle'),
      component: AutoTaskToolboxView,
      header: true,
    })
    capsuleDisposable = context.ui.registerCapsuleItem({
      id: 'task-page',
      label: context.i18n.t('capsuleTitle'),
      icon: 'M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 7l2 2 4-4',
      order: 20,
      onSelect: () => {
        void context.ui.openPage('tasks')
      },
    })
  } catch (e) {
    context.logger.warn(`Shell task page registration failed: ${e}`)
    routeDisposable = null
    capsuleDisposable = null
  }

  context.logger.info('Task domain activated')
}

/** 任务域停用（注册项回收；面板与样式一并摘除） */
export function deactivateTaskDomain(): void {
  stopConnectionWatch?.()
  stopConnectionWatch = null
  routeDisposable?.dispose()
  routeDisposable = null
  capsuleDisposable?.dispose()
  capsuleDisposable = null
  unmountPanel()
  document.getElementById('terminal-session-task-panel-style')?.remove()
  document.getElementById('terminal-session-task-toolbox-style')?.remove()
  document.getElementById('terminal-session-task-datepicker-style')?.remove()
  _ctx?.logger.info('Task domain deactivated')
}
