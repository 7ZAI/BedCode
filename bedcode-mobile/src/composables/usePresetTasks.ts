/**
 * Preset Tasks Composable
 *
 * 预设任务数据管理 - CRUD、localStorage 持久化、执行状态（ADR-0008）
 *
 * 执行状态语义：入队即视为已执行（可靠信号），服务端完成广播只做状态细化。
 * 状态机为纯函数（presetTaskState.ts），本模块负责持久化与事件应用。
 */

import { ref } from 'vue'
import { httpSendSessionInput, httpTaskQueueList } from '@/composables/useHttpApi'
import {
  enqueue,
  taskDone,
  manualExecute,
  queueItemRemoved,
  edit,
  reconcile,
  canEnqueue as canEnqueueState,
} from './presetTaskState'
import type { PresetTask, PresetTaskStatus } from './model'

const STORAGE_KEY = 'preset-tasks'

const tasks = ref<PresetTask[]>([])

/** 旧数据迁移：补齐执行状态字段（旧任务无属性，默认可重复，行为不变） */
function migrateTask(t: Partial<PresetTask>): PresetTask {
  return {
    id: t.id ?? '',
    content: t.content ?? '',
    createdAt: t.createdAt ?? '',
    updatedAt: t.updatedAt ?? '',
    repeatable: t.repeatable ?? true,
    status: t.status ?? 'unused',
    pendingTaskId: t.pendingTaskId ?? null,
    pendingSessionId: t.pendingSessionId ?? null,
  }
}

/** 从 localStorage 读取任务列表（migrateTask 负责逐字段兜底与旧数据迁移） */
function loadFromStorage(): PresetTask[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return []
    return (JSON.parse(raw) as Partial<PresetTask>[]).map(migrateTask)
  } catch {
    return []
  }
}

/** 写入 localStorage */
export function saveToStorage() {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(tasks.value))
}

/** 从 localStorage 加载到响应式状态 */
export async function load() {
  tasks.value = loadFromStorage()
}

/** 添加预设任务 */
export async function addTask(input: { content: string; repeatable?: boolean }) {
  const now = new Date().toISOString()
  const task: PresetTask = {
    id: crypto.randomUUID(),
    content: input.content,
    createdAt: now,
    updatedAt: now,
    repeatable: input.repeatable ?? true,
    status: 'unused',
    pendingTaskId: null,
    pendingSessionId: null,
  }
  tasks.value.push(task)
  saveToStorage()
}

/** 更新预设任务；内容变化视为重新定义任务，执行状态重置为未使用 */
export async function updateTask(task: PresetTask) {
  const index = tasks.value.findIndex(t => t.id === task.id)
  if (index === -1) return
  const prev = tasks.value[index]
  const contentChanged = prev.content !== task.content
  tasks.value[index] = migrateTask({
    ...task,
    // 内容变了 → 状态机 edit 语义（unused + 清除队列关联；仅改属性不重置）
    ...(contentChanged ? edit(prev) : {}),
    updatedAt: new Date().toISOString(),
  })
  saveToStorage()
}

/** 删除预设任务 */
export async function deleteTask(id: string) {
  tasks.value = tasks.value.filter(t => t.id !== id)
  saveToStorage()
}

/** 发送任务内容到终端（不按回车）——不改变执行状态（未真正执行） */
export async function sendTask(task: PresetTask, sessionId: string) {
  const result = await httpSendSessionInput(sessionId, task.content)
  if (result.code !== 0) {
    throw new Error('mobile.toolbox.sendFailed')
  }
}

/** 执行任务内容到终端（按回车）；成功即本地标记已执行（manualExecute） */
export async function executeTask(task: PresetTask, sessionId: string) {
  const result = await httpSendSessionInput(sessionId, task.content, 'enter')
  if (result.code !== 0) {
    throw new Error('mobile.toolbox.sendFailed')
  }
  const index = tasks.value.findIndex(t => t.id === task.id)
  if (index !== -1) {
    tasks.value[index] = migrateTask({ ...tasks.value[index], ...manualExecute(tasks.value[index]) })
    saveToStorage()
  }
}

/** 清除所有预设任务 */
export function clearAllTasks() {
  tasks.value = []
  saveToStorage()
}

// ==================== 执行状态操作（插件面板/工具箱经 SDK 调用） ====================

/** 入队成功：记录队列项 id 与会话，状态 → executing */
export function markEnqueued(id: string, taskId: string, sessionId: string) {
  const index = tasks.value.findIndex(t => t.id === id)
  if (index === -1) return
  tasks.value[index] = migrateTask({
    ...tasks.value[index],
    ...enqueue(tasks.value[index], taskId),
    pendingSessionId: sessionId,
  })
  saveToStorage()
}

/** 完成广播：按队列项 id 匹配预设，状态 → completed（不匹配忽略） */
export function markCompletedByTaskId(taskId: string) {
  const index = tasks.value.findIndex(t => t.pendingTaskId === taskId)
  if (index === -1) return
  tasks.value[index] = migrateTask({ ...tasks.value[index], ...taskDone(tasks.value[index], taskId) })
  saveToStorage()
}

/** 对账：单个预设落中断（调用方已确认其队列项不在桌面 pending 队列，幂等） */
export function markInterrupted(id: string) {
  const index = tasks.value.findIndex(t => t.id === id)
  if (index === -1) return
  tasks.value[index] = migrateTask({ ...tasks.value[index], ...reconcile(tasks.value[index]) })
  saveToStorage()
}

/** 按队列项 id 落中断（桌面端超时取消/会话终止广播）：匹配预设 → interrupted（不匹配忽略） */
export function markInterruptedByTaskId(taskId: string) {
  const index = tasks.value.findIndex(t => t.pendingTaskId === taskId)
  if (index === -1) return
  tasks.value[index] = migrateTask({ ...tasks.value[index], ...reconcile(tasks.value[index]) })
  saveToStorage()
}

/** 队列项移除（面板删除/清空）：按队列项 id 回退未使用（不匹配忽略） */
export function revertToUnusedByTaskId(taskId: string) {
  const index = tasks.value.findIndex(t => t.pendingTaskId === taskId)
  if (index === -1) return
  tasks.value[index] = migrateTask({ ...tasks.value[index], ...queueItemRemoved(tasks.value[index], taskId) })
  saveToStorage()
}

/**
 * 对账：把「执行中且队列项已不在指定会话 pending 队列」的预设落为中断
 *
 * 仅判定 pendingSessionId === sessionId 的预设（防多会话误中断：入队到会话 A、
 * 打开面板时活动会话为 B 的任务保持执行中，等待其自身会话的对账/广播）。
 * 面板打开与工具箱页进入时调用（spec：面板打开 + 应用启动后的首次进入）。
 */
export async function reconcileWithQueue(sessionId: string) {
  if (!sessionId) return
  let result
  try {
    result = await httpTaskQueueList(sessionId)
  } catch {
    // 拉取失败不误判：保持现状，等待下次对账
    return
  }
  if (result.code !== 0) return
  const pendingIds = new Set((result.data?.tasks ?? []).map(q => q.id))
  // 处理中的队列项（waiting/executing）不算丢失：桌面端返回 active_task，
  // 对账仅覆盖"已不在队列且未在处理中"的预设，避免任务执行中误落中断
  const activeId = result.data?.active_task?.id ?? null
  for (const task of tasks.value) {
    if (
      task.status === 'executing' &&
      task.pendingSessionId === sessionId &&
      task.pendingTaskId &&
      !pendingIds.has(task.pendingTaskId) &&
      task.pendingTaskId !== activeId
    ) {
      markInterrupted(task.id)
    }
  }
}

export function usePresetTasks() {
  return {
    tasks,
    load,
    addTask,
    updateTask,
    deleteTask,
    sendTask,
    executeTask,
    clearAllTasks,
    saveToStorage,
    markEnqueued,
    markCompletedByTaskId,
    markInterrupted,
    markInterruptedByTaskId,
    revertToUnusedByTaskId,
    reconcileWithQueue,
    canEnqueue: canEnqueueState,
  }
}
