/**
 * AutoTask 对端桌面端 REST API 强类型封装（移动端插件领域代码）
 *
 * 宿主 shared runtime 只暴露通用能力（连接状态 + httpRequest 请求通道），
 * 本模块承载 AutoTask 业务端点（/api/plugin/com.bedcode.auto-task/...）与 DTO 形状，
 * 使插件业务代码内聚于插件工程、宿主与 SDK 不感知具体业务细节。
 */
import { getMobileApi, type MobileHostApi, type MobileHttpResult } from '@binblink/plugin-sdk-mobile'

/** 队列任务项（与桌面端 task-queue DTO 对齐） */
export interface QueueTaskItem {
  id: string
  prompt: string
  position: number
  status: string
  created_at: string
}

/** 任务历史条目（与桌面端 task_history 表字段一一对应） */
export interface TaskHistoryItem {
  id: string
  description: string | null
  status: string
  agent: string | null
  source: string | null
  session_id: string
  claude_sid: string | null
  working_dir: string | null
  auto_approve: number
  exit_reason: string | null
  created_at: string
  started_at: string | null
  completed_at: string | null
  input_tokens: number | null
  output_tokens: number | null
}

/** 定时任务条目（与桌面端 scheduled_jobs 表字段一一对应） */
export interface ScheduledJob {
  id: string
  name: string | null
  config_id: string
  trigger_at: string
  prompts: string
  status: string
  session_id: string | null
  created_at: string
  executed_at: string | null
  error: string | null
}

const AUTO_TASK_BASE = '/api/plugin/com.bedcode.auto-task'

function host(): MobileHostApi {
  return getMobileApi() as MobileHostApi
}

/** 查询任务队列 */
export function httpTaskQueueList(
  sessionId: string,
): Promise<MobileHttpResult<{
  session_id: string
  tasks: QueueTaskItem[]
  queue_count: number
  active_task: (QueueTaskItem & { source?: string }) | null
}>> {
  return host().httpRequest(`${AUTO_TASK_BASE}/task-queue/list?session_id=${encodeURIComponent(sessionId)}`)
}

/** 添加任务到队列 */
export function httpTaskQueueAdd(
  sessionId: string,
  prompt: string,
): Promise<MobileHttpResult<{ task_id: string; position: number }>> {
  return host().httpRequest(`${AUTO_TASK_BASE}/task-queue/add`, {
    method: 'POST',
    body: { session_id: sessionId, prompt },
  })
}

/** 从队列删除任务 */
export function httpTaskQueueRemove(
  sessionId: string,
  taskId: string,
): Promise<MobileHttpResult> {
  return host().httpRequest(`${AUTO_TASK_BASE}/task-queue/remove`, {
    method: 'DELETE',
    body: { session_id: sessionId, task_id: taskId },
  })
}

/** 取消活动队列项（waiting / executing） */
export function httpTaskQueueCancel(
  sessionId: string,
  taskId: string,
): Promise<MobileHttpResult> {
  return host().httpRequest(`${AUTO_TASK_BASE}/task-queue/cancel`, {
    method: 'POST',
    body: { session_id: sessionId, task_id: taskId },
  })
}

/** 清空任务队列 */
export function httpTaskQueueClear(sessionId: string): Promise<MobileHttpResult> {
  return host().httpRequest(`${AUTO_TASK_BASE}/task-queue/clear`, {
    method: 'POST',
    body: { session_id: sessionId },
  })
}

/** 更新队列任务内容 */
export function httpTaskQueueUpdate(
  sessionId: string,
  taskId: string,
  prompt: string,
): Promise<MobileHttpResult> {
  return host().httpRequest(`${AUTO_TASK_BASE}/task-queue/update`, {
    method: 'POST',
    body: { session_id: sessionId, task_id: taskId, prompt },
  })
}

/** 重排序任务队列 */
export function httpTaskQueueReorder(
  sessionId: string,
  taskIds: string[],
): Promise<MobileHttpResult> {
  return host().httpRequest(`${AUTO_TASK_BASE}/task-queue/reorder`, {
    method: 'POST',
    body: { session_id: sessionId, task_ids: taskIds },
  })
}

/** 查询会话设置（auto_execute / auto_answer） */
export function httpSessionSettings(
  sessionId: string,
): Promise<MobileHttpResult<{
  session_id: string
  auto_execute: boolean
  auto_answer: boolean
}>> {
  return host().httpRequest(
    `${AUTO_TASK_BASE}/session-settings?session_id=${encodeURIComponent(sessionId)}`,
  )
}

/** 设置会话自动模式 */
export function httpSetSessionMode(
  sessionId: string,
  autoExecute?: boolean,
  autoAnswer?: boolean,
): Promise<MobileHttpResult> {
  const body: { session_id: string; auto_execute?: boolean; auto_answer?: boolean } = { session_id: sessionId }
  if (autoExecute !== undefined) body.auto_execute = autoExecute
  if (autoAnswer !== undefined) body.auto_answer = autoAnswer
  return host().httpRequest(`${AUTO_TASK_BASE}/session-mode`, {
    method: 'POST',
    body,
  })
}

/** 查询会话当前任务 */
export function httpCurrentTask(
  sessionId: string,
): Promise<MobileHttpResult<{
  session_id: string
  task: {
    id: string
    description: string | null
    status: string
    auto_approve: number
    created_at: string
  } | null
}>> {
  return host().httpRequest(
    `${AUTO_TASK_BASE}/task-history/current?session_id=${encodeURIComponent(sessionId)}`,
  )
}

/** 查询 auto-task 支持的 agent 列表 */
export function httpListSupportedAgents(): Promise<MobileHttpResult<{ agents: string[] }>> {
  return host().httpRequest(`${AUTO_TASK_BASE}/supported-agents`)
}

/** 查询任务历史列表（分页 + 筛选，只拼接已提供的参数） */
export function httpTaskHistoryList(params?: {
  status?: string
  agent?: string
  source?: string
  since?: string
  until?: string
  limit?: number
  offset?: number
}): Promise<MobileHttpResult<{
  tasks: TaskHistoryItem[]
  total: number
  limit: number
  offset: number
}>> {
  const query = new URLSearchParams()
  if (params?.status) query.set('status', params.status)
  if (params?.agent) query.set('agent', params.agent)
  if (params?.source) query.set('source', params.source)
  if (params?.since) query.set('since', params.since)
  if (params?.until) query.set('until', params.until)
  if (params?.limit !== undefined) query.set('limit', String(params.limit))
  if (params?.offset !== undefined) query.set('offset', String(params.offset))
  const qs = query.toString()
  return host().httpRequest(`${AUTO_TASK_BASE}/task-history/list${qs ? `?${qs}` : ''}`)
}

/** 查询定时任务列表 */
export function httpScheduledJobsList(): Promise<MobileHttpResult<{
  jobs: ScheduledJob[]
}>> {
  return host().httpRequest(`${AUTO_TASK_BASE}/scheduled-jobs/list`)
}

/** 创建定时任务 */
export function httpScheduledJobCreate(body: {
  name?: string
  config_id: string
  trigger_at: string
  prompts: string[]
}): Promise<MobileHttpResult<{ job_id: string }>> {
  return host().httpRequest(`${AUTO_TASK_BASE}/scheduled-jobs/create`, {
    method: 'POST',
    body,
  })
}

/** AutoTask 业务 API 聚合（组件/composable 按需解构使用） */
export interface AutoTaskApi {
  httpTaskQueueList: typeof httpTaskQueueList
  httpTaskQueueAdd: typeof httpTaskQueueAdd
  httpTaskQueueRemove: typeof httpTaskQueueRemove
  httpTaskQueueCancel: typeof httpTaskQueueCancel
  httpTaskQueueClear: typeof httpTaskQueueClear
  httpTaskQueueUpdate: typeof httpTaskQueueUpdate
  httpTaskQueueReorder: typeof httpTaskQueueReorder
  httpSessionSettings: typeof httpSessionSettings
  httpSetSessionMode: typeof httpSetSessionMode
  httpCurrentTask: typeof httpCurrentTask
  httpListSupportedAgents: typeof httpListSupportedAgents
  httpTaskHistoryList: typeof httpTaskHistoryList
  httpScheduledJobsList: typeof httpScheduledJobsList
  httpScheduledJobCreate: typeof httpScheduledJobCreate
}

/** 获取 AutoTask 业务 API（单例，基于宿主通用 httpRequest 通道） */
export function getAutoTaskApi(): AutoTaskApi {
  return {
    httpTaskQueueList,
    httpTaskQueueAdd,
    httpTaskQueueRemove,
    httpTaskQueueCancel,
    httpTaskQueueClear,
    httpTaskQueueUpdate,
    httpTaskQueueReorder,
    httpSessionSettings,
    httpSetSessionMode,
    httpCurrentTask,
    httpListSupportedAgents,
    httpTaskHistoryList,
    httpScheduledJobsList,
    httpScheduledJobCreate,
  }
}
