/**
 * Preset Tasks Composable
 *
 * 预设任务数据管理 - CRUD、状态流转、localStorage 持久化
 */

import { ref } from 'vue'
import { wsSendInput } from '@/modules/mobile/composables/useMobileCommands'
import type { PresetTask, PresetTaskType, OnceTaskStatus } from './model'

const STORAGE_KEY = 'preset-tasks'

const tasks = ref<PresetTask[]>([])

/** 从 localStorage 读取任务列表 */
function loadFromStorage(): PresetTask[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return []
    return JSON.parse(raw) as PresetTask[]
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
export async function addTask(input: { title: string; content: string; type: PresetTaskType }) {
  const now = new Date().toISOString()
  const task: PresetTask = {
    id: crypto.randomUUID(),
    title: input.title,
    content: input.content,
    type: input.type,
    status: input.type === 'once' ? 'pending' : null,
    createdAt: now,
    updatedAt: now,
  }
  tasks.value.push(task)
  saveToStorage()
}

/** 更新预设任务（类型不可更改） */
export async function updateTask(task: PresetTask) {
  const index = tasks.value.findIndex(t => t.id === task.id)
  if (index === -1) return
  // 类型不可更改，保留原类型
  tasks.value[index] = {
    ...task,
    type: tasks.value[index].type,
    updatedAt: new Date().toISOString(),
  }
  saveToStorage()
}

/** 删除预设任务 */
export async function deleteTask(id: string) {
  tasks.value = tasks.value.filter(t => t.id !== id)
  saveToStorage()
}

/** 执行预设任务：更新状态 + 发送内容到终端 */
export async function executeTask(task: PresetTask, sessionId: string) {
  if (task.type === 'once') {
    // 一次性任务：pending → running
    const index = tasks.value.findIndex(t => t.id === task.id)
    if (index !== -1) {
      tasks.value[index].status = 'running'
      tasks.value[index].updatedAt = new Date().toISOString()
      saveToStorage()
    }

    try {
      await wsSendInput(sessionId, task.content)
      // 发送成功：running → completed
      if (index !== -1) {
        tasks.value[index].status = 'completed'
        tasks.value[index].updatedAt = new Date().toISOString()
        saveToStorage()
      }
    } catch {
      // 发送失败：running → failed
      if (index !== -1) {
        tasks.value[index].status = 'failed'
        tasks.value[index].updatedAt = new Date().toISOString()
        saveToStorage()
      }
      throw new Error('mobile.toolbox.sendFailed')
    }
  } else {
    // 模板任务：直接发送，不改变状态
    await wsSendInput(sessionId, task.content)
  }
}

/** 重置一次性任务状态为 pending */
export async function resetTaskStatus(id: string) {
  const index = tasks.value.findIndex(t => t.id === id)
  if (index !== -1 && tasks.value[index].type === 'once') {
    tasks.value[index].status = 'pending'
    tasks.value[index].updatedAt = new Date().toISOString()
    saveToStorage()
  }
}

export function usePresetTasks() {
  return {
    tasks,
    load,
    addTask,
    updateTask,
    deleteTask,
    executeTask,
    resetTaskStatus,
    saveToStorage,
  }
}
