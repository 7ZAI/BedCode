/**
 * MobileHostApi mock（SDK getMobileApi() 返回的宿主连接/HTTP 能力）
 *
 * 任务队列数据保存在内存 ref（可在 MockTerminalView 中查看/重置），
 * HTTP 接口返回与真实宿主一致的 MobileHttpResult 形状。
 */
import { computed, ref } from 'vue'
import type {
  MobileHostApi,
  MobileHttpResult,
  MobileQueueTaskItem,
} from '../../src/types'
import { activeSessionId, connected, sessions } from './session'

/** 活跃会话列表（响应式，MobileHostApi.activeSessions） */
const activeSessions = computed(() => sessions.value.map((s) => ({ ...s })))

const STORAGE_KEY = 'bedcode-dev-shell:queue-tasks'

function loadQueue(): MobileQueueTaskItem[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) return JSON.parse(raw)
  } catch {
    // localStorage 不可用则使用内存种子数据
  }
  return [
    {
      id: 'dev-queue-1',
      prompt: '查看当前目录文件列表',
      position: 1,
      status: 'pending',
      created_at: new Date().toISOString(),
    },
    {
      id: 'dev-queue-2',
      prompt: '输出系统信息',
      position: 2,
      status: 'pending',
      created_at: new Date().toISOString(),
    },
  ]
}

const queueTasks = ref<MobileQueueTaskItem[]>(loadQueue())
const sessionMode = ref({ autoExecute: false, autoAnswer: false })

function saveQueue(): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(queueTasks.value))
  } catch {
    // 隐私模式等场景静默降级
  }
}

function ok<T>(data?: T): MobileHttpResult<T> {
  return { code: 0, message: 'ok', data }
}

function fail(message: string): MobileHttpResult {
  return { code: 1, message }
}

/** 暴露到 window.__BEDCODE_SHARED__.mobileApi */
export const mobileApi: MobileHostApi = {
  activeSessionId,
  activeSessions,
  sessionConfigs: computed(() =>
    sessions.value.map((s) => ({ session_id: s.id, agent: s.agent, status: s.status })),
  ),
  isConnected: connected,

  async httpTaskQueueList(sessionId) {
    if (!sessionId) return fail('missing sessionId')
    return ok({
      session_id: sessionId,
      tasks: queueTasks.value.map((t, i) => ({ ...t, position: i + 1 })),
      queue_count: queueTasks.value.length,
    })
  },

  async httpTaskQueueAdd(sessionId, prompt) {
    if (!sessionId || !prompt) return fail('missing sessionId or prompt')
    queueTasks.value.push({
      id: `dev-queue-${Date.now()}`,
      prompt,
      position: queueTasks.value.length + 1,
      status: 'pending',
      created_at: new Date().toISOString(),
    })
    saveQueue()
    return ok()
  },

  async httpTaskQueueRemove(sessionId, taskId) {
    if (!sessionId || !taskId) return fail('missing params')
    queueTasks.value = queueTasks.value.filter((t) => t.id !== taskId)
    saveQueue()
    return ok()
  },

  async httpTaskQueueClear(sessionId) {
    if (!sessionId) return fail('missing sessionId')
    queueTasks.value = []
    saveQueue()
    return ok()
  },

  async httpTaskQueueUpdate(sessionId, taskId, prompt) {
    if (!sessionId || !taskId) return fail('missing params')
    const task = queueTasks.value.find((t) => t.id === taskId)
    if (!task) return fail('task not found')
    task.prompt = prompt
    saveQueue()
    return ok()
  },

  async httpTaskQueueReorder(sessionId, taskIds) {
    if (!sessionId) return fail('missing sessionId')
    const byId = new Map(queueTasks.value.map((t) => [t.id, t]))
    queueTasks.value = taskIds.map((id) => byId.get(id)).filter(Boolean) as MobileQueueTaskItem[]
    saveQueue()
    return ok()
  },

  async httpSessionSettings(sessionId) {
    if (!sessionId) return fail('missing sessionId')
    return ok({
      session_id: sessionId,
      auto_execute: sessionMode.value.autoExecute,
      auto_answer: sessionMode.value.autoAnswer,
    })
  },

  async httpSetSessionMode(sessionId, autoExecute, autoAnswer) {
    if (!sessionId) return fail('missing sessionId')
    if (autoExecute !== undefined) sessionMode.value.autoExecute = autoExecute
    if (autoAnswer !== undefined) sessionMode.value.autoAnswer = autoAnswer
    return ok()
  },

  async httpCurrentTask(sessionId) {
    if (!sessionId) return fail('missing sessionId')
    const first = queueTasks.value.find((t) => t.status !== 'done')
    return ok({
      session_id: sessionId,
      task: first
        ? {
            id: first.id,
            description: first.prompt,
            status: first.status,
            auto_approve: sessionMode.value.autoExecute ? 1 : 0,
            created_at: first.created_at,
          }
        : null,
    })
  },

  async httpListSupportedAgents() {
    return ok({ agents: ['bedcode', 'claude-code', 'deepseek'] })
  },
}

/** 供 MockTerminalView 展示队列 */
export { queueTasks, sessionMode }
