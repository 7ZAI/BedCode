/**
 * MobileHostApi mock（SDK getMobileApi() 返回的宿主通用连接/HTTP 能力）
 *
 * 与真实宿主一致：暴露连接态（activeSessionId / activeSessions / sessionConfigs /
 * isConnected）+ 通用 httpRequest 通道。任务队列等业务端点在 dev-shell 内按
 * 路径路由模拟（宿主能力 mock 的一部分，固定在 dev-shell 内实现）；演示种子
 * 数据由插件 devMock.queueSeed 提供（领域数据归插件）。
 */
import { computed, ref } from 'vue'
import type {
  MobileHostApi,
  MobileHttpRequestOptions,
  MobileHttpResult,
} from '../../../src/types'
import { activeSessionId, connected, sessions } from './session'
import { getAllDevMocks } from '../registry'

/** 活跃会话列表（响应式，MobileHostApi.activeSessions） */
const activeSessions = computed(() => sessions.value.map((s) => ({ ...s })))

/** 队列任务项（dev-shell mock 内部结构，演示数据来自插件 devMock.queueSeed） */
interface QueueTaskItem {
  id: string
  prompt: string
  position: number
  status: string
  created_at: string
}

const STORAGE_KEY = 'bedcode-dev-shell:queue-tasks'

function seedQueue(): QueueTaskItem[] {
  // 首个注册了队列种子的插件的领域数据（localStorage 无缓存时使用）
  for (const mock of getAllDevMocks()) {
    const seed = mock.queueSeed as QueueTaskItem[] | undefined
    if (seed?.length) return seed
  }
  return []
}

function loadQueue(): QueueTaskItem[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) return JSON.parse(raw)
  } catch {
    // localStorage 不可用则使用内存种子数据
  }
  return seedQueue()
}

const queueTasks = ref<QueueTaskItem[]>(loadQueue())
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

// ==================== 请求体解析（避免 any 字典，字段按需窄化） ====================

type JsonBody = Record<string, unknown>

function parseBody(body: unknown): JsonBody {
  if (!body) return {}
  if (typeof body === 'string') {
    try {
      return JSON.parse(body) as JsonBody
    } catch {
      return {}
    }
  }
  return body as JsonBody
}

function strField(body: JsonBody, key: string): string {
  const v = body[key]
  return typeof v === 'string' ? v : ''
}

function strArrayField(body: JsonBody, key: string): string[] {
  const v = body[key]
  return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : []
}

function boolField(body: JsonBody, key: string): boolean | undefined {
  const v = body[key]
  return typeof v === 'boolean' ? v : undefined
}

/** 通用 HTTP 请求路由：按对端 AutoTask 端点路径模拟（dev-shell 宿主能力 mock） */
async function routeHttpRequest(
  path: string,
  method: string,
  body: JsonBody,
): Promise<MobileHttpResult> {
  const [pathname, queryString] = path.split('?')
  const query = new URLSearchParams(queryString || '')
  const sessionId = query.get('session_id') || strField(body, 'session_id') || ''

  const BASE = '/api/plugin/com.bedcode.auto-task'

  if (pathname === `${BASE}/task-queue/list`) {
    if (!sessionId) return fail('missing sessionId')
    ensureQueueSeed()
    return ok({
      session_id: sessionId,
      tasks: queueTasks.value.map((t, i) => ({ ...t, position: i + 1 })),
      queue_count: queueTasks.value.length,
      active_task: null,
    })
  }

  if (pathname === `${BASE}/task-queue/add`) {
    const prompt = strField(body, 'prompt')
    if (!sessionId || !prompt) return fail('missing sessionId or prompt')
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
  }

  if (pathname === `${BASE}/task-queue/remove`) {
    const taskId = strField(body, 'task_id')
    if (!sessionId || !taskId) return fail('missing params')
    queueTasks.value = queueTasks.value.filter((t) => t.id !== taskId)
    saveQueue()
    return ok()
  }

  if (pathname === `${BASE}/task-queue/cancel`) {
    const taskId = strField(body, 'task_id')
    if (!sessionId || !taskId) return fail('missing params')
    queueTasks.value = queueTasks.value.filter((t) => t.id !== taskId)
    saveQueue()
    return ok()
  }

  if (pathname === `${BASE}/task-queue/clear`) {
    if (!sessionId) return fail('missing sessionId')
    queueTasks.value = []
    saveQueue()
    return ok()
  }

  if (pathname === `${BASE}/task-queue/update`) {
    const taskId = strField(body, 'task_id')
    if (!sessionId || !taskId) return fail('missing params')
    const task = queueTasks.value.find((t) => t.id === taskId)
    if (!task) return fail('task not found')
    task.prompt = strField(body, 'prompt')
    saveQueue()
    return ok()
  }

  if (pathname === `${BASE}/task-queue/reorder`) {
    if (!sessionId) return fail('missing sessionId')
    const byId = new Map(queueTasks.value.map((t) => [t.id, t]))
    const ids = strArrayField(body, 'task_ids')
    queueTasks.value = ids.map((id) => byId.get(id)).filter(Boolean) as QueueTaskItem[]
    saveQueue()
    return ok()
  }

  if (pathname === `${BASE}/session-settings`) {
    if (!sessionId) return fail('missing sessionId')
    return ok({
      session_id: sessionId,
      auto_execute: sessionMode.value.autoExecute,
      auto_answer: sessionMode.value.autoAnswer,
    })
  }

  if (pathname === `${BASE}/session-mode`) {
    if (!sessionId) return fail('missing sessionId')
    const autoExecute = boolField(body, 'auto_execute')
    const autoAnswer = boolField(body, 'auto_answer')
    if (autoExecute !== undefined) sessionMode.value.autoExecute = autoExecute
    if (autoAnswer !== undefined) sessionMode.value.autoAnswer = autoAnswer
    return ok()
  }

  if (pathname === `${BASE}/task-history/current`) {
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
  }

  if (pathname === `${BASE}/task-history/list`) {
    return ok({
      tasks: [],
      total: 0,
      limit: Number(query.get('limit') || 20),
      offset: Number(query.get('offset') || 0),
    })
  }

  if (pathname === `${BASE}/supported-agents`) {
    return ok({ agents: ['bedcode', 'claude-code', 'deepseek'] })
  }

  if (pathname === `${BASE}/scheduled-jobs/list`) {
    return ok({ jobs: [] })
  }

  if (pathname === `${BASE}/scheduled-jobs/create`) {
    return ok({ job_id: `dev-job-${Date.now()}` })
  }

  return fail(`dev-shell mock: unhandled endpoint ${method} ${path}`)
}

/** 暴露到 window.__BEDCODE_SHARED__.mobileApi */
export const mobileApi: MobileHostApi = {
  activeSessionId,
  activeSessions,
  sessionConfigs: computed(() =>
    sessions.value.map((s) => ({ session_id: s.id, agent: s.agent, status: s.status })),
  ),
  isConnected: connected,

  httpRequest<T = any>(path: string, options: MobileHttpRequestOptions = {}) {
    return routeHttpRequest(path, options.method || 'GET', parseBody(options.body)) as Promise<
      MobileHttpResult<T>
    >
  },
}

/** 供 MockTerminalView 展示队列 */
export { queueTasks, sessionMode }
