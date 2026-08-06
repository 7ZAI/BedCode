/**
 * HTTP API Client Composable
 *
 * 移动端直接调用桌面端 HTTP REST API
 * 使用 @tauri-apps/plugin-http 替代浏览器 fetch，绕过 CORS 和网络限制
 * JWT token 自动注入到 Authorization header
 */

import { ref } from 'vue'
import { fetch as tauriFetch } from '@tauri-apps/plugin-http'
import { useMobileConnection } from './useMobileConnection'

// ==================== Config ====================

const API_BASE_URL = ref<string>('')

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

async function request<T = any>(
  path: string,
  options: RequestInit = {}
): Promise<ApiResult<T>> {
  const { authCredentials } = useMobileConnection()
  const baseUrl = API_BASE_URL.value

  if (!baseUrl) {
    console.error('[HttpApi] No base URL set, cannot make request to', path)
    return { code: -1, message: 'Not connected: no base URL set' }
  }

  const url = `http://${baseUrl}${path}`
  console.log('[HttpApi] Request:', options.method || 'GET', url)

  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...(options.headers as Record<string, string>),
  }

  // 注入 JWT token（auth 路由除外）
  if (!path.startsWith('/api/auth/') && authCredentials.value?.sessionToken) {
    headers['Authorization'] = `Bearer ${authCredentials.value.sessionToken}`
  }

  try {
    const response = await tauriFetch(url, {
      ...options,
      headers,
      connectTimeout: 30000,
    })

    if (!response.ok) {
      const text = await response.text().catch(() => '')
      console.error('[HttpApi] HTTP error:', response.status, response.statusText, text)
      return { code: response.status, message: `HTTP ${response.status}: ${response.statusText}` }
    }

    const result = await response.json()
    console.log('[HttpApi] Response OK:', path, 'code=', result.code)
    return result
  } catch (e: any) {
    console.error('[HttpApi] Fetch failed:', path, e?.message || e)
    return { code: -1, message: e?.message || String(e) }
  }
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

export async function httpStartSession(configId: string) {
  return request<{ sessionId: string; status: string }>(
    '/api/sessions/start',
    { method: 'POST', body: JSON.stringify({ configId }) }
  )
}

export async function httpStopSession(sessionId: string) {
  return request(`/api/sessions/${sessionId}/stop`, { method: 'POST' })
}

export async function httpResizeSession(sessionId: string, cols: number, rows: number) {
  return request(`/api/sessions/${sessionId}/resize`, {
    method: 'POST',
    body: JSON.stringify({ cols, rows }),
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
  const body: Record<string, any> = { session_id: sessionId }
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
 */
export async function httpProbe(address: string, port: number): Promise<ProbeResult> {
  const url = `http://${address}:${port}/api/health`
  console.log('[HttpApi] Probing:', url)

  try {
    const response = await tauriFetch(url, {
      method: 'GET',
      connectTimeout: 3000,
    })

    if (!response.ok) {
      return { reachable: false, error: `HTTP ${response.status}` }
    }

    const data = await response.json()
    console.log('[HttpApi] Probe success:', data)
    return {
      reachable: true,
      status: data.status,
      port: data.port,
      uptimeSecs: data.uptime_secs,
    }
  } catch (e: any) {
    console.warn('[HttpApi] Probe failed:', e?.message || e)
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
    httpTaskQueueClear,
    httpTaskQueueUpdate,
    httpTaskQueueReorder,
    httpSessionSettings,
    httpCurrentTask,
    // Git
    httpGetGitBranches,
    httpGetGitStatus,
    httpGitCheckout,
  }
}
