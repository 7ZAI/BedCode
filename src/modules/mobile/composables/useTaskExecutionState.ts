/**
 * Task Execution State - 任务执行状态管理
 *
 * 按 sessionId 维护任务执行状态：模式、当前任务 ID、已完成一次性任务集合
 * 与 useAutoExecutor 集成，在关键节点同步状态
 * 当自动执行完成一次性任务时，同步更新 PresetTask 状态
 */
import { ref, watch, type Ref } from 'vue'
import { usePresetTasks } from './usePresetTasks'

/** 会话级任务执行状态 */
export interface TaskExecutionState {
  mode: 'manual' | 'auto'
  currentTaskId: string | null
  completedOnceTaskIds: string[]
}

// 按 sessionId 隔离的状态存储
const executionStates = new Map<string, TaskExecutionState>()

function createState(): TaskExecutionState {
  return {
    mode: 'manual',
    currentTaskId: null,
    completedOnceTaskIds: [],
  }
}

export function useTaskExecutionState(sessionId: Ref<string>) {
  const mode = ref<'manual' | 'auto'>('manual')
  const currentTaskId = ref<string | null>(null)
  const completedOnceTaskIds = ref<Set<string>>(new Set())

  /** 加载指定会话的状态 */
  function loadState(sid: string) {
    let state = executionStates.get(sid)
    if (!state) {
      state = createState()
      executionStates.set(sid, state)
    }
    mode.value = state.mode
    currentTaskId.value = state.currentTaskId
    completedOnceTaskIds.value = new Set(state.completedOnceTaskIds)
  }

  /** 持久化当前状态到 Map */
  function saveState() {
    executionStates.set(sessionId.value, {
      mode: mode.value,
      currentTaskId: currentTaskId.value,
      completedOnceTaskIds: [...completedOnceTaskIds.value],
    })
  }

  /** 设置当前执行任务 ID */
  function setCurrentTask(taskId: string | null) {
    currentTaskId.value = taskId
    saveState()
  }

  /** 同步模式变更 */
  function setMode(newMode: 'manual' | 'auto') {
    mode.value = newMode
    // 切换到手动模式时清除当前任务 ID
    if (newMode === 'manual') {
      currentTaskId.value = null
    }
    saveState()
  }

  /** 标记一次性任务完成，同步更新 PresetTask 状态 */
  function markOnceTaskCompleted(taskId: string) {
    completedOnceTaskIds.value.add(taskId)
    saveState()

    // 同步更新 PresetTask 状态
    const { tasks, saveToStorage } = usePresetTasks()
    const task = tasks.value.find(t => t.id === taskId)
    if (task && task.type === 'once' && task.status !== 'completed') {
      task.status = 'completed'
      task.updatedAt = new Date().toISOString()
      saveToStorage()
    }
  }

  /** 查询一次性任务是否已完成 */
  function isOnceTaskCompleted(taskId: string): boolean {
    return completedOnceTaskIds.value.has(taskId)
  }

  /** 处理桌面端推送的任务状态变更事件
   *  自动模式下：有 currentTaskId 时同步 PresetTask 状态
   *  手动模式下：currentTaskId 为 null，不更新 PresetTask
   */
  function handleTaskStatusChanged(status: string): { taskCompleted: boolean; taskId: string | null } {
    const tid = currentTaskId.value

    if (status === 'completed' && tid) {
      // 查找对应 PresetTask 的类型
      const { tasks } = usePresetTasks()
      const task = tasks.value.find(t => t.id === tid)
      if (task?.type === 'once') {
        markOnceTaskCompleted(tid)
      }
      setCurrentTask(null)
      return { taskCompleted: true, taskId: tid }
    }

    if (status === 'interrupted' && tid) {
      // 中断不标记为完成，只清除当前任务 ID
      setCurrentTask(null)
      return { taskCompleted: false, taskId: tid }
    }

    return { taskCompleted: false, taskId: null }
  }

  /** 清空指定会话的状态（会话停止时调用） */
  function cleanup() {
    executionStates.delete(sessionId.value)
  }

  // 初始化：加载当前会话状态
  loadState(sessionId.value)

  // 监听 sessionId 变化（会话切换时加载对应状态）
  watch(sessionId, (newSid) => {
    loadState(newSid)
  })

  return {
    mode,
    currentTaskId,
    completedOnceTaskIds,
    setCurrentTask,
    setMode,
    markOnceTaskCompleted,
    isOnceTaskCompleted,
    handleTaskStatusChanged,
    cleanup,
  }
}
