/**
 * Auto Executor - 自动执行任务引擎
 *
 * 管理手动/自动模式切换、任务队列和状态机驱动
 * 仅对当前会话生效，以 sessionId 隔离
 */
import { ref, computed, watch, type Ref } from 'vue'
import type { PresetTask, PresetTaskType } from './model'
import { useMobileConnection } from './useMobileConnection'
import { useHttpApi } from './useHttpApi'
import { useTaskNotification } from './useTaskNotification'
import { useTaskExecutionState } from './useTaskExecutionState'

/** 队列中的任务 */
export interface QueuedTask {
  id: string
  title: string
  content: string
  type: PresetTaskType
  status: 'pending' | 'running' | 'completed' | 'failed' | 'retrying'
}

/** 自动执行器状态 */
export interface AutoExecutorState {
  mode: 'manual' | 'auto'
  queue: QueuedTask[]
  currentTask: QueuedTask | null
  retryCount: number
  isPaused: boolean
}

// 按 sessionId 隔离的状态存储
const executorStates = new Map<string, AutoExecutorState>()

function createState(): AutoExecutorState {
  return {
    mode: 'manual',
    queue: [],
    currentTask: null,
    retryCount: 0,
    isPaused: false,
  }
}

export function useAutoExecutor(sessionId: Ref<string>) {
  const mode = ref<'manual' | 'auto'>('manual')
  const queue = ref<QueuedTask[]>([])
  const currentTask = ref<QueuedTask | null>(null)
  const retryCount = ref(0)
  const isPaused = ref(false)

  const pendingTasks = computed(() => queue.value.filter(t => t.status === 'pending'))
  const hasQueuedTasks = computed(() => pendingTasks.value.length > 0)

  const {
    setCurrentTask: setExecutionCurrentTask,
    setMode: setExecutionMode,
    handleTaskStatusChanged: handleExecutionTaskStatusChanged,
    cleanup: executionStateCleanup,
  } = useTaskExecutionState(sessionId)

  /** 加载指定会话的状态 */
  function loadState(sid: string) {
    let state = executorStates.get(sid)
    if (!state) {
      state = createState()
      executorStates.set(sid, state)
    }
    mode.value = state.mode
    queue.value = state.queue
    currentTask.value = state.currentTask
    retryCount.value = state.retryCount
    isPaused.value = state.isPaused
    // 同步加载的模式到通知系统
    const { setSessionMode } = useTaskNotification()
    setSessionMode(sid, state.mode)
  }

  /** 持久化当前状态到 Map */
  function saveState() {
    const sid = sessionId.value
    executorStates.set(sid, {
      mode: mode.value,
      queue: queue.value,
      currentTask: currentTask.value,
      retryCount: retryCount.value,
      isPaused: isPaused.value,
    })
  }

  /** 切换模式：乐观更新本地状态，同时通过 HTTP API 通知桌面端
   *  本地先更新 mode.value，桌面端确认后广播 SessionModeChanged 再次同步
   *  避免因网络延迟或事件丢失导致 UI 不响应切换操作
   */
  async function setMode(newMode: 'manual' | 'auto') {
    // 乐观更新：立即反映到 UI
    mode.value = newMode
    saveState()
    // 同步模式到通知系统
    const { setSessionMode } = useTaskNotification()
    setSessionMode(sessionId.value, newMode)
    // 同步模式到任务执行状态
    setExecutionMode(newMode)

    // 通知桌面端更新内存状态，桌面端会广播 SessionModeChanged 事件
    const { httpSetSessionMode } = useHttpApi()
    const autoApprove = newMode === 'auto'
    try {
      await httpSetSessionMode(sessionId.value, autoApprove)
    } catch {
      // API 失败时回滚到原模式
      const rollbackMode = newMode === 'auto' ? 'manual' : 'auto'
      mode.value = rollbackMode
      saveState()
      setSessionMode(sessionId.value, rollbackMode)
      setExecutionMode(rollbackMode)
    }
  }

  /** 添加任务到队列 */
  function addToQueue(tasks: PresetTask[]) {
    const queued: QueuedTask[] = tasks.map(t => ({
      id: t.id,
      title: t.title,
      content: t.content,
      type: t.type,
      status: 'pending' as const,
    }))
    queue.value.push(...queued)
    saveState()
  }

  /** 从队列移除任务 */
  function removeFromQueue(taskId: string) {
    queue.value = queue.value.filter(t => t.id !== taskId)
    saveState()
  }

  /** 清空队列 */
  function clearQueue() {
    queue.value = []
    currentTask.value = null
    retryCount.value = 0
    saveState()
  }

  /** 暂停自动执行 */
  function pause() {
    isPaused.value = true
    saveState()
  }

  /** 恢复自动执行 */
  function resume() {
    isPaused.value = false
    saveState()
  }

  // ==================== 自动执行引擎 ====================

  const { sendInput } = useMobileConnection()

  /** 处理 asking 状态：仅更新 UI 状态，不通过 sendInput 回答
   *  自动模式下 Python PreToolUse hook 已通过 permissionDecision: "allow" 自动回答
   *  手动模式下用户在 Claude Code 原生界面操作
   *  移动端只需感知 asking 状态更新 UI 即可
   */
  function handleAsking(questions?: Array<{ header: string; options: Array<{ label: string }> }>) {
    if (currentTask.value) {
      currentTask.value.status = 'running'
      saveState()
    }
    // questions 数据可用于 UI 展示（如显示问题内容），但不通过 sendInput 回答
    // 自动模式由 Python hook 处理，手动模式由用户在 Claude Code 中操作
    void questions
  }

  /** 开始执行下一个 pending 任务 */
  function startNext() {
    if (isPaused.value) return

    const next = pendingTasks.value[0]
    if (!next) {
      currentTask.value = null
      setExecutionCurrentTask(null)
      saveState()
      return
    }

    currentTask.value = next
    next.status = 'running'
    retryCount.value = 0
    saveState()
    // 同步当前任务 ID 到执行状态
    setExecutionCurrentTask(next.id)

    // 发送任务内容到终端 + Enter 提交执行
    sendInput(sessionId.value, next.content)
    sendInput(sessionId.value, '', 'enter')
  }

  /** 处理任务完成 */
  function handleTaskCompleted() {
    if (currentTask.value) {
      currentTask.value.status = 'completed'
    }
    // 通过执行状态管理器同步 PresetTask 状态
    handleExecutionTaskStatusChanged('completed')
    saveState()

    // 执行 /clear + Enter 清空上下文，等待 Claude Code 回到 idle 后自动开始下一个任务
    // 下一次 handleTaskStatusChanged('idle') 会触发 startNext()
    sendInput(sessionId.value, '/clear')
    sendInput(sessionId.value, '', 'enter')
  }

  /** 处理任务中断 */
  function handleInterrupted() {
    if (!currentTask.value) return

    if (retryCount.value < 3) {
      retryCount.value++
      currentTask.value.status = 'retrying'
      saveState()
      // 发送"继续"利用 Claude Code 上下文机制从中断点恢复
      sendInput(sessionId.value, '继续')
      sendInput(sessionId.value, '', 'enter')
    } else {
      currentTask.value.status = 'failed'
      // 超过重试次数，清除当前任务 ID
      setExecutionCurrentTask(null)
      saveState()
      // 超过重试次数，执行下一个任务
      startNext()
    }
  }

  /** 监听桌面端推送的任务状态变更事件 */
  function handleTaskStatusChanged(status: string, questions?: Array<{ header: string; options: Array<{ label: string }> }>) {
    // 非自动模式或已暂停，不处理
    if (mode.value !== 'auto' || isPaused.value) return

    switch (status) {
      case 'idle':
        // 无任务运行中，如果有待执行任务则开始
        if (!currentTask.value || currentTask.value.status === 'completed' || currentTask.value.status === 'failed') {
          startNext()
        }
        break
      case 'in_progress':
        if (currentTask.value) {
          currentTask.value.status = 'running'
          saveState()
        }
        break
      case 'asking':
        handleAsking(questions)
        break
      case 'completed':
        handleTaskCompleted()
        break
      case 'interrupted':
        handleInterrupted()
        break
    }
  }

  /** 处理桌面端推送的会话模式变更事件 */
  function handleSessionModeChanged(autoApprove: boolean) {
    const newMode = autoApprove ? 'auto' as const : 'manual' as const
    mode.value = newMode
    saveState()
    // 同步模式到通知系统
    const { setSessionMode } = useTaskNotification()
    setSessionMode(sessionId.value, newMode)
    // 同步模式到任务执行状态
    setExecutionMode(newMode)
  }

  /** 清空指定会话的状态（会话停止时调用） */
  function cleanup() {
    executorStates.delete(sessionId.value)
    executionStateCleanup()
    clearQueue()
  }

  // 初始化：加载当前会话状态
  loadState(sessionId.value)

  // 监听 sessionId 变化（会话切换时加载对应状态）
  watch(sessionId, (newSid) => {
    loadState(newSid)
  })

  return {
    mode,
    queue,
    currentTask,
    retryCount,
    isPaused,
    pendingTasks,
    hasQueuedTasks,
    setMode,
    addToQueue,
    removeFromQueue,
    clearQueue,
    pause,
    resume,
    startNext,
    handleTaskStatusChanged,
    handleSessionModeChanged,
    cleanup,
  }
}
