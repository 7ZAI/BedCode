/**
 * Auto Task 插件入口
 *
 * Agent 任务状态同步与自动授权（Claude Code hooks / pi 扩展）
 * Rust+TS 双层架构：Rust WASM 处理后端逻辑，TS 负责 UI 和 toast 通知
 *
 * UI 入口：
 * - 终端工具栏按钮（registerTerminalToolbarItem）→ 打开自动任务队列弹窗
 * - 侧边栏任务历史视图（registerSidebarPanel）
 */
import { createApp, type App, watch } from 'vue'
import TaskHistoryView from './components/TaskHistoryView.vue'
import AutoTaskModal from './components/AutoTaskModal.vue'
import autoTaskModalCss from './components/auto-task-modal.css?inline'
// 开源 Vue3 日期/时间选择组件（替代原生 datetime-local 控件，样式可随主题定制）
import datepickerCss from '@vuepic/vue-datepicker/dist/main.css?inline'
// 宿主 OS 平台：自动任务投递的输入提交符按平台选择（Windows=CR，Linux=LF），
// 通过 @tauri-apps/plugin-os 读取（同步 API，宿主已注册该插件）
import { platform } from '@tauri-apps/plugin-os'
import { autoTaskModalVisible } from './state'
import { messages } from './i18n'
import type { PluginContext } from '@binblink/plugin-sdk-desktop'

// ==================== Datepicker 主题定制 ====================

// 日期选择器与宿主主题融合：跟随应用的设计变量（bg-card / border / primary 等），
// 深色模式由 Datepicker 的 dark prop 切换 .dp__theme_dark，此处覆盖其默认深色变量
const DATEPICKER_THEME_OVERRIDES = `
/* 输入框与宿主控件保持一致（controlCls 同规格：高 32px、圆角 6px、跟随设计变量） */
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
let toolbarDisposable: { dispose(): void } | null = null
let stopLocaleWatch: (() => void) | null = null
let stopRouteWatch: (() => void) | null = null

// ==================== 工具栏入口可见性（仅插件适配的 agent 会话） ====================

// 异步同步序号：路由快速切换时丢弃过期结果，避免旧会话的 agent 覆盖新状态
let toolbarSyncSeq = 0

// 按当前路由会话的 agent 动态注册/注销工具栏入口（路由切换时重新评估）
// 直接调用后端 list-running-sessions，利用其返回的 is_supported 字段判断，
// 避免前端 hardcode 白名单（权威来源在 Rust AGENT_PROFILES）。
async function syncToolbarEntry(context: PluginContext) {
  const seq = ++toolbarSyncSeq
  const shared = (window as any).__BEDCODE_SHARED__
  const id = shared?.router?.currentRoute?.value?.params?.id
  if (typeof id !== 'string' || !id) {
    if (toolbarDisposable) {
      toolbarDisposable.dispose()
      toolbarDisposable = null
    }
    return
  }
  try {
    const result: any = await context.commands.execute('auto-task.list-running-sessions')
    if (seq !== toolbarSyncSeq) return // 过期结果丢弃
    const match = (result?.sessions ?? []).find((s: any) => s.session_id === id)
    const shouldShow = match?.is_supported ?? false
    if (shouldShow && !toolbarDisposable) {
      toolbarDisposable = context.ui.registerTerminalToolbarItem({
        id: 'auto-task.open-modal',
        label: context.i18n.t('title'),
        onClick: () => {
          autoTaskModalVisible.value = true
        },
      })
    } else if (!shouldShow && toolbarDisposable) {
      toolbarDisposable.dispose()
      toolbarDisposable = null
    }
  } catch (e) {
    console.warn('[AutoTask] Failed to sync toolbar entry:', e)
  }
}

/**
 * 注册侧边栏面板
 *
 * 注册时标题被静态捕获（宿主 labelKey 非 i18n key，不随 vue-i18n 自动更新），
 * 语言切换时先释放旧注册再重新注册，菜单/路由显示文本即时刷新。
 */
function registerSidebarPanel(context: PluginContext) {
  sidebarDisposable?.dispose()

  sidebarDisposable = context.ui.registerSidebarPanel({
    id: 'auto-task.history',
    title: context.i18n.t('historyTitle'),
    // 菜单排序：紧跟终端会话（内置 200）之后，位于服务器（内置 300）之前
    order: 210,
    icon: 'M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01',
    component: TaskHistoryView,
  })
}

// ==================== 弹窗挂载管理 ====================

let modalApp: App | null = null
let modalContainer: HTMLElement | null = null

/** 挂载自动任务弹窗（每个 webview 独立实例，常驻 document.body） */
function mountModal(context: PluginContext) {
  if (modalApp) return
  modalContainer = document.createElement('div')
  document.body.appendChild(modalContainer)
  modalApp = createApp(AutoTaskModal)
  // 与 PluginViewHost 保持一致：通过 provide/inject 传递插件 context
  modalApp.provide('pluginContext', context)
  modalApp.mount(modalContainer)
}

function unmountModal() {
  modalApp?.unmount()
  modalContainer?.remove()
  modalApp = null
  modalContainer = null
}

export async function activate(context: PluginContext): Promise<void> {
  // 上报宿主平台：WASM 调度按平台选择终端输入提交符（Windows=CR，Linux=LF），
  // 见 rust/src/queue.rs input_submit_char。失败仅告警，不影响插件激活（默认回退 CR）
  try {
    await context.commands.execute('auto-task.set-platform', { platform: platform() })
  } catch (e) {
    console.warn('[AutoTask] failed to report host platform:', e)
  }

  // 注册 i18n 消息（自动添加插件 ID 前缀），必须在弹窗组件 setup 前完成
  // 翻译表维护在 src/i18n/ 独立文件，构建期由 Vite 编译内联进 bundle（无运行时文件读取）
  for (const [locale, msgs] of Object.entries(messages)) {
    context.i18n.registerMessages(locale, msgs)
  }

  // 注入弹窗样式（插件构建的 CSS 不会被宿主自动加载，运行时手动注入）
  if (!document.getElementById('auto-task-modal-style')) {
    const styleEl = document.createElement('style')
    styleEl.id = 'auto-task-modal-style'
    styleEl.textContent = autoTaskModalCss
    document.head.appendChild(styleEl)
  }

  // 注入日期选择器样式（基础样式 + 主题覆盖，同样运行时注入）
  if (!document.getElementById('auto-task-datepicker-style')) {
    const styleEl = document.createElement('style')
    styleEl.id = 'auto-task-datepicker-style'
    styleEl.textContent = datepickerCss + DATEPICKER_THEME_OVERRIDES
    document.head.appendChild(styleEl)
  }

  // 注册侧边栏面板 + 终端工具栏按钮（标题随宿主语言切换重注册，见 registerSidebarPanel / syncToolbarEntry）
  registerSidebarPanel(context)

  // 工具栏入口仅对插件适配的 agent 会话显示：监听终端窗口路由切换动态注册/注销
  const sharedRouter = (window as any).__BEDCODE_SHARED__?.router
  if (sharedRouter) {
    stopRouteWatch = watch(
      () => sharedRouter.currentRoute?.value?.params?.id,
      () => syncToolbarEntry(context),
    )
    syncToolbarEntry(context)
  } else {
    console.warn('[AutoTask] Shared router unavailable, toolbar entry visibility not managed')
  }

  // 宿主语言切换时重注册菜单项：面板/按钮标题在注册时被静态捕获（labelKey 非 i18n key），
  // 不随 vue-i18n 自动更新，需监听 locale 变化后重新注册刷新菜单/路由显示文本
  const hostI18n = context.i18n.getI18n()
  stopLocaleWatch = watch(
    () => hostI18n?.global?.locale?.value,
    () => {
      registerSidebarPanel(context)
      // 工具栏标题同样静态捕获：语言切换后注销重注册，重新评估当前会话 agent
      toolbarDisposable?.dispose()
      toolbarDisposable = null
      syncToolbarEntry(context)
    },
  )

  // 挂载自动任务弹窗（i18n 已注册，组件 setup 可正常取文案）
  mountModal(context)

  // 监听任务状态变更 → toast 提示
  context.events.on('task:status-changed', (data: any) => {
    const { taskStatus, taskReason } = data
    const statusMessages: Record<string, string> = {
      idle: '空闲',
      in_progress: '执行中',
      asking: '等待输入',
      completed: '已完成',
      interrupted: '已中断',
    }
    const label = statusMessages[taskStatus] || taskStatus
    console.log(`[Auto Task] 状态变更: ${label}${taskReason ? ` - ${taskReason}` : ''}`)
  })

  // 监听会话模式变更
  context.events.on('session:mode-changed', (data: any) => {
    const { autoApprove } = data
    console.log(`[Auto Task] 模式变更: ${autoApprove ? '自动授权' : '手动模式'}`)
  })

  console.log('[Auto Task] Plugin activated')
}

export async function deactivate(): Promise<void> {
  stopRouteWatch?.()
  stopLocaleWatch?.()
  toolbarDisposable?.dispose()
  toolbarDisposable = null
  sidebarDisposable?.dispose()
  sidebarDisposable = null
  unmountModal()
  document.getElementById('auto-task-modal-style')?.remove()
  document.getElementById('auto-task-datepicker-style')?.remove()
  console.log('[Auto Task] Plugin deactivated')
}
