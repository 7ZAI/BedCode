/**
 * Auto Task 插件入口
 *
 * Claude Code 任务状态同步与自动授权
 * Rust+TS 双层架构：Rust WASM 处理后端逻辑，TS 负责 UI 和 toast 通知
 *
 * UI 入口：
 * - 终端工具栏按钮（registerTerminalToolbarItem）→ 打开自动任务队列弹窗
 * - 侧边栏任务历史视图（registerSidebarPanel）
 */
import { createApp, type App } from 'vue'
import TaskHistoryView from './components/TaskHistoryView.vue'
import AutoTaskModal from './components/AutoTaskModal.vue'
import autoTaskModalCss from './components/auto-task-modal.css?inline'
import { autoTaskModalVisible } from './state'
import { messages } from './i18n'
import type { PluginContext } from '@bedcode/plugin-sdk-desktop'

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

  // 注册侧边栏面板 — 任务历史（标题经 i18n 解析，注册时取当前语言）
  context.ui.registerSidebarPanel({
    id: 'auto-task.history',
    title: context.i18n.t('historyTitle'),
    icon: 'M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01',
    component: TaskHistoryView,
  })

  // 注册终端工具栏按钮 — 打开自动任务队列弹窗
  context.ui.registerTerminalToolbarItem({
    id: 'auto-task.open-modal',
    label: context.i18n.t('title'),
    onClick: () => {
      autoTaskModalVisible.value = true
    },
  })

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
  unmountModal()
  document.getElementById('auto-task-modal-style')?.remove()
  console.log('[Auto Task] Plugin deactivated')
}
