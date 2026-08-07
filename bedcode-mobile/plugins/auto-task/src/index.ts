/**
 * Auto Task 插件入口 (Mobile)
 *
 * 终端工具栏按钮 + 自渲染任务队列面板（AutoTaskPanelHost.vue）。
 * 面板经 createApp 挂载到 document.body（与桌面端 AutoTaskModal 一致），
 * 通过 provide/inject 传递 PluginContext。
 */
import { createApp, type App } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'
import AutoTaskPanelHost from './components/AutoTaskPanelHost.vue'
import { autoTaskPanelVisible } from './state'
import { messages } from './i18n'
import panelCss from './panel.css?inline'

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

/** 注入面板样式（宿主不加载插件 dist/style.css，运行时注入一次） */
function injectPanelStyle() {
  if (document.getElementById('auto-task-panel-style')) return
  const styleEl = document.createElement('style')
  styleEl.id = 'auto-task-panel-style'
  styleEl.textContent = panelCss
  document.head.appendChild(styleEl)
}

// ==================== 激活 ====================

export async function activate(context: PluginContext): Promise<void> {
  _ctx = context
  context.logger.info('Auto Task plugin activating...')

  // 注册 i18n 消息（自动添加插件 ID 前缀），必须在面板组件 setup 前完成
  for (const [locale, msgs] of Object.entries(messages)) {
    context.i18n.registerMessages(locale, msgs)
  }

  // 图标使用 SVG path（Heroicons outline 风格，与终端工具栏按钮一致），不使用 emoji
  context.ui.registerTerminalToolbarItem({
    id: 'auto-task-toolbar',
    label: context.i18n.t('title'),
    icon: 'M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 7l2 2 4-4',
    onClick: () => {
      autoTaskPanelVisible.value = !autoTaskPanelVisible.value
    },
  })

  // 挂载面板（i18n 已注册，组件 setup 可正常取文案）
  injectPanelStyle()
  mountPanel(context)

  context.logger.info('Auto Task plugin activated')
}

export async function deactivate(): Promise<void> {
  unmountPanel()
  document.getElementById('auto-task-panel-style')?.remove()
  _ctx?.logger.info('Auto Task plugin deactivated')
}
