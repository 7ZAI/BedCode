//! Mobile Commands - Rust 后端命令封装
//!
//! 所有移动端可用的 Tauri 命令调用

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

// ==================== Types ====================

import type {
  ConnectionStatus,
  RemoteDevice,
  AuthCredentials,
  ConnectionInfo,
  AuthState,
  SessionInfo,
  RemoteSession,
  TerminalOutputEvent,
  TerminalIncrementalOutput,
} from './model'
export type {
  ConnectionStatus,
  RemoteDevice,
  AuthCredentials,
  ConnectionInfo,
  AuthState,
  SessionInfo,
  RemoteSession,
  TerminalOutputEvent,
  TerminalIncrementalOutput,
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

/**
 * 重新连接（断线重连）
 */
export async function wsReconnect(sessionToken?: string): Promise<void> {
  return await invoke('ws_reconnect', { sessionToken: sessionToken || null })
}

// ==================== Token Commands ====================

/**
 * 设置全局 Token（前端启动时从 localStorage 读取并调用）
 */
export async function wsSetToken(token: string): Promise<void> {
  return await invoke('ws_set_token', { token })
}

/**
 * 获取当前全局 Token
 */
export async function wsGetToken(): Promise<string> {
  return await invoke('ws_get_token')
}

/**
 * 清除全局 Token（登出时调用）
 */
export async function wsClearToken(): Promise<void> {
  return await invoke('ws_clear_token')
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
 * 订阅会话，开始接收该会话的输出
 *
 * @param sessionId - 会话 ID
 * @param startSeq - 起始序号，不指定则从头补完所有历史；用于断线重连从断点继续
 * @returns 订阅响应信息，包含 minSeq/maxSeq/historyCount
 */
export async function wsJoinSession(sessionId: string, startSeq?: number): Promise<{ minSeq: number; maxSeq: number; historyCount: number }> {
  console.log('[wsJoinSession] sessionId=' + sessionId + ', startSeq=' + startSeq)
  return await invoke('ws_subscribe_session', { sessionId, startSeq: startSeq ?? null })
}

/**
 * 取消订阅会话，停止接收该会话的输出
 */
export async function wsLeaveSession(sessionId: string): Promise<void> {
  console.log('[wsLeaveSession] sessionId=' + sessionId)
  return await invoke('ws_leave_session', { sessionId })
}

/**
 * 启动会话，返回会话 ID 和会话信息
 */
export async function wsStartSession(configId: string, sessionName?: string): Promise<{ sessionId: string; session?: any }> {
  return await invoke('ws_start_session', { configId: configId, sessionName: sessionName })
}

/**
 * 停止会话
 */
export async function wsStopSession(sessionId: string): Promise<void> {
  return await invoke('ws_stop_session', { sessionId })
}

/**
 * 删除会话
 */
export async function wsRemoveSession(sessionId: string): Promise<void> {
  return await invoke('ws_remove_session', { sessionId })
}

/**
 * 发送输入到会话（异步模式，不等待服务端确认）
 */
export async function wsSendInput(sessionId: string, data: string, specialKey?: string): Promise<void> {
  console.log('[wsSendInput] sessionId=' + sessionId + ' data_len=' + data.length + ' specialKey=' + (specialKey || 'none'))
  return await invoke('ws_send_input_async', { sessionId, data, specialKey: specialKey })
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

// ==================== Terminal Commands ====================

/**
 * 订阅终端（记录当前索引位置，用于增量获取）
 */
export async function wsSubscribeTerminal(sessionId: string): Promise<number> {
  return await invoke('ws_subscribe_terminal', { sessionId })
}

/**
 * 取消订阅终端
 */
export async function wsUnsubscribeTerminal(sessionId: string): Promise<void> {
  return await invoke('ws_unsubscribe_terminal', { sessionId })
}

/**
 * 获取增量输出（自上次获取之后的新数据）
 */
export async function wsGetTerminalIncremental(sessionId: string): Promise<TerminalIncrementalOutput | null> {
  return await invoke('ws_get_terminal_incremental', { sessionId })
}

/**
 * 更新订阅者的索引位置（在增量数据消费后调用）
 */
export async function wsUpdateTerminalIndex(sessionId: string, index: number): Promise<void> {
  return await invoke('ws_update_terminal_index', { sessionId, index })
}

/**
 * 清空终端缓冲区
 */
export async function wsClearTerminalBuffer(sessionId: string): Promise<void> {
  return await invoke('ws_clear_terminal_buffer', { sessionId })
}

/**
 * 清除所有终端缓冲区（断开连接时调用）
 */
export async function wsClearAllTerminalBuffers(): Promise<void> {
  return await invoke('ws_clear_all_terminal_buffers')
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
let unlistenUnexpectedDisconnect: UnlistenFn | null = null
let unlistenOutput: UnlistenFn | null = null

// 同步事件监听器
let unlistenSyncSessionCreated: UnlistenFn | null = null
let unlistenSyncSessionStatusChanged: UnlistenFn | null = null
let unlistenSyncSessionStopped: UnlistenFn | null = null
let unlistenSyncSessionRemoved: UnlistenFn | null = null
let unlistenSyncConfigCreated: UnlistenFn | null = null
let unlistenSyncConfigUpdated: UnlistenFn | null = null
let unlistenSyncConfigRemoved: UnlistenFn | null = null
let unlistenSyncTaskStatusChanged: UnlistenFn | null = null

/**
 * 同步事件回调接口
 */
export interface SyncEventCallbacks {
  onSyncSessionCreated?: (data: { session: any; source_device: string }) => void
  onSyncSessionStatusChanged?: (data: { session_id: string; old_status: string; new_status: string; session_name: string }) => void
  onSyncSessionStopped?: (data: { session_id: string; session_name: string }) => void
  onSyncSessionRemoved?: (data: { session_id: string; session_name: string }) => void
  onSyncConfigCreated?: (data: { config: any; source_device: string }) => void
  onSyncConfigUpdated?: (data: { config: any; source_device: string }) => void
  onSyncConfigRemoved?: (data: { config_id: string; config_name: string }) => void
  onSyncTaskStatusChanged?: (data: { session_id: string; task_status: string; task_reason?: string; task_questions?: Array<{ header: string; question: string; multi_select: boolean; options: Array<{ label: string; description: string }> }> }) => void
}

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
  onUnexpectedDisconnect?: (reason: string) => void
  onOutput?: (data: any) => void
  // 同步事件回调
  onSyncSessionCreated?: (data: { session: any; source_device: string }) => void
  onSyncSessionStatusChanged?: (data: { session_id: string; old_status: string; new_status: string; session_name: string }) => void
  onSyncSessionStopped?: (data: { session_id: string; session_name: string }) => void
  onSyncSessionRemoved?: (data: { session_id: string; session_name: string }) => void
  onSyncConfigCreated?: (data: { config: any; source_device: string }) => void
  onSyncConfigUpdated?: (data: { config: any; source_device: string }) => void
  onSyncConfigRemoved?: (data: { config_id: string; config_name: string }) => void
  onSyncTaskStatusChanged?: (data: { session_id: string; task_status: string; task_reason?: string; task_questions?: Array<{ header: string; question: string; multi_select: boolean; options: Array<{ label: string; description: string }> }> }) => void
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
  if (callbacks.onUnexpectedDisconnect) {
    unlistenUnexpectedDisconnect = await listen<{ reason: string }>('ws_unexpected_disconnect', (event) => {
      callbacks.onUnexpectedDisconnect?.(event.payload.reason)
    })
  }
  if (callbacks.onOutput) {
    unlistenOutput = await listen('ws_output', callbacks.onOutput)
  }

  // 初始化同步事件监听
  if (callbacks.onSyncSessionCreated) {
    unlistenSyncSessionCreated = await listen<{ session: any; source_device: string }>('ws_sync_session_created', (event) => {
      console.debug('[MobileCommands] ws_sync_session_created:', event.payload.session.id, 'source:', event.payload.source_device)
      callbacks.onSyncSessionCreated?.(event.payload)
    })
  }
  if (callbacks.onSyncSessionStatusChanged) {
    unlistenSyncSessionStatusChanged = await listen<{ session_id: string; old_status: string; new_status: string; session_name: string }>('ws_sync_session_status_changed', (event) => {
      console.debug('[MobileCommands] ws_sync_session_status_changed:', event.payload.session_id, event.payload.old_status, '->', event.payload.new_status)
      callbacks.onSyncSessionStatusChanged?.(event.payload)
    })
  }
  if (callbacks.onSyncSessionStopped) {
    unlistenSyncSessionStopped = await listen<{ session_id: string; session_name: string }>('ws_sync_session_stopped', (event) => {
      console.debug('[MobileCommands] ws_sync_session_stopped:', event.payload.session_id, event.payload.session_name)
      callbacks.onSyncSessionStopped?.(event.payload)
    })
  }
  if (callbacks.onSyncSessionRemoved) {
    unlistenSyncSessionRemoved = await listen<{ session_id: string; session_name: string }>('ws_sync_session_removed', (event) => {
      console.debug('[MobileCommands] ws_sync_session_removed:', event.payload.session_id, event.payload.session_name)
      callbacks.onSyncSessionRemoved?.(event.payload)
    })
  }
  if (callbacks.onSyncConfigCreated) {
    unlistenSyncConfigCreated = await listen<{ config: any; source_device: string }>('ws_sync_config_created', (event) => {
      console.debug('[MobileCommands] ws_sync_config_created:', event.payload.config.id, 'source:', event.payload.source_device)
      callbacks.onSyncConfigCreated?.(event.payload)
    })
  }
  if (callbacks.onSyncConfigUpdated) {
    unlistenSyncConfigUpdated = await listen<{ config: any; source_device: string }>('ws_sync_config_updated', (event) => {
      console.debug('[MobileCommands] ws_sync_config_updated:', event.payload.config.id, 'source:', event.payload.source_device)
      callbacks.onSyncConfigUpdated?.(event.payload)
    })
  }
  if (callbacks.onSyncConfigRemoved) {
    unlistenSyncConfigRemoved = await listen<{ config_id: string; config_name: string }>('ws_sync_config_removed', (event) => {
      console.debug('[MobileCommands] ws_sync_config_removed:', event.payload.config_id, event.payload.config_name)
      callbacks.onSyncConfigRemoved?.(event.payload)
    })
  }
  if (callbacks.onSyncTaskStatusChanged) {
    unlistenSyncTaskStatusChanged = await listen<{ session_id: string; task_status: string; task_reason?: string; task_questions?: Array<{ header: string; question: string; multi_select: boolean; options: Array<{ label: string; description: string }> }> }>('ws_sync_task_status_changed', (event) => {
      console.debug('[MobileCommands] ws_sync_task_status_changed:', event.payload.session_id, 'status:', event.payload.task_status, 'reason:', event.payload.task_reason ?? 'none')
      callbacks.onSyncTaskStatusChanged?.(event.payload)
    })
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
  unlistenUnexpectedDisconnect?.()
  unlistenOutput?.()
  // 清理同步事件监听
  unlistenSyncSessionCreated?.()
  unlistenSyncSessionStatusChanged?.()
  unlistenSyncSessionStopped?.()
  unlistenSyncSessionRemoved?.()
  unlistenSyncConfigCreated?.()
  unlistenSyncConfigUpdated?.()
  unlistenSyncConfigRemoved?.()
  unlistenSyncTaskStatusChanged?.()
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

    // Token
    wsSetToken,
    wsGetToken,
    wsClearToken,

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

    // Terminal (Rust-managed buffer)
    wsSubscribeTerminal,
    wsUnsubscribeTerminal,
    wsGetTerminalIncremental,
    wsUpdateTerminalIndex,
    wsClearTerminalBuffer,
    wsClearAllTerminalBuffers,

    // Android-specific
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