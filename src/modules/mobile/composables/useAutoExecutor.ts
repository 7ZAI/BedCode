/**
 * Auto Executor - 自动执行任务引擎
 *
 * 管理手动/自动模式切换、任务队列和状态机驱动
 * 仅对当前会话生效，以 sessionId 隔离
 */
import { ref, computed, watch, type Ref } from 'vue'
import type { PresetTask, PresetTaskType } from './model'
import { useMobileConnection } from './useMobileConnection'
import { useTaskNotification } from './useTaskNotification'

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

  /** 切换模式 */
  function setMode(newMode: 'manual' | 'auto') {
    // 通过 /bedcode 命令切换，桌面端拦截后设置模式并广播 SessionModeChanged
    // 移动端通过 ws_sync_session_mode_changed 事件同步状态
    const cmd = newMode === 'auto' ? '/bedcode auto' : '/bedcode manual'
    sendInput(sessionId.value, cmd)
    sendInput(sessionId.value, '', 'enter')
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

  /** 授权类问题关键词 */
  const AUTH_KEYWORDS = ['allow', 'permit', 'approve', 'confirm', '授权', '允许', '同意']

  /** 判断问题是否为授权类 */
  function isAuthQuestion(header: string): boolean {
    const lower = header.toLowerCase()
    return AUTH_KEYWORDS.some(kw => lower.includes(kw))
  }

  /** 处理 asking 状态：自动回复问题 */
  function handleAsking(questions: Array<{ header: string; options: Array<{ label: string }> }>) {
    if (!questions.length || !currentTask.value) return

    for (const question of questions) {
      if (isAuthQuestion(question.header)) {
        // 授权类：选择同意/yes 选项
        const agreeOption = question.options.find(o =>
          ['yes', 'agree', 'allow', 'confirm', '是', '同意', '允许'].some(kw => o.label.toLowerCase().includes(kw))
        )
        const choice = agreeOption || question.options[0]
        // 发送选项文本 + Enter 确认提交
        sendInput(sessionId.value, choice.label)
        sendInput(sessionId.value, '', 'enter')
      } else {
        // 选择类：选第一个选项（Claude Code 推荐项）
        const choice = question.options[0]
        sendInput(sessionId.value, choice.label)
        sendInput(sessionId.value, '', 'enter')
      }
    }
  }

  /** 开始执行下一个 pending 任务 */
  function startNext() {
    if (isPaused.value) return

    const next = pendingTasks.value[0]
    if (!next) {
      currentTask.value = null
      saveState()
      return
    }

    currentTask.value = next
    next.status = 'running'
    retryCount.value = 0
    saveState()

    // 发送任务内容到终端 + Enter 提交执行
    sendInput(sessionId.value, next.content)
    sendInput(sessionId.value, '', 'enter')
  }

  /** 处理任务完成 */
  function handleTaskCompleted() {
    if (currentTask.value) {
      currentTask.value.status = 'completed'
    }
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
      // 发送继续执行 + Enter 提交
      sendInput(sessionId.value, '继续')
      sendInput(sessionId.value, '', 'enter')
    } else {
      currentTask.value.status = 'failed'
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
        if (currentTask.value) {
          currentTask.value.status = 'running'
          saveState()
        }
        if (questions) {
          handleAsking(questions)
        }
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
  }

  /** 清空指定会话的状态（会话停止时调用） */
  function cleanup() {
    executorStates.delete(sessionId.value)
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
