/**
 * Preset Tasks Composable
 *
 * 预设任务数据管理 - CRUD、localStorage 持久化
 */

import { ref } from 'vue'
import { httpSendSessionInput } from '@/composables/useHttpApi'
import type { PresetTask } from './model'

const STORAGE_KEY = 'preset-tasks'

const tasks = ref<PresetTask[]>([])

/** 从 localStorage 读取任务列表 */
function loadFromStorage(): PresetTask[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw) as PresetTask[]
    // 兼容旧数据：移除 type/status 字段
    return parsed.map(t => ({
      id: t.id,
      title: t.title,
      content: t.content,
      createdAt: t.createdAt,
      updatedAt: t.updatedAt,
    }))
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
export async function addTask(input: { title: string; content: string }) {
  const now = new Date().toISOString()
  const task: PresetTask = {
    id: crypto.randomUUID(),
    title: input.title,
    content: input.content,
    createdAt: now,
    updatedAt: now,
  }
  tasks.value.push(task)
  saveToStorage()
}

/** 更新预设任务 */
export async function updateTask(task: PresetTask) {
  const index = tasks.value.findIndex(t => t.id === task.id)
  if (index === -1) return
  tasks.value[index] = {
    ...task,
    updatedAt: new Date().toISOString(),
  }
  saveToStorage()
}

/** 删除预设任务 */
export async function deleteTask(id: string) {
  tasks.value = tasks.value.filter(t => t.id !== id)
  saveToStorage()
}

/** 发送任务内容到终端（不按回车） */
export async function sendTask(task: PresetTask, sessionId: string) {
  const result = await httpSendSessionInput(sessionId, task.content)
  if (result.code !== 0) {
    throw new Error('mobile.toolbox.sendFailed')
  }
}

/** 执行任务内容到终端（按回车） */
export async function executeTask(task: PresetTask, sessionId: string) {
  const result = await httpSendSessionInput(sessionId, task.content, 'enter')
  if (result.code !== 0) {
    throw new Error('mobile.toolbox.sendFailed')
  }
}

/** 清除所有预设任务 */
export function clearAllTasks() {
  tasks.value = []
  saveToStorage()
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
  }
}
