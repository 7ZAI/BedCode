//! Mobile Commands - Rust 后端命令封装
//!
//! 所有移动端可用的 Tauri 命令调用

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

// ==================== Types ====================

export type ConnectionStatus =
  | 'disconnected'
  | 'connecting'
  | 'connected'
  | 'pairing'
  | 'paired'
  | 'error'

export interface RemoteDevice {
  id: string
  name: string
  address: string
  port: number
  isPaired: boolean
}

export interface AuthCredentials {
  pairingId: string
  fingerprint: string
  sessionToken: string
}

export interface ConnectionInfo {
  address: string
  port: number
  status: string
}

export interface AuthState {
  status: string
  is_authenticated: boolean
}

export interface SessionInfo {
  id: string
  name: string
  config_id: string
  status: string
  created_at: string
}

// ==================== WebSocket Connection Commands ====================

/**
 * 连接到桌面端
 */
export async function wsConnect(address: string, port: number, name?: string): Promise<ConnectionInfo> {
  return await invoke('ws_connect', { address, port, name })
}

/**
 * 断开连接
 */
export async function wsDisconnect(): Promise<void> {
  return await invoke('ws_disconnect')
}

/**
 * 获取连接状态
 */
export async function wsGetStatus(): Promise<string> {
  return await invoke('ws_get_status')
}

/**
 * 检查是否已连接
 */
export async function wsIsConnected(): Promise<boolean> {
  return await invoke('ws_is_connected')
}

// ==================== Auth Commands ====================

/**
 * 获取认证状态
 */
export async function wsGetAuthStatus(): Promise<AuthState> {
  return await invoke('ws_get_auth_status')
}

/**
 * 使用 JWT token 认证（重连时使用已存储的 session_token）
 */
export async function wsAuthenticate(sessionToken: string): Promise<boolean> {
  return await invoke('ws_authenticate', { sessionToken })
}

/**
 * 请求配对
 */
export async function wsRequestPairing(): Promise<void> {
  return await invoke('ws_request_pairing')
}

/**
 * 验证配对码，成功后返回凭据（含 JWT token）
 */
export async function wsVerifyPairingCode(code: string): Promise<AuthCredentials | null> {
  return await invoke('ws_verify_pairing_code', { code })
}

/**
 * 使用 QR token 认证
 */
export async function wsAuthenticateWithQr(token: string): Promise<AuthCredentials | null> {
  return await invoke('ws_authenticate_with_qr', { token })
}

// ==================== Session Commands ====================

/**
 * 加载会话列表
 */
export async function wsLoadSessions(): Promise<SessionInfo[]> {
  return await invoke('ws_load_sessions')
}

/**
 * 启动会话
 */
export async function wsStartSession(configId: string, sessionName?: string): Promise<string> {
  return await invoke('ws_start_session', { configId, sessionName })
}

/**
 * 停止会话
 */
export async function wsStopSession(sessionId: string): Promise<void> {
  return await invoke('ws_stop_session', { sessionId })
}

/**
 * 发送输入到会话
 */
export async function wsSendInput(sessionId: string, data: string, specialKey?: string): Promise<void> {
  return await invoke('ws_send_input', { sessionId, data, specialKey })
}

/**
 * 调整终端大小
 */
export async function wsResizeTerminal(sessionId: string, cols: number, rows: number): Promise<void> {
  return await invoke('ws_resize_terminal', { sessionId, cols, rows })
}

/**
 * 加载会话配置列表
 */
export async function wsLoadSessionConfigs(): Promise<any[]> {
  return await invoke('ws_load_session_configs')
}

// ==================== Message Commands ====================

/**
 * 发送消息（不等待响应）
 */
export async function wsSendMessage(messageType: string, payload: any): Promise<void> {
  return await invoke('ws_send_message', { messageType, payload })
}

/**
 * 发送消息并等待响应
 */
export async function wsSendAndWait(
  messageType: string,
  payload: any,
  timeoutSecs?: number
): Promise<any> {
  return await invoke('ws_send_and_wait', { messageType, payload, timeoutSecs })
}

// ==================== Android-specific Commands ====================

/**
 * 获取 Android 状态栏高度
 */
export async function getStatusBarHeight(): Promise<number> {
  return await invoke('get_status_bar_height')
}

/**
 * 设置屏幕方向
 */
export async function setScreenOrientation(orientation: string): Promise<void> {
  return await invoke('set_screen_orientation', { orientation })
}

/**
 * 保持屏幕唤醒
 */
export async function keepScreenAwake(enabled: boolean): Promise<void> {
  return await invoke('keep_screen_awake', { enabled })
}

// ==================== Event Listeners ====================

let unlistenConnecting: UnlistenFn | null = null
let unlistenConnected: UnlistenFn | null = null
let unlistenDisconnected: UnlistenFn | null = null
let unlistenPaired: UnlistenFn | null = null
let unlistenAuthSuccess: UnlistenFn | null = null
let unlistenAuthFailed: UnlistenFn | null = null
let unlistenPairingRequest: UnlistenFn | null = null
let unlistenPairingVerified: UnlistenFn | null = null
let unlistenError: UnlistenFn | null = null
let unlistenServerClosed: UnlistenFn | null = null
let unlistenOutput: UnlistenFn | null = null

/**
 * 初始化事件监听
 */
export async function initMobileEventListeners(callbacks: {
  onConnecting?: () => void
  onConnected?: () => void
  onDisconnected?: () => void
  onPaired?: () => void
  onAuthSuccess?: () => void
  onAuthFailed?: (reason: string) => void
  onPairingRequest?: () => void
  onPairingVerified?: () => void
  onError?: (message: string) => void
  onServerClosed?: (reason: string) => void
  onOutput?: (data: any) => void
}) {
  if (callbacks.onConnecting) {
    unlistenConnecting = await listen('ws_connecting', callbacks.onConnecting)
  }
  if (callbacks.onConnected) {
    unlistenConnected = await listen('ws_connected', callbacks.onConnected)
  }
  if (callbacks.onDisconnected) {
    unlistenDisconnected = await listen('ws_disconnected', callbacks.onDisconnected)
  }
  if (callbacks.onPaired) {
    unlistenPaired = await listen('ws_paired', callbacks.onPaired)
  }
  if (callbacks.onAuthSuccess) {
    unlistenAuthSuccess = await listen('ws_auth_success', callbacks.onAuthSuccess)
  }
  if (callbacks.onAuthFailed) {
    unlistenAuthFailed = await listen<{ reason: string }>('ws_auth_failed', (event) => {
      callbacks.onAuthFailed?.(event.payload.reason)
    })
  }
  if (callbacks.onPairingRequest) {
    unlistenPairingRequest = await listen('ws_pairing_request', callbacks.onPairingRequest)
  }
  if (callbacks.onPairingVerified) {
    unlistenPairingVerified = await listen('ws_pairing_verified', callbacks.onPairingVerified)
  }
  if (callbacks.onError) {
    unlistenError = await listen<{ message: string }>('ws_error', (event) => {
      callbacks.onError?.(event.payload.message)
    })
  }
  if (callbacks.onServerClosed) {
    unlistenServerClosed = await listen<{ reason: string }>('ws_server_closed', (event) => {
      callbacks.onServerClosed?.(event.payload.reason)
    })
  }
  if (callbacks.onOutput) {
    unlistenOutput = await listen('ws_output', callbacks.onOutput)
  }
}

/**
 * 清理所有事件监听
 */
export function cleanupMobileEventListeners() {
  unlistenConnecting?.()
  unlistenConnected?.()
  unlistenDisconnected?.()
  unlistenPaired?.()
  unlistenAuthSuccess?.()
  unlistenAuthFailed?.()
  unlistenPairingRequest?.()
  unlistenPairingVerified?.()
  unlistenError?.()
  unlistenServerClosed?.()
  unlistenOutput?.()
}

// ==================== Mobile Commands Composable ====================

/**
 * 移动端命令 composable
 * 整合所有移动端可用的 Rust 命令
 */
export function useMobileCommands() {
  return {
    // Connection
    wsConnect,
    wsDisconnect,
    wsGetStatus,
    wsIsConnected,

    // Auth
    wsGetAuthStatus,
    wsAuthenticate,
    wsRequestPairing,
    wsVerifyPairingCode,
    wsAuthenticateWithQr,

    // Session
    wsLoadSessions,
    wsStartSession,
    wsStopSession,
    wsSendInput,
    wsResizeTerminal,
    wsLoadSessionConfigs,

    // Message
    wsSendMessage,
    wsSendAndWait,

    // Android-specific
    getStatusBarHeight,
    setScreenOrientation,
    keepScreenAwake,

    // Events
    initMobileEventListeners,
    cleanupMobileEventListeners,
  }
}

// ==================== Utility Functions ====================

/**
 * 保存认证凭据到 localStorage
 */
export function saveAuthCredentials(creds: AuthCredentials) {
  localStorage.setItem('auth_pairing_id', creds.pairingId)
  localStorage.setItem('auth_fingerprint', creds.fingerprint)
  localStorage.setItem('auth_session_token', creds.sessionToken)
}

/**
 * 从 localStorage 加载认证凭据
 */
export function loadAuthCredentials(): AuthCredentials | null {
  const pairingId = localStorage.getItem('auth_pairing_id')
  const fingerprint = localStorage.getItem('auth_fingerprint')
  const sessionToken = localStorage.getItem('auth_session_token')

  if (pairingId && fingerprint && sessionToken) {
    return { pairingId, fingerprint, sessionToken }
  }
  return null
}

/**
 * 清除认证凭据
 */
export function clearAuthCredentials() {
  localStorage.removeItem('auth_pairing_id')
  localStorage.removeItem('auth_fingerprint')
  localStorage.removeItem('auth_session_token')
}