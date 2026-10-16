/**
 * Terminal Session Center 插件入口（TS 前端）
 *
 * 票 13（P3）：贡献会话中心页面——侧边栏多目录中的「终端会话」一项，占据宿主
 * 内置会话入口的同一排序槽位（`order = 200`）→ 宿主内置入口按激活态让位，原路由
 * `/sessions` 退为跳转壳（判据与实现见宿主 `useSidebarMenu.builtinSupersededBy`
 * 与 `router` 守卫）；插件 error / 停用时宿主兜底壳接管（不白屏）。
 *
 * 票 14（P3）：设备与配对视图搬迁——贡献「设备配对」（`order = 100`，顶替宿主
 * 内置设备入口）与「连接历史」（`order = 101`，紧随其后）两个侧边栏目录，并贡献
 * 设置页「配对设置」分组（`ui:settings`，宿主内置配对分组随之退役）。
 *
 * 票 17（P4）：任务域 UI 并入——「任务历史」侧边栏目录（`order = 210`，与旧
 * `com.bedcode.auto-task` 插件同槽位）、终端工具栏按钮与任务队列弹窗改由本插件贡献
 * （spec D6「界面维持，贡献方换人」：像素级不变，只换归属）。
 *
 * 取数红线（spec D2）：只经 PluginContext（commands / session / ui / events /
 * storage / i18n）与插件命令通道（宿主 `plugin_invoke` → 本插件 WASM
 * `command.invoke`）取数；**禁止**直调宿主领域命令（`list_sessions` /
 * `generate_pairing_code` 等）——那层门面是宿主 UI 的兼容接缝。
 */
import { createApp, watch, type App } from 'vue'
import SessionCenterView from './components/SessionCenterView.vue'
import DeviceCenterView from './components/DeviceCenterView.vue'
import ConnectionHistoryView from './components/ConnectionHistoryView.vue'
import PairingSettingsSection from './components/PairingSettingsSection.vue'
import SessionSettingsSection from './components/SessionSettingsSection.vue'
import TaskHistoryView from './components/TaskHistoryView.vue'
import TaskQueueModal from './components/TaskQueueModal.vue'
// 终端窗口视图（票 03a 壳迁入；type='page' 不进侧边栏菜单，宿主 /terminal-window/:id
// 路由经 PluginWindowHostView → PluginViewHost 渲染本视图）
import TerminalWindowView from './views/terminal/TerminalWindowView.vue'
import taskModalCss from './components/task-queue-modal.css?inline'
// 开源 Vue3 日期/时间选择组件（替代原生 datetime-local 控件，样式可随主题定制）
import datepickerCss from '@vuepic/vue-datepicker/dist/main.css?inline'
// 终端基础样式（票 02）：宿主 TerminalPreview 同款导入。宿主 vite 是 app 模式直接产出
// CSS 文件，插件是 lib 模式——vite 会把 CSS 提取为独立 asset，由本插件 vite.config.ts 的
// inlinePluginCss() 内联进 index.js 并在加载时自注入 document.head（task-modal 同机制，
// 无 url()/@import 引用，内联安全）；终端窗口为独立 WebviewWindow，各自加载插件入口时
// 自带样式，不依赖宿主 bundle 的 CSS。
import '@xterm/xterm/css/xterm.css'
// 宿主 OS 平台：任务域 hooks 按平台选择 Python 解释器命令（Windows=python，
// Linux/macOS=python3），通过 @tauri-apps/plugin-os 读取（同步 API，宿主已注册该插件）
import { platform } from '@tauri-apps/plugin-os'
import { currentSessionId, sharedRouter } from './utils/route'
import { taskModalVisible } from './state'
import { messages } from './i18n'
import sessionDevMock from './devMock'
import { startDeviceNotifications } from './notifications'
import type { Disposable, PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'

// dev-shell 领域种子数据（SDK PluginDevMock 协议；真实宿主忽略）
export const devMock = sessionDevMock

/**
 * 宿主内置菜单槽位（`BUILTIN_MENU_ORDERS`）与设置分组槽位
 * （`BUILTIN_SECTION_ORDERS`）。
 *
 * 插件不引宿主模块，故此常量按值复制并锁定：让位判据是「同 order 值」
 * （宿主 `builtinSupersededBy`），槽位写错即变成「插队」而非「接管」——
 * 界面会出现两个同域入口。宿主常量变更时此处必须同步（票 18 核对）。
 */
const DEVICES_SLOT_ORDER = 100
const SESSIONS_SLOT_ORDER = 200
/** 连接历史紧随设备与配对之后（spec D6：同域其余项取 101+） */
const DEVICES_HISTORY_ORDER = DEVICES_SLOT_ORDER + 1
/**
 * 任务历史排在终端会话之后、服务器（内置 300，保留不复用）之前——
 * 沿用旧 auto-task 插件的槽位 210，合并后菜单顺序不变（spec D6）。
 */
const TASKS_ORDER = SESSIONS_SLOT_ORDER + 10
/** 设置分组槽位：顶替宿主内置「配对设置」分组的原位（该内置分组已退役） */
const PAIRING_SECTION_ORDER = 200
/**
 * 设置分组槽位：顶替宿主内置「会话」分组的原位（宿主 `BUILTIN_SECTION_ORDERS.session`
 * = 400，该内置分组随域下沉退役，槽位值保留不复用）
 */
const SESSION_SECTION_ORDER = 400

/** 侧边栏图标：宿主同款 Heroicons outline path（同图标体系） */
const PAIRING_ICON = 'M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z'
const HISTORY_ICON = 'M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z'
const SESSIONS_ICON =
  'M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z'
const TASKS_ICON =
  'M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01'

/** 贡献的侧边栏目录（id 与 manifest `contributes.views` 一一对应）
 * `kind`：'sidebar' = 侧边栏菜单目录；'page' = 不进菜单、仅路由可达的二级页面
 * （票 14 收尾：连接历史从侧边栏移除，改由设备列表的「历史」按钮经 query 深链直达） */
interface PanelSpec {
  id: string
  titleKey: string
  order: number
  icon: string
  component: unknown
  kind: 'sidebar' | 'page'
}

const PANELS: PanelSpec[] = [
  {
    id: 'session.pairing',
    titleKey: 'pairing.sidebar.title',
    order: DEVICES_SLOT_ORDER,
    icon: PAIRING_ICON,
    component: DeviceCenterView,
    kind: 'sidebar',
  },
  {
    id: 'session.history',
    titleKey: 'pairing.history.sidebar.title',
    order: DEVICES_HISTORY_ORDER,
    icon: HISTORY_ICON,
    component: ConnectionHistoryView,
    kind: 'page',
  },
  {
    id: 'session.sidebar',
    titleKey: 'session.sidebar.title',
    order: SESSIONS_SLOT_ORDER,
    icon: SESSIONS_ICON,
    component: SessionCenterView,
    kind: 'sidebar',
  },
  {
    id: 'session.task-history',
    titleKey: 'task.historyTitle',
    order: TASKS_ORDER,
    icon: TASKS_ICON,
    component: TaskHistoryView,
    kind: 'sidebar',
  },
  {
    // 终端窗口视图（票 03a）：不进侧边栏菜单，仅宿主 /terminal-window/:id 路由
    // 深链直达（同 session.history 的 type='page' 先例）
    id: 'session.terminal-window',
    titleKey: 'session.terminal.windowTitle',
    order: 0,
    icon: '',
    component: TerminalWindowView,
    kind: 'page',
  },
]

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

// ==================== 贡献面注册 ====================

let disposables: { dispose(): void }[] = []
/** 入口自身的事件订阅（与 registerPluginUi 重注册的贡献面分开释放，互不覆盖） */
let eventDisposables: { dispose(): void }[] = []
let stopLocaleWatch: (() => void) | null = null
let stopRouteWatch: (() => void) | null = null
let toolbarDisposable: { dispose(): void } | null = null
/** 设备连接通知订阅句柄（激活期常驻，停用时注销） */
let deviceNotifications: Disposable | null = null

/**
 * 注册侧边栏目录、插件页与设置分组
 *
 * 注册时标题被宿主静态捕获（labelKey 非 i18n key，不随 vue-i18n 自动更新），
 * 语言切换时先释放旧注册再重新注册，菜单与分组标题即时刷新。
 */
function registerPluginUi(context: PluginContext): void {
  for (const d of disposables) {
    d.dispose()
  }
  disposables = PANELS.map((panel) =>
    panel.kind === 'page'
      ? context.ui.registerPage({
          id: panel.id,
          title: context.i18n.t(panel.titleKey),
          order: panel.order,
          icon: panel.icon,
          component: panel.component as never,
        })
      : context.ui.registerSidebarPanel({
          id: panel.id,
          title: context.i18n.t(panel.titleKey),
          order: panel.order,
          icon: panel.icon,
          component: panel.component as never,
        }),
  )
  disposables.push(
    context.ui.registerSettingsSection({
      id: 'pairing.settings',
      titleKey: 'pairing.settings.title',
      order: PAIRING_SECTION_ORDER,
      component: PairingSettingsSection,
    }),
    // 会话默认值分组：宿主内置「会话」分组（失效 UI——改的是无人消费的宿主设置）
    // 随域下沉到本插件，写插件存储 `session.formDefaults`（新建表单的真实默认值源）
    context.ui.registerSettingsSection({
      id: 'session.settings',
      titleKey: 'session.settings.title',
      order: SESSION_SECTION_ORDER,
      component: SessionSettingsSection,
    }),
  )
}

// ==================== 终端工具栏入口（仅任务域适配的 agent 会话） ====================

// 异步同步序号：路由快速切换时丢弃过期结果，避免旧会话的 agent 覆盖新状态
let toolbarSyncSeq = 0

// 按当前路由会话的 agent 动态注册/注销工具栏入口（路由切换时重新评估）。
// 判据直接取后端 `session.task.running-sessions` 的 is_supported 字段，
// 不在前端 hardcode 白名单——权威来源是 Rust 侧 AGENT_PROFILES。
async function syncToolbarEntry(context: PluginContext) {
  const seq = ++toolbarSyncSeq
  const id = currentSessionId()
  if (!id) {
    toolbarDisposable?.dispose()
    toolbarDisposable = null
    return
  }
  try {
    const result: any = await context.commands.execute('session.task.running-sessions')
    if (seq !== toolbarSyncSeq) return // 过期结果丢弃
    const match = (result?.sessions ?? []).find((s: any) => s.session_id === id)
    const shouldShow = match?.is_supported ?? false
    if (shouldShow && !toolbarDisposable) {
      toolbarDisposable = context.ui.registerTerminalToolbarItem({
        id: 'session.task.open-modal',
        label: context.i18n.t('task.title'),
        onClick: () => {
          taskModalVisible.value = true
        },
      })
    } else if (!shouldShow && toolbarDisposable) {
      toolbarDisposable.dispose()
      toolbarDisposable = null
    }
  } catch (e) {
    console.warn('[Session Center/task] failed to sync toolbar entry:', e)
  }
}

// ==================== 弹窗挂载与样式注入 ====================

let modalApp: App | null = null
let modalContainer: HTMLElement | null = null

/** 样式注入：插件构建的 CSS 不会被宿主自动加载，运行时手动注入 document.head */
function injectStyle(id: string, css: string) {
  if (document.getElementById(id)) return
  const styleEl = document.createElement('style')
  styleEl.id = id
  styleEl.textContent = css
  document.head.appendChild(styleEl)
}

function removeStyle(id: string) {
  document.getElementById(id)?.remove()
}

/** 挂载任务队列弹窗（每个 webview 独立实例，常驻 document.body） */
function mountModal(context: PluginContext) {
  if (modalApp) return
  modalContainer = document.createElement('div')
  document.body.appendChild(modalContainer)
  modalApp = createApp(TaskQueueModal)
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

// ==================== 事件留痕（任务状态 / 会话模式） ====================

// 弹窗与历史视图各自注册数据刷新监听；入口这两条只做运行日志留痕，
// 供排查「事件到没到前端」时对照，不参与渲染
function onTaskStatusChanged(data: any) {
  const { taskStatus, taskReason } = data ?? {}
  console.log(
    `[Session Center/task] 状态变更: ${taskStatus}${taskReason ? ` - ${taskReason}` : ''}`,
  )
}

function onSessionModeChanged(data: any) {
  console.log(`[Session Center/task] 模式变更: ${data?.autoApprove ? '自动授权' : '手动模式'}`)
}

// ==================== 生命周期 ====================

export async function activate(context: PluginContext): Promise<void> {
  // 上报宿主平台：任务域 hooks 按平台选择 Python 解释器命令（Windows=python，
  // Linux/macOS=python3）。失败仅告警，不影响激活（后端默认回退 python）
  try {
    await context.commands.execute('session.task.set-platform', { platform: platform() })
  } catch (e) {
    console.warn('[Session Center/task] failed to report host platform:', e)
  }

  // 注册 i18n 消息（自动添加插件 ID 前缀 → com.bedcode.terminal-session.task.* 等），
  // 必须在组件 setup 前完成，保证模板取文案可用
  for (const [locale, msgs] of Object.entries(messages)) {
    context.i18n.registerMessages(locale, msgs)
  }

  // 注入弹窗与日期选择器样式（基础样式 + 主题覆盖）
  injectStyle('session-task-modal-style', taskModalCss)
  injectStyle('session-task-datepicker-style', datepickerCss + DATEPICKER_THEME_OVERRIDES)

  registerPluginUi(context)

  // 工具栏入口仅对任务域适配的 agent 会话显示：监听终端窗口路由切换动态注册/注销
  const router = sharedRouter()
  if (router) {
    stopRouteWatch = watch(
      () => router.currentRoute?.value?.params?.id,
      () => syncToolbarEntry(context),
    )
    syncToolbarEntry(context)
  }

  // 宿主语言切换时重注册贡献面：标题在注册时被静态捕获，不随 vue-i18n 自动更新。
  // 工具栏标题同样静态捕获，故注销后重新评估当前会话 agent 再注册
  const hostI18n = context.i18n.getI18n()
  stopLocaleWatch = watch(
    () => hostI18n?.global?.locale?.value,
    () => {
      registerPluginUi(context)
      toolbarDisposable?.dispose()
      toolbarDisposable = null
      syncToolbarEntry(context)
    },
  )

  // 挂载任务队列弹窗（i18n 已注册，组件 setup 可正常取文案）
  mountModal(context)

  // 任务状态与模式变更留痕（弹窗与历史视图各自注册数据刷新监听）。
  // 两条 disposable 必须收好：注册表在 deactivate 后才清空，漏释放即监听跨插件
  // 生命周期存活，重载插件会重复计数
  eventDisposables.push(context.events.on('task:status-changed', onTaskStatusChanged))
  eventDisposables.push(context.events.on('session:mode-changed', onSessionModeChanged))

  // 设备上下线通知：设备域归属本插件，宿主 `useGlobalNotifications` 已退役，
  // 不再替本域弹 toast（启动期经 connect-list 种子化在线基线）
  deviceNotifications = startDeviceNotifications(context)

  console.log('[Session Center] Plugin activated')
}

export async function deactivate(): Promise<void> {
  stopRouteWatch?.()
  stopRouteWatch = null
  stopLocaleWatch?.()
  stopLocaleWatch = null
  toolbarDisposable?.dispose()
  toolbarDisposable = null
  deviceNotifications?.dispose()
  deviceNotifications = null
  for (const d of disposables) {
    d.dispose()
  }
  disposables = []
  unmountModal()
  for (const d of eventDisposables) {
    d.dispose()
  }
  eventDisposables = []
  removeStyle('session-task-modal-style')
  removeStyle('session-task-datepicker-style')
  console.log('[Session Center] Plugin deactivated')
}
