/**
 * Preset Task Execution State — 预设任务执行状态状态机（纯函数，无 Vue/localStorage 依赖）
 *
 * 领域语义（见 CONTEXT.md「可重复 / 已执行 / 执行状态」与 ADR-0008）：
 * - 锁定依据 = 用户本地操作（入队即视为已执行），不依赖服务端终态通知
 * - 服务端完成广播只做状态细化（taskDone → completed）；对账兜底（reconcile）
 *   把无完成记录的执行中任务落为 interrupted
 * - 删除/清空队列项回退未使用；编辑内容重置未使用
 *
 * 本模块为纯函数：输入旧状态 + 事件，输出新状态；不可变更新，未命中事件返回原引用。
 */

import type { PresetTaskStatus } from './model'

/** 预设任务执行状态与队列项关联 */
export interface PresetTaskExecState {
  status: PresetTaskStatus
  /** 入队时记录的队列项 id（桌面端 httpTaskQueueAdd 返回），用于广播/移除事件匹配 */
  pendingTaskId: string | null
}

/** 入队：任意可入队预设 → executing，记录新 taskId（覆盖旧记录） */
export function enqueue(state: PresetTaskExecState, taskId: string): PresetTaskExecState {
  return { status: 'executing', pendingTaskId: taskId }
}

/** 完成广播：taskId 匹配 → completed 并清除记录；不匹配（孤儿/手动输入项）→ 原样 */
export function taskDone(state: PresetTaskExecState, taskId: string): PresetTaskExecState {
  if (state.pendingTaskId !== taskId) return state
  return { status: 'completed', pendingTaskId: null }
}

/** 对账：executing → interrupted（幂等，其余状态原样） */
export function reconcile(state: PresetTaskExecState): PresetTaskExecState {
  if (state.status !== 'executing') return state
  return { ...state, status: 'interrupted' }
}

/** 手动执行成功（toolbox 直接发送）：→ completed，清除记录 */
export function manualExecute(state: PresetTaskExecState): PresetTaskExecState {
  return { status: 'completed', pendingTaskId: null }
}

/** 队列项移除（面板删除/清空该队列项）：taskId 匹配 → unused 并清除记录；不匹配原样 */
export function queueItemRemoved(state: PresetTaskExecState, taskId: string): PresetTaskExecState {
  if (state.pendingTaskId !== taskId) return state
  return { status: 'unused', pendingTaskId: null }
}

/** 编辑内容：任意状态 → unused，清除记录（内容变了即重新定义任务） */
export function edit(state: PresetTaskExecState): PresetTaskExecState {
  return { status: 'unused', pendingTaskId: null }
}

/** 锁定判定：不可重复 + 已执行（执行中/已完成/已中断）→ 锁定；可重复 + 执行中 → 防重复锁定 */
export function canEnqueue(task: { repeatable: boolean; status: PresetTaskStatus }): boolean {
  if (task.status === 'executing') return false
  if (!task.repeatable && task.status !== 'unused') return false
  return true
}

/** 从预设任务列表中筛选可入队子集 */
export function filterEnqueueable<T extends { repeatable: boolean; status: PresetTaskStatus }>(
  tasks: T[],
): T[] {
  return tasks.filter(canEnqueue)
}
