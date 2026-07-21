/**
 * Auto Task 插件入口
 *
 * Claude Code 任务状态同步与自动授权
 * Rust+TS 双层架构：Rust WASM 处理后端逻辑，TS 负责 UI 和 toast 通知
 */
import TaskHistoryView from './components/TaskHistoryView.vue'
import type { PluginContext } from '@bedcode/plugin-sdk-desktop'

export async function activate(context: PluginContext): Promise<void> {
  // 注册侧边栏面板 — 任务历史
  context.ui.registerSidebarPanel({
    id: 'auto-task.history',
    title: '任务历史',
    icon: 'M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01',
    component: TaskHistoryView,
  })

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
  console.log('[Auto Task] Plugin deactivated')
}
