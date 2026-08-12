/**
 * 预设任务 mock（宿主 usePresetTasks 的浏览器实现）
 *
 * tasks 持久化到 localStorage；sendTask/executeTask 需要与对端桌面端通信，
 * 浏览器中不可用，调用时记日志提示。API 形状与宿主 composable 保持一致（含执行状态字段）。
 */
import { ref } from 'vue'
import { pushLog } from '../registry'

const STORAGE_KEY = 'bedcode-dev-shell:preset-tasks'

export type PresetTaskStatus = 'unused' | 'executing' | 'completed' | 'interrupted'

export interface PresetTaskItem {
  id: string
  prompt: string
  status: PresetTaskStatus
  createdAt: string
  repeatable: boolean
  pendingTaskId: string | null
  pendingSessionId: string | null
}

const tasks = ref<PresetTaskItem[]>(load())

function load(): PresetTaskItem[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    const parsed = raw ? JSON.parse(raw) : []
    return (parsed as Partial<PresetTaskItem>[]).map(t => ({
      id: t.id ?? '',
      prompt: t.prompt ?? '',
      status: t.status ?? 'unused',
      createdAt: t.createdAt ?? '',
      repeatable: t.repeatable ?? true,
      pendingTaskId: t.pendingTaskId ?? null,
      pendingSessionId: t.pendingSessionId ?? null,
    }))
  } catch {
    return []
  }
}

function saveToStorage(): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(tasks.value))
  } catch {
    // localStorage 不可用（隐私模式）时静默降级
  }
}

async function loadRemote(): Promise<void> {
  // 宿主实现从桌面端拉取；浏览器 mock 直接读本地
  tasks.value = load()
}

async function addTask(input: { content: string; repeatable?: boolean }): Promise<void> {
  tasks.value.unshift({
    id: `dev-task-${Date.now()}`,
    prompt: input.content,
    status: 'unused',
    createdAt: new Date().toISOString(),
    repeatable: input.repeatable ?? true,
    pendingTaskId: null,
  })
  saveToStorage()
}

async function updateTask(id: string, prompt: string): Promise<void> {
  const task = tasks.value.find((t) => t.id === id)
  if (task) {
    const contentChanged = task.prompt !== prompt
    task.prompt = prompt
    // 内容变化视为重新定义任务，执行状态重置
    if (contentChanged) {
      task.status = 'unused'
      task.pendingTaskId = null
    }
    saveToStorage()
  }
}

async function deleteTask(id: string): Promise<void> {
  tasks.value = tasks.value.filter((t) => t.id !== id)
  saveToStorage()
}

async function sendTask(id: string): Promise<void> {
  pushLog('warn', 'preset-tasks', `sendTask(${id}) 需要连接对端桌面端，浏览器 dev-shell 不可用`)
}

async function executeTask(id: string): Promise<void> {
  pushLog('warn', 'preset-tasks', `executeTask(${id}) 需要连接对端桌面端，浏览器 dev-shell 不可用`)
  const task = tasks.value.find((t) => t.id === id)
  if (task) {
    task.status = 'completed'
    task.pendingTaskId = null
    task.pendingSessionId = null
    saveToStorage()
  }
}

/** 入队成功：记录队列项 id 与会话，状态 → executing */
async function markEnqueued(id: string, taskId: string, sessionId: string): Promise<void> {
  const task = tasks.value.find((t) => t.id === id)
  if (!task) return
  task.status = 'executing'
  task.pendingTaskId = taskId
  task.pendingSessionId = sessionId
  saveToStorage()
}

/** 完成广播：按队列项 id 匹配预设，状态 → completed（不匹配忽略） */
async function markCompletedByTaskId(taskId: string): Promise<void> {
  const task = tasks.value.find((t) => t.pendingTaskId === taskId)
  if (!task) return
  task.status = 'completed'
  task.pendingTaskId = null
  task.pendingSessionId = null
  saveToStorage()
}

/** 对账：单个预设落中断 */
async function markInterrupted(id: string): Promise<void> {
  const task = tasks.value.find((t) => t.id === id)
  if (!task || task.status !== 'executing') return
  task.status = 'interrupted'
  saveToStorage()
}

/** 按队列项 id 落中断（取消/会话终止广播，与宿主 usePresetTasks 对齐） */
async function markInterruptedByTaskId(taskId: string): Promise<void> {
  const task = tasks.value.find((t) => t.pendingTaskId === taskId)
  if (!task || task.status !== 'executing') return
  task.status = 'interrupted'
  task.pendingTaskId = null
  task.pendingSessionId = null
  saveToStorage()
}

/** 队列项移除：按队列项 id 回退未使用（不匹配忽略） */
async function revertToUnusedByTaskId(taskId: string): Promise<void> {
  const task = tasks.value.find((t) => t.pendingTaskId === taskId)
  if (!task) return
  task.status = 'unused'
  task.pendingTaskId = null
  task.pendingSessionId = null
  saveToStorage()
}

function clearAllTasks(): void {
  tasks.value = []
  saveToStorage()
}

/** 锁定判定：不可重复 + 已执行 → 锁定；可重复 + 执行中 → 防重复。
 * 必须同步返回（模板禁用直接依赖布尔值，async 会恒为真导致禁用失效） */
function canEnqueueTask(task: PresetTaskItem): boolean {
  if (task.status === 'executing') return false
  if (!task.repeatable && task.status !== 'unused') return false
  return true
}

export function usePresetTasks() {
  return {
    tasks,
    load: loadRemote,
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
    canEnqueue: canEnqueueTask,
  }
}

/** 暴露到 window.__BEDCODE_SHARED__.presetTasks（SDK getPresetTasks() 约定） */
export const presetTasksApi = { usePresetTasks }
