/**
 * 预设任务 mock（宿主 usePresetTasks 的浏览器实现）
 *
 * tasks 持久化到 localStorage；sendTask/executeTask 需要与对端桌面端通信，
 * 浏览器中不可用，调用时记日志提示。API 形状与宿主 composable 保持一致。
 */
import { ref } from 'vue'
import { pushLog } from '../registry'

const STORAGE_KEY = 'bedcode-dev-shell:preset-tasks'

export interface PresetTaskItem {
  id: string
  prompt: string
  status: string
  createdAt: string
}

const tasks = ref<PresetTaskItem[]>(load())

function load(): PresetTaskItem[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    return raw ? JSON.parse(raw) : []
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

async function addTask(prompt: string): Promise<void> {
  tasks.value.unshift({
    id: `dev-task-${Date.now()}`,
    prompt,
    status: 'pending',
    createdAt: new Date().toISOString(),
  })
  saveToStorage()
}

async function updateTask(id: string, prompt: string): Promise<void> {
  const task = tasks.value.find((t) => t.id === id)
  if (task) {
    task.prompt = prompt
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
}

function clearAllTasks(): void {
  tasks.value = []
  saveToStorage()
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
  }
}

/** 暴露到 window.__BEDCODE_SHARED__.presetTasks（SDK getPresetTasks() 约定） */
export const presetTasksApi = { usePresetTasks }
