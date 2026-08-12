/**
 * Auto Task 插件入口 (Mobile)
 *
 * 终端工具栏按钮 + 自渲染任务队列面板（AutoTaskPanelHost.vue）。
 * 面板经 createApp 挂载到 document.body（与桌面端 AutoTaskModal 一致），
 * 通过 provide/inject 传递 PluginContext。
 *
 * 工具栏入口可见性：仅当当前终端会话使用已适配的 agent 时才显示，
 * 避让未适配的 agent（避免用户打开后发现功能不可用）。
 * 适配 agent 列表通过后端 HTTP 端点获取（权威来源：Rust AGENT_PROFILES）。
 */
import { createApp, type App, watch } from 'vue'
import type { PluginContext, PluginDevMock } from '@bedcode/plugin-sdk-mobile'
import { getMobileApi } from '@bedcode/plugin-sdk-mobile'
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
// 深色模式由 Datepicker 的 dark prop 切换 .dp__theme_dark，此处覆盖其默认深色变量
const DATEPICKER_THEME_OVERRIDES = `
/* 输入框与宿主控件保持一致（高度 44px、圆角 10px，与自绘输入框同规格） */
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
.dp__theme_dark {
  --dp-background-color: var(--mobile-bg-card);
  --dp-text-color: var(--mobile-text-primary);
  --dp-hover-color: var(--mobile-bg-tertiary);
  --dp-hover-text-color: var(--mobile-text-primary);
  --dp-hover-icon-color: var(--mobile-text-primary);
  --dp-border-color: var(--mobile-border);
  --dp-border-color-hover: var(--mobile-border-hover);
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
}
.dp__menu {
  font-size: 14px;
}
`

/**
 * dev-shell 领域数据：任务队列种子（浏览器 mock 宿主初始队列）
 * 仅 dev-shell 消费（见 PluginDevMock 协议），真实宿主忽略此导出
 */
export const devMock: PluginDevMock = {
  queueSeed: [
    {
      id: 'dev-queue-1',
      prompt: '查看当前目录文件列表',
      position: 1,
      status: 'pending',
      created_at: new Date().toISOString(),
    },
    {
      id: 'dev-queue-2',
      prompt: '输出系统信息',
      position: 2,
      status: 'pending',
      created_at: new Date().toISOString(),
    },
  ],
}

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
  if (!document.getElementById('auto-task-panel-style')) {
    const styleEl = document.createElement('style')
    styleEl.id = 'auto-task-panel-style'
    styleEl.textContent = panelCss
    document.head.appendChild(styleEl)
  }
  if (!document.getElementById('auto-task-toolbox-style')) {
    const styleEl = document.createElement('style')
    styleEl.id = 'auto-task-toolbox-style'
    styleEl.textContent = toolboxCss
    document.head.appendChild(styleEl)
  }
  // 日期选择器样式（基础样式 + 主题覆盖，同样运行时注入）
  if (!document.getElementById('auto-task-datepicker-style')) {
    const styleEl = document.createElement('style')
    styleEl.id = 'auto-task-datepicker-style'
    styleEl.textContent = datepickerCss + DATEPICKER_THEME_OVERRIDES
    document.head.appendChild(styleEl)
  }
}

// ==================== 工具栏入口可见性（仅插件适配的 agent 会话） ====================

// 异步同步序号：会话快速切换时丢弃过期结果，避免旧会话的 agent 覆盖新状态
let toolbarSyncSeq = 0

// 从后端获取的适配 agent 列表（权威来源：Rust AGENT_PROFILES），activate 时缓存
let supportedAgents: string[] = []

/** 按当前活动会话的 agent 动态注册/注销工具栏入口 */
async function syncToolbarEntry(context: PluginContext) {
  const seq = ++toolbarSyncSeq
  const mobileApi = getMobileApi()
  const sessionId = mobileApi.activeSessionId?.value
  if (!sessionId) {
    if (toolbarDisposable) {
      toolbarDisposable.dispose()
      toolbarDisposable = null
    }
    return
  }

  // 从会话配置命令识别 agent，再用后端返回的 supportedAgents 白名单判断
  const sessions = mobileApi.activeSessions?.value || []
  const session = sessions.find((s: any) => s.id === sessionId)
  const configId = session?.config_id || session?.configId
  if (!configId) {
    if (toolbarDisposable) {
      toolbarDisposable.dispose()
      toolbarDisposable = null
    }
    return
  }

  const configs = mobileApi.sessionConfigs?.value || []
  const config = configs.find((c: any) => c.id === configId)
  if (!config?.command) {
    if (toolbarDisposable) {
      toolbarDisposable.dispose()
      toolbarDisposable = null
    }
    return
  }

  if (seq !== toolbarSyncSeq) return // 过期结果丢弃

  // 识别 agent 并判断是否在后端白名单中
  const lower = config.command.toLowerCase()
  let agent = 'unknown'
  if (lower.includes('claude')) agent = 'claude'
  else if (lower.includes('codex')) agent = 'codex'
  else if (lower.includes('opencode')) agent = 'opencode'
  else {
    const firstToken = lower.split(/\s+/)[0] || ''
    const basename = firstToken.split(/[\\/]/).pop() || ''
    if (basename.replace(/\.exe$/i, '') === 'pi') agent = 'pi'
  }

  const shouldShow = supportedAgents.includes(agent)
  if (shouldShow && !toolbarDisposable) {
    toolbarDisposable = context.ui.registerTerminalToolbarItem({
      id: 'auto-task-toolbar',
      label: context.i18n.t('title'),
      icon: 'M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 7l2 2 4-4',
      onClick: () => {
        autoTaskPanelVisible.value = !autoTaskPanelVisible.value
      },
    })
  } else if (!shouldShow && toolbarDisposable) {
    toolbarDisposable.dispose()
    toolbarDisposable = null
  }
}

let toolbarDisposable: { dispose(): void } | null = null
let toolboxDisposable: { dispose(): void } | null = null
let stopSessionWatch: (() => void) | null = null
let stopConnectionWatch: (() => void) | null = null

// ==================== 激活 ====================

export async function activate(context: PluginContext): Promise<void> {
  _ctx = context
  context.logger.info('Auto Task plugin activating...')

  // 注册 i18n 消息（自动添加插件 ID 前缀），必须在面板组件 setup 前完成
  for (const [locale, msgs] of Object.entries(messages)) {
    context.i18n.registerMessages(locale, msgs)
  }

  // 从后端获取适配 agent 白名单（权威来源 Rust AGENT_PROFILES），缓存后供 syncToolbarEntry 使用
  const mobileApi = getMobileApi()

  /** 拉取适配 agent 白名单并缓存（未连接时对端不可达，失败保持旧值） */
  async function refreshSupportedAgents() {
    try {
      const result = await mobileApi.httpListSupportedAgents()
      if (result.code === 0 && result.data) {
        supportedAgents = result.data.agents || []
      }
    } catch (e) {
      console.warn('[AutoTask] Failed to load supported agents:', e)
    }
  }

  // 启动时未连接则跳过首次请求（对空 baseUrl 发起 REST 调用只会命中宿主的
  // "No base URL set" 错误日志）；连接建立后再拉取
  if (mobileApi.isConnected?.value) {
    await refreshSupportedAgents()
  }

  // 连接建立后白名单才有意义：监听连接状态，连上时拉取并重算工具栏入口
  // （避免 connect 后 supportedAgents 仍为空导致适配 agent 入口永远不显示）
  stopConnectionWatch = watch(
    () => mobileApi.isConnected?.value,
    (connected) => {
      if (!connected) return
      void (async () => {
        await refreshSupportedAgents()
        syncToolbarEntry(context)
      })()
    },
  )

  // 工具栏入口仅对插件适配的 agent 会话显示：监听活动会话变化动态注册/注销
  stopSessionWatch = watch(
    () => mobileApi.activeSessionId?.value,
    () => syncToolbarEntry(context),
    { immediate: true },
  )

  // 挂载面板（i18n 已注册，组件 setup 可正常取文案）
  injectPanelStyle()
  mountPanel(context)

  // 工具箱页（任务记录 + 定时任务两页签）：manifest-gen 扫描 registerToolboxPage 自动补全
  // contributes.views 与 ui:toolbox 权限；权限未授予时注册抛错，不影响插件其余能力
  try {
    toolboxDisposable = context.ui.registerToolboxPage({
      id: 'auto-task.toolbox',
      title: context.i18n.t('toolboxTitle'),
      icon: 'M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 7l2 2 4-4',
      component: AutoTaskToolboxView,
    })
  } catch (e) {
    context.logger.warn(`Toolbox page registration failed: ${e}`)
    toolboxDisposable = null
  }

  context.logger.info('Auto Task plugin activated')
}

export async function deactivate(): Promise<void> {
  stopConnectionWatch?.()
  stopConnectionWatch = null
  stopSessionWatch?.()
  stopSessionWatch = null
  toolbarDisposable?.dispose()
  toolbarDisposable = null
  toolboxDisposable?.dispose()
  toolboxDisposable = null
  unmountPanel()
  document.getElementById('auto-task-panel-style')?.remove()
  document.getElementById('auto-task-toolbox-style')?.remove()
  document.getElementById('auto-task-datepicker-style')?.remove()
  _ctx?.logger.info('Auto Task plugin deactivated')
}
