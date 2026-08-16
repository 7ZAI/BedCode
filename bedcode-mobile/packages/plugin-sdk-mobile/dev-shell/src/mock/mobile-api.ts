/**
 * MobileHostApi mock（SDK getMobileApi() 返回的宿主连接/HTTP 能力）
 *
 * 任务队列数据保存在内存 ref（可在 MockTerminalView 中查看/重置），
 * HTTP 接口返回与真实宿主一致的 MobileHttpResult 形状。
 * 队列种子数据由插件 devMock.queueSeed 提供（领域数据归插件）。
 */
import { computed, ref } from 'vue'
import type {
  MobileHostApi,
  MobileHttpResult,
  MobileQueueTaskItem,
} from '../../src/types'
import { activeSessionId, connected, sessions } from './session'
import { getAllDevMocks } from '../registry'

/** 活跃会话列表（响应式，MobileHostApi.activeSessions） */
const activeSessions = computed(() => sessions.value.map((s) => ({ ...s })))

const STORAGE_KEY = 'bedcode-dev-shell:queue-tasks'

function seedQueue(): MobileQueueTaskItem[] {
  // 首个注册了队列种子的插件的领域数据（localStorage 无缓存时使用）
  for (const mock of getAllDevMocks()) {
    if (mock.queueSeed?.length) return mock.queueSeed
  }
  return []
}

function loadQueue(): MobileQueueTaskItem[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) return JSON.parse(raw)
  } catch {
    // localStorage 不可用则使用内存种子数据
  }
  return seedQueue()
}

const queueTasks = ref<MobileQueueTaskItem[]>(loadQueue())
let queueSeeded = queueTasks.value.length > 0

/** 插件 devMock 注册完成后调用：无持久化缓存时注入队列种子（惰性，避免模块加载时序问题） */
export function syncQueueSeed(): void {
  if (queueSeeded) return
  queueSeeded = true
  const seed = seedQueue()
  if (seed.length) {
    queueTasks.value = seed
    saveQueue()
  }
}

/** 任务队列接口访问前确保种子已注入 */
function ensureQueueSeed(): void {
  if (!queueSeeded) syncQueueSeed()
}

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
    ensureQueueSeed()
    return ok({
      session_id: sessionId,
      tasks: queueTasks.value.map((t, i) => ({ ...t, position: i + 1 })),
      queue_count: queueTasks.value.length,
      // dev-shell 无真实执行：无处理中队列项
      active_task: null,
    })
  },

  async httpTaskQueueAdd(sessionId, prompt) {
    if (!sessionId || !prompt) return fail('missing sessionId or prompt')
    // 与真实宿主一致：响应携带 task_id（面板 handleAddFromPreset 依赖它标记预设）
    const id = `dev-queue-${Date.now()}`
    queueTasks.value.push({
      id,
      prompt,
      position: queueTasks.value.length + 1,
      status: 'pending',
      created_at: new Date().toISOString(),
    })
    saveQueue()
    return ok({ task_id: id, position: queueTasks.value.length })
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
