/**
 * HTTP API Client Composable
 *
 * 移动端直接调用桌面端 HTTP REST API（ticket 03/07 收束：HTTP 全部经 Rust 统一代理）
 * - 所有请求走 `invoke('http_request')`，request_id 多路复用（D1/D3）
 * - JWT 注入 / 链路加密信封 / 超时 / 取消 / 日志全部在 Rust 端（前端只渲染）
 * - Egress：desktop 类请求经 L1 桌面端目标放行（setApiBaseUrl/httpProbe 时声明目标）
 */

import { ref } from 'vue'
import { v4 as uuidv4 } from 'uuid'
import { invoke } from '@tauri-apps/api/core'
import { logger } from '@/utils/frontendLogger'

// ==================== Config ====================

const API_BASE_URL = ref<string>('')

/** http_request 响应形状（Rust 侧 HttpProxyResponse） */
interface HttpProxyResponse {
  status: number
  statusText: string
  headers: Record<string, string>
  bodyText: string
}

// ==================== Core HTTP Client ====================

/** Git diff 行数据 */
export interface FileDiffLine {
  type: 'context' | 'added' | 'removed'
  content: string
  oldLineNo: number | null
  newLineNo: number | null
}

export interface ApiResult<T = any> {
  code: number
  message: string
  data?: T
}

/** 正统渲染端身份（与桌面端 ResizeOutcome serde 形状对齐） */
export type RendererSource = { kind: 'desktop' } | { kind: 'mobile'; deviceName: string }

/** resize 裁决结果 */
export type ResizeOutcome =
  | { status: 'applied'; canonical: RendererSource }
  | { status: 'needsConfirmation'; currentCanonical: RendererSource }

async function request<T = any>(
  path: string,
  options: RequestInit = {}
): Promise<ApiResult<T>> {
  const baseUrl = API_BASE_URL.value

  if (!baseUrl) {
    logger.error('[HttpApi] No base URL set, cannot make request to', path)
    return { code: -1, message: 'Not connected: no base URL set' }
  }

  const url = `http://${baseUrl}${path}`
  const requestId = uuidv4()

  // body 归一化：字符串原样，对象 JSON.stringify（与 httpRequest 通道一致）
  const body =
    typeof options.body === 'string'
      ? options.body
      : options.body !== undefined && options.body !== null
        ? JSON.stringify(options.body)
        : null

  try {
    logger.log('[HttpApi] Request:', options.method || 'GET', url)
    const resp = await invoke<HttpProxyResponse>('http_request', {
      requestId,
      method: options.method || 'GET',
      url,
      headers: (options.headers as Record<string, string>) || {},
      body,
      timeoutMs: 30000,
      kind: 'desktop',
    })

    // HTTP 非 2xx：返回状态码（code=status，与迁移前语义一致）
    if (resp.status < 200 || resp.status >= 300) {
      const text = resp.bodyText || ''
      logger.error('[HttpApi] HTTP error:', resp.status, resp.statusText, text)
      return { code: resp.status, message: `HTTP ${resp.status}: ${resp.statusText}` }
    }

    // JWT / 加密信封 / pin 刷新均由 Rust 代理完成（bodyText 已解密）
    const result = JSON.parse(resp.bodyText)
    logger.log('[HttpApi] Response OK:', path, 'code=', result.code)
    return result
  } catch (e: any) {
    // invoke 拒绝 = Egress 拒绝（EXTERNAL_URL_*）/ 加密失败（LINK_ENCRYPTION_*）/
    // 取消（REQUEST_CANCELED）/ 网络错误——message 携带 Rust 侧错误码
    logger.error('[HttpApi] Fetch failed:', path, e?.message || e)
    return { code: -1, message: e?.message || String(e) }
  }
}

// ==================== 通用请求通道（插件 shared runtime mobileApi 用） ====================
/**
 * 通用对端 REST 请求选项（插件经 mobileApi.httpRequest 访问；body 支持对象或字符串） */
export interface MobileHttpRequestOptions {
  method?: string
  body?: unknown
  headers?: Record<string, string>
}

/**
 * 外部 URL 请求（Egress external 类：L1/L2/L3 全层判定，未声明 → 弹窗）
 *
 * 供 useUpdateChecker（GitHub API 经宿主内置 L2 声明放行）等外网调用面使用；
 * 返回 Rust `HttpProxyResponse` 形状（调用方自行解析 bodyText）。
 */
export async function externalRequest(
  url: string,
  options: MobileHttpRequestOptions = {},
): Promise<HttpProxyResponse> {
  const requestId = uuidv4()
  return invoke<HttpProxyResponse>('http_request', {
    requestId,
    method: options.method || 'GET',
    url,
    headers: options.headers || {},
    body: null,
    timeoutMs: 30000,
    kind: 'external',
  })
}

/**
 * 通用 HTTP 请求：与内部 request 一致（JWT 注入 / 链路加密 / 错误归一化），
 * body 为对象时自动 JSON.stringify（与既有业务包装函数行为对齐）。
 * 具体插件业务端点由各插件工程基于本通道自行封装，宿主不感知插件领域。
 */
export function httpRequest<T = any>(
  path: string,
  options: MobileHttpRequestOptions = {},
): Promise<ApiResult<T>> {
  const body =
    typeof options.body === 'string'
      ? options.body
      : options.body !== undefined
        ? JSON.stringify(options.body)
        : undefined
  return request<T>(path, { ...options, body } as RequestInit)
}

// ==================== Auth API ====================

export async function httpRequestPairing(data: {
  deviceId: string
  deviceName: string
  fingerprint: string
}) {
  return request<{ pairingCode: string; expiresIn: number }>(
    '/api/auth/pairing',
    { method: 'POST', body: JSON.stringify(data) }
  )
}

export async function httpVerifyPairingCode(data: {
  deviceId: string
  deviceName: string
  fingerprint: string
  pairingCode: string
}) {
  return request<{ token: string; expiresIn: number }>(
    '/api/auth/verify',
    { method: 'POST', body: JSON.stringify(data) }
  )
}

export async function httpQrConnect(data: {
  deviceId: string
  deviceName: string
  fingerprint: string
  qrToken: string
}) {
  return request<{ token: string; expiresIn: number }>(
    '/api/auth/qr-connect',
    { method: 'POST', body: JSON.stringify(data) }
  )
}

export async function httpReauth(data: {
  deviceId: string
  fingerprint: string
  sessionToken: string
}) {
  return request<{ token: string; expiresIn: number }>(
    '/api/auth/reauth',
    { method: 'POST', body: JSON.stringify(data) }
  )
}

// ==================== Session API ====================

export async function httpListSessions() {
  return request<{ sessions: any[] }>('/api/sessions')
}

export async function httpStartSession(
  configId: string,
  size?: { cols: number; rows: number }
) {
  return request<{ sessionId: string; status: string }>(
    '/api/sessions/start',
    {
      method: 'POST',
      // size：本端终端组件按设备屏幕预算的默认网格，主机 PTY 以此为初始尺寸
      body: JSON.stringify({ configId, cols: size?.cols, rows: size?.rows })
    }
  )
}

export async function httpStopSession(sessionId: string) {
  return request(`/api/sessions/${sessionId}/stop`, { method: 'POST' })
}

export async function httpResizeSession(
  sessionId: string,
  cols: number,
  rows: number,
  force = false,
): Promise<ApiResult<ResizeOutcome>> {
  return request(`/api/sessions/${sessionId}/resize`, {
    method: 'POST',
    body: JSON.stringify({ cols, rows, force }),
  })
}

export async function httpRemoveSession(sessionId: string) {
  return request(`/api/sessions/${sessionId}/remove`, { method: 'DELETE' })
}

/** 通过 HTTP API 发送终端输入（绕过 WebSocket 阻塞） */
export async function httpSendSessionInput(sessionId: string, data: string, specialKey?: string) {
  return request(`/api/sessions/${sessionId}/input`, {
    method: 'POST',
    body: JSON.stringify({ data, specialKey: specialKey || null }),
  })
}

// ==================== Config API ====================

export async function httpListConfigs() {
  return request<{ configs: any[] }>('/api/configs')
}

export async function httpListQuickActions() {
  return request<{ actions: any[] }>('/api/quick-actions')
}

// ==================== File API ====================

export async function httpGetFileTree(sessionId: string, excludeDirs: string[] = []) {
  return request<{ tree: any[] }>(
    '/api/file-tree',
    { method: 'POST', body: JSON.stringify({ sessionId, excludeDirs }) }
  )
}

/** 获取指定目录的一层子节点（懒加载模式） */
export async function httpGetFileTreeChildren(
  sessionId: string,
  dirPath: string,
  excludeDirs: string[] = [],
  noCache = false,
) {
  const params = new URLSearchParams({
    session_id: sessionId,
    dir_path: dirPath || '.',
    exclude_dirs: excludeDirs.join(','),
  })
  // 刷新时附加时间戳绕过 HTTP 缓存
  if (noCache) {
    params.set('_t', Date.now().toString())
  }
  return request<{ children: any[] }>(
    `/api/file-tree-children?${params.toString()}`,
  )
}

export async function httpGetFileContent(sessionId: string, filePath: string) {
  return request<{ content: string; fileName: string }>(
    '/api/file-content',
    { method: 'POST', body: JSON.stringify({ sessionId, filePath }) }
  )
}

export async function httpGetDiffTree(sessionId: string, excludeDirs: string[] = []) {
  return request<{ tree: any[] }>(
    '/api/diff-tree',
    { method: 'POST', body: JSON.stringify({ sessionId, excludeDirs }) }
  )
}

export async function httpGetFileDiff(sessionId: string, filePath: string) {
  return request<{ fileName: string; lines: FileDiffLine[] }>(
    '/api/file-diff',
    { method: 'POST', body: JSON.stringify({ sessionId, filePath }) }
  )
}

// ==================== Plugin API ====================

/** 设置会话自动模式（auto_execute / auto_answer） */
export async function httpSetSessionMode(sessionId: string, autoExecute?: boolean, autoAnswer?: boolean) {
  const body: { session_id: string; auto_execute?: boolean; auto_answer?: boolean } = { session_id: sessionId }
  if (autoExecute !== undefined) body.auto_execute = autoExecute
  if (autoAnswer !== undefined) body.auto_answer = autoAnswer
  return request(
    '/api/plugin/com.bedcode.auto-task/session-mode',
    { method: 'POST', body: JSON.stringify(body) }
  )
}

// ==================== Auto Task Queue API ====================

/** 队列任务项 */
export interface AutoTaskQueueItem {
  id: string
  prompt: string
  position: number
  status: string
  created_at: string
}

/** 任务队列列表响应 */
export interface QueueListResponse {
  session_id: string
  tasks: AutoTaskQueueItem[]
  queue_count: number
  /** 当前处理中的队列项（waiting=等待 clear 后下发 / executing=已下发未完成），对账与状态展示用 */
  active_task?: AutoTaskQueueItem | null
}

/** 查询任务队列 */
export async function httpTaskQueueList(sessionId: string) {
  return request<QueueListResponse>(
    `/api/plugin/com.bedcode.auto-task/task-queue/list?session_id=${encodeURIComponent(sessionId)}`
  )
}

/** 添加任务到队列 */
export async function httpTaskQueueAdd(sessionId: string, prompt: string) {
  return request(
    '/api/plugin/com.bedcode.auto-task/task-queue/add',
    { method: 'POST', body: JSON.stringify({ session_id: sessionId, prompt }) }
  )
}

/** 从队列删除任务 */
export async function httpTaskQueueRemove(sessionId: string, taskId: string) {
  return request(
    '/api/plugin/com.bedcode.auto-task/task-queue/remove',
    { method: 'DELETE', body: JSON.stringify({ session_id: sessionId, task_id: taskId }) }
  )
}

/** 取消活动队列项（waiting / executing） */
export async function httpTaskQueueCancel(sessionId: string, taskId: string) {
  return request(
    '/api/plugin/com.bedcode.auto-task/task-queue/cancel',
    { method: 'POST', body: JSON.stringify({ session_id: sessionId, task_id: taskId }) }
  )
}

/** 清空任务队列 */
export async function httpTaskQueueClear(sessionId: string) {
  return request(
    '/api/plugin/com.bedcode.auto-task/task-queue/clear',
    { method: 'POST', body: JSON.stringify({ session_id: sessionId }) }
  )
}

/** 更新队列任务内容 */
export async function httpTaskQueueUpdate(sessionId: string, taskId: string, prompt: string) {
  return request(
    '/api/plugin/com.bedcode.auto-task/task-queue/update',
    { method: 'POST', body: JSON.stringify({ session_id: sessionId, task_id: taskId, prompt }) }
  )
}

/** 重排序任务队列 */
export async function httpTaskQueueReorder(sessionId: string, taskIds: string[]) {
  return request(
    '/api/plugin/com.bedcode.auto-task/task-queue/reorder',
    { method: 'POST', body: JSON.stringify({ session_id: sessionId, task_ids: taskIds }) }
  )
}

/** 会话设置响应 */
export interface SessionSettingsData {
  session_id: string
  auto_execute: boolean
  auto_answer: boolean
}

/** 查询会话设置（auto_execute / auto_answer） */
export async function httpSessionSettings(sessionId: string) {
  return request<SessionSettingsData>(
    `/api/plugin/com.bedcode.auto-task/session-settings?session_id=${encodeURIComponent(sessionId)}`
  )
}

/** 当前任务响应 */
export interface CurrentTaskData {
  session_id: string
  task: {
    id: string
    description: string | null
    status: string
    auto_approve: number
    created_at: string
    started_at: string | null
    completed_at: string | null
  } | null
}

/** 查询会话当前任务 */
export async function httpCurrentTask(sessionId: string) {
  return request<CurrentTaskData>(
    `/api/plugin/com.bedcode.auto-task/task-history/current?session_id=${encodeURIComponent(sessionId)}`
  )
}

/** 查询 auto-task 支持的 agent 列表 */
export async function httpListSupportedAgents() {
  return request<{ agents: string[] }>(
    '/api/plugin/com.bedcode.auto-task/supported-agents'
  )
}

// ==================== Auto Task History & Scheduled ====================

/** 任务历史条目 */
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

/** 任务历史列表响应（分页字段由后端原样返回） */
export interface TaskHistoryListResponse {
  tasks: TaskHistoryItem[]
  total: number
  limit: number
  offset: number
}

/**
 * 查询任务历史列表
 *
 * 只拼接已提供的筛选参数，空值不出现；时间字段（since/until）
 * 为 UTC `YYYY-MM-DD HH:MM:SS` 字符串。
 */
export async function httpTaskHistoryList(params?: {
  status?: string
  agent?: string
  source?: string
  since?: string
  until?: string
  limit?: number
  offset?: number
}) {
  const query = new URLSearchParams()
  if (params?.status) query.set('status', params.status)
  if (params?.agent) query.set('agent', params.agent)
  if (params?.source) query.set('source', params.source)
  if (params?.since) query.set('since', params.since)
  if (params?.until) query.set('until', params.until)
  if (params?.limit !== undefined) query.set('limit', String(params.limit))
  if (params?.offset !== undefined) query.set('offset', String(params.offset))
  const qs = query.toString()
  return request<TaskHistoryListResponse>(
    `/api/plugin/com.bedcode.auto-task/task-history/list${qs ? `?${qs}` : ''}`
  )
}

/** 定时任务条目 */
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

/** 定时任务列表响应 */
export interface ScheduledJobsListResponse {
  jobs: ScheduledJob[]
}

/** 查询定时任务列表 */
export async function httpScheduledJobsList() {
  return request<ScheduledJobsListResponse>(
    '/api/plugin/com.bedcode.auto-task/scheduled-jobs/list'
  )
}

/** 创建定时任务请求体 */
export interface ScheduledJobCreateBody {
  name?: string
  config_id: string
  trigger_at: string
  prompts: string[]
}

/** 创建定时任务（trigger_at 为 UTC `YYYY-MM-DD HH:MM:SS`） */
export async function httpScheduledJobCreate(body: ScheduledJobCreateBody) {
  return request<{ job_id: string }>(
    '/api/plugin/com.bedcode.auto-task/scheduled-jobs/create',
    { method: 'POST', body: JSON.stringify(body) }
  )
}

// ==================== Git API ====================

/** Git 分支列表响应 */
export interface GitBranchesData {
  currentBranch: string | null
  branches: string[]
  isGitRepo: boolean
}

/** Git 工作区状态响应 */
export interface GitStatusData {
  hasChanges: boolean
  changedCount: number
}

/** 获取 git 分支列表 */
export async function httpGetGitBranches(sessionId: string) {
  return request<GitBranchesData>(
    `/api/git/branches?session_id=${encodeURIComponent(sessionId)}`
  )
}

/** 检查工作区是否有未提交的更改 */
export async function httpGetGitStatus(sessionId: string) {
  return request<GitStatusData>(
    `/api/git/status?session_id=${encodeURIComponent(sessionId)}`
  )
}

/** 切换 git 分支 */
export async function httpGitCheckout(sessionId: string, branch: string) {
  return request<{ branch: string }>(
    '/api/git/checkout',
    { method: 'POST', body: JSON.stringify({ sessionId, branch }) }
  )
}

// ==================== Setup ====================

export function setApiBaseUrl(address: string, port: number) {
  API_BASE_URL.value = `${address}:${port}`
  // Egress L1：声明桌面端目标（httpProbe 在 ws_connect 前执行，target 未设——
  // 此处提前声明使 probe/会话内 desktop 类请求放行，时序见 ticket 03 方案 a）
  invoke('egress_declare_desktop_target', { address, port }).catch(() => {
    // 声明失败（如测试环境无后端）不阻断；L1 判定在 Rust 端兜底
  })
}

// ==================== Connectivity Probe ====================

/** HTTP 探测结果 */
export interface ProbeResult {
  reachable: boolean
  status?: string
  port?: number
  uptimeSecs?: number
  error?: string
}

/**
 * 探测桌面端 HTTP 服务是否可达
 *
 * 在 WS 连接前调用，3 秒超时快速判断网络连通性。
 * 失败时立即返回而非等待 10 秒 WS 超时。
 * 经统一代理（desktop 类 + L1 放行）；Rust 端先行声明目标。
 */
export async function httpProbe(address: string, port: number): Promise<ProbeResult> {
  const url = `http://${address}:${port}/api/health`
  logger.log('[HttpApi] Probing:', url)

  try {
    // 声明目标（L1；probe 在 ws_connect 前，ConnectionManager.target 未设）
    await invoke('egress_declare_desktop_target', { address, port })
    const resp = await invoke<HttpProxyResponse>('http_request', {
      requestId: uuidv4(),
      method: 'GET',
      url,
      headers: {},
      body: null,
      timeoutMs: 3000,
      kind: 'desktop',
    })

    if (resp.status !== 200) {
      return { reachable: false, error: `HTTP ${resp.status}` }
    }

    const data = JSON.parse(resp.bodyText)
    logger.log('[HttpApi] Probe success:', data)
    return {
      reachable: true,
      status: data.status,
      port: data.port,
      uptimeSecs: data.uptime_secs,
    }
  } catch (e: any) {
    logger.warn('[HttpApi] Probe failed:', e?.message || e)
    return { reachable: false, error: e?.message || String(e) }
  }
}

export function useHttpApi() {
  return {
    setApiBaseUrl,
    // Probe
    httpProbe,
    // Auth
    httpRequestPairing,
    httpVerifyPairingCode,
    httpQrConnect,
    httpReauth,
    // Session
    httpListSessions,
    httpStartSession,
    httpStopSession,
    httpResizeSession,
    httpRemoveSession,
    httpSendSessionInput,
    // Config
    httpListConfigs,
    httpListQuickActions,
    // File
    httpGetFileTree,
    httpGetFileTreeChildren,
    httpGetFileContent,
    httpGetDiffTree,
    httpGetFileDiff,
    // Plugin
    httpSetSessionMode,
    // Auto Task Queue
    httpTaskQueueList,
    httpTaskQueueAdd,
    httpTaskQueueRemove,
    httpTaskQueueCancel,
    httpTaskQueueClear,
    httpTaskQueueUpdate,
    httpTaskQueueReorder,
    httpSessionSettings,
    httpCurrentTask,
    // Auto Task History & Scheduled
    httpTaskHistoryList,
    httpScheduledJobsList,
    httpScheduledJobCreate,
    // Git
    httpGetGitBranches,
    httpGetGitStatus,
    httpGitCheckout,
  }
}
