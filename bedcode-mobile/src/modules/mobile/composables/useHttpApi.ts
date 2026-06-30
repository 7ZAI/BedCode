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

/** 设置会话自动授权模式 */
export async function httpSetSessionMode(sessionId: string, autoApprove: boolean) {
  return request('/api/plugin/session-mode', {
    method: 'POST',
    body: JSON.stringify({ session_id: sessionId, auto_approve: autoApprove, token: '' }),
  })
}

// ==================== Setup ====================

export function setApiBaseUrl(address: string, port: number) {
  API_BASE_URL.value = `${address}:${port}`
}

export function useHttpApi() {
  return {
    setApiBaseUrl,
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
    // Config
    httpListConfigs,
    httpListQuickActions,
    // File
    httpGetFileTree,
    httpGetFileContent,
    httpGetDiffTree,
    httpGetFileDiff,
    // Plugin
    httpSetSessionMode,
  }
}
