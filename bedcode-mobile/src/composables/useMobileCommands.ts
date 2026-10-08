//! Mobile Commands - Rust 后端命令封装
//!
//! 所有移动端可用的 Tauri 命令调用

import { invoke, type Channel } from '@tauri-apps/api/core'
import { logger } from '@/utils/frontendLogger'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

// ==================== Types ====================

import type {
  ConnectionStatus,
  RemoteDevice,
  AuthCredentials,
  ConnectionInfo,
  SessionInfo,
  RemoteSession,
} from './model'

export type {
  ConnectionStatus,
  RemoteDevice,
  AuthCredentials,
  ConnectionInfo,
  SessionInfo,
  RemoteSession,
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

/**
 * 同步「自动重连」开关到 Rust 连接层
 *
 * 自动重连由 `EventWsSupervisor` 唯一执行、退避节奏由 `ReconnectManager` 决定，
 * 前端不持有重连循环，只把用户意图递过去。
 */
export async function setAutoReconnect(enabled: boolean): Promise<void> {
  return await invoke('set_auto_reconnect', { enabled })
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

// ==================== Auth Commands（票 14 阶段 B：配对 / 认证编排迁插件） ====================
// 配对 / QR / 生物挑战流程归 `com.bedcode.terminal-session` 插件（WIT host-auth
// 触达宿主引擎，凭据零过境——插件拿不到 token，JWT 由宿主落地 global token +
// 凭据表）。流程事件（ws_pairing_request / ws_pairing_verified / ws_paired /
// ws_auth_failed）由插件经 host-events 广播，事件名与载荷与退役前逐字一致，
// initMobileEventListeners 的监听零改动。前端 localStorage 持久化镜像在受理后
// 经宿主窄读命令 `ws_get_auth_credentials` 取数（C4：凭据持有与读取面都在
// 宿主引擎，不经插件）。`ws_authenticate`（重启 / 重连后的 JWT 换新）与生物
// 凭证绑定面（Keystore，C4）保留为宿主命令。

/** 插件命令结果形状：accepted = 桌面端受理且凭据已落地宿主 */
interface PluginAuthOutcome {
  accepted: boolean
}

/**
 * 使用 JWT token 认证（重连时使用已存储的 session_token）
 */
export async function wsAuthenticate(sessionToken: string): Promise<boolean> {
  return await invoke('ws_authenticate', { sessionToken })
}

/**
 * 请求配对（桌面端生成一次性配对码并展示）
 */
export async function wsRequestPairing(): Promise<void> {
  await invoke('plugin_invoke', {
    pluginId: TERMINAL_PLUGIN_ID,
    command: 'terminal-session.request-pairing',
    args: {},
  })
}

/**
 * 验证配对码，成功后返回凭据（含 JWT token，宿主引擎窄读）
 */
export async function wsVerifyPairingCode(code: string): Promise<AuthCredentials | null> {
  const outcome = await invoke<PluginAuthOutcome>('plugin_invoke', {
    pluginId: TERMINAL_PLUGIN_ID,
    command: 'terminal-session.verify-pairing-code',
    args: { code },
  })
  return outcome.accepted ? await invoke('ws_get_auth_credentials') : null
}

/**
 * 使用 QR token 认证，成功后返回凭据（含 JWT token，宿主引擎窄读）
 */
export async function wsAuthenticateWithQr(token: string): Promise<AuthCredentials | null> {
  const outcome = await invoke<PluginAuthOutcome>('plugin_invoke', {
    pluginId: TERMINAL_PLUGIN_ID,
    command: 'terminal-session.authenticate-with-qr',
    args: { token },
  })
  return outcome.accepted ? await invoke('ws_get_auth_credentials') : null
}

/**
 * 生物认证登录（挑战-应答握手，弹系统生物识别），成功后返回凭据（宿主引擎窄读）
 */
export async function wsAuthenticateWithBiometric(): Promise<AuthCredentials | null> {
  const outcome = await invoke<PluginAuthOutcome>('plugin_invoke', {
    pluginId: TERMINAL_PLUGIN_ID,
    command: 'terminal-session.authenticate-with-biometric',
    args: {},
  })
  return outcome.accepted ? await invoke('ws_get_auth_credentials') : null
}

/**
 * 绑定生物凭证：本地生成密钥对并注册公钥到桌面端（需已认证连接）
 */
export async function wsBindBiometricCredential(): Promise<boolean> {
  return await invoke('ws_bind_biometric_credential')
}

/**
 * 解绑生物凭证：删除本地密钥并通知桌面端清空公钥（需已认证连接）
 */
export async function wsUnbindBiometricCredential(): Promise<boolean> {
  return await invoke('ws_unbind_biometric_credential')
}

/**
 * 生物认证密钥状态（设备支持 + 本地密钥已生成）
 */
export interface BiometricKeyStatus {
  deviceSupported: boolean
  /** BiometricManager 结果码：0=SUCCESS 1=HW_UNAVAILABLE 11=NONE_ENROLLED 12=NO_HARDWARE；-1=未知/插件异常 */
  deviceReason: number
  hasKey: boolean
}

export async function wsGetBiometricKeyStatus(): Promise<BiometricKeyStatus> {
  return await invoke('ws_get_biometric_key_status')
}

// ==================== Session Commands ====================

// 票 04：会话控制 / 终端输入 / 配置查询已迁桌面 HTTP 面（`useHttpApi`）——
// 旧 WS `Message` 信封命令（ws_load_sessions / ws_send_input_async /
// get_terminal_ws_info 等）随协议退役删除，本段不再提供对应 wrapper。

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

// 同步事件监听器
let unlistenSyncSessionCreated: UnlistenFn | null = null
let unlistenSyncSessionStatusChanged: UnlistenFn | null = null
let unlistenSyncSessionStopped: UnlistenFn | null = null
let unlistenSyncSessionRemoved: UnlistenFn | null = null
let unlistenSyncConfigCreated: UnlistenFn | null = null
let unlistenSyncConfigUpdated: UnlistenFn | null = null
let unlistenSyncConfigRemoved: UnlistenFn | null = null
let unlistenSyncTaskStatusChanged: UnlistenFn | null = null
let unlistenSyncTaskQueueChanged: UnlistenFn | null = null

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

  // 初始化同步事件监听
  if (callbacks.onSyncSessionCreated) {
    unlistenSyncSessionCreated = await listen<{ session: any; source_device: string }>('ws_sync_session_created', (event) => {
      logger.debug('[MobileCommands] ws_sync_session_created:', event.payload.session.id, 'source:', event.payload.source_device)
      callbacks.onSyncSessionCreated?.(event.payload)
    })
  }
  if (callbacks.onSyncSessionStatusChanged) {
    unlistenSyncSessionStatusChanged = await listen<{ session_id: string; old_status: string; new_status: string; session_name: string }>('ws_sync_session_status_changed', (event) => {
      logger.debug('[MobileCommands] ws_sync_session_status_changed:', event.payload.session_id, event.payload.old_status, '->', event.payload.new_status)
      callbacks.onSyncSessionStatusChanged?.(event.payload)
    })
  }
  if (callbacks.onSyncSessionStopped) {
    unlistenSyncSessionStopped = await listen<{ session_id: string; session_name: string }>('ws_sync_session_stopped', (event) => {
      logger.debug('[MobileCommands] ws_sync_session_stopped:', event.payload.session_id, event.payload.session_name)
      callbacks.onSyncSessionStopped?.(event.payload)
    })
  }
  if (callbacks.onSyncSessionRemoved) {
    unlistenSyncSessionRemoved = await listen<{ session_id: string; session_name: string }>('ws_sync_session_removed', (event) => {
      logger.debug('[MobileCommands] ws_sync_session_removed:', event.payload.session_id, event.payload.session_name)
      callbacks.onSyncSessionRemoved?.(event.payload)
    })
  }
  if (callbacks.onSyncConfigCreated) {
    unlistenSyncConfigCreated = await listen<{ config: any; source_device: string }>('ws_sync_config_created', (event) => {
      logger.debug('[MobileCommands] ws_sync_config_created:', event.payload.config.id, 'source:', event.payload.source_device)
      callbacks.onSyncConfigCreated?.(event.payload)
    })
  }
  if (callbacks.onSyncConfigUpdated) {
    unlistenSyncConfigUpdated = await listen<{ config: any; source_device: string }>('ws_sync_config_updated', (event) => {
      logger.debug('[MobileCommands] ws_sync_config_updated:', event.payload.config.id, 'source:', event.payload.source_device)
      callbacks.onSyncConfigUpdated?.(event.payload)
    })
  }
  if (callbacks.onSyncConfigRemoved) {
    unlistenSyncConfigRemoved = await listen<{ config_id: string; config_name: string }>('ws_sync_config_removed', (event) => {
      logger.debug('[MobileCommands] ws_sync_config_removed:', event.payload.config_id, event.payload.config_name)
      callbacks.onSyncConfigRemoved?.(event.payload)
    })
  }
  if (callbacks.onSyncTaskStatusChanged) {
    unlistenSyncTaskStatusChanged = await listen<{ session_id: string; task_status: string; task_reason?: string; task_questions?: Array<{ header: string; question: string; multi_select: boolean; options: Array<{ label: string; description: string }> }> }>('ws_sync_task_status_changed', (event) => {
      logger.debug('[MobileCommands] ws_sync_task_status_changed:', event.payload.session_id, 'status:', event.payload.task_status, 'reason:', event.payload.task_reason ?? 'none')
      callbacks.onSyncTaskStatusChanged?.(event.payload)
    })
  }

  // 任务队列变更转发：无条件监听并转发为 window CustomEvent，供插件（任务域面板）
  // 订阅完成广播（action='done' + task_id）更新预设任务执行状态。插件不直接依赖
  // @tauri-apps/api，经宿主转发保持插件/宿主边界（dev-shell 可手动 dispatch 模拟）
  unlistenSyncTaskQueueChanged = await listen<{ session_id: string; queue_count: number; action: string; task_id?: string | null; status?: string | null }>('ws_sync_task_queue_changed', (event) => {
    logger.debug('[MobileCommands] ws_sync_task_queue_changed:', event.payload.session_id, 'action:', event.payload.action, 'task_id:', event.payload.task_id ?? 'none')
    window.dispatchEvent(new CustomEvent('bedcode:task_queue_changed', { detail: event.payload }))
  })
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
  // 清理同步事件监听
  unlistenSyncSessionCreated?.()
  unlistenSyncSessionStatusChanged?.()
  unlistenSyncSessionStopped?.()
  unlistenSyncSessionRemoved?.()
  unlistenSyncConfigCreated?.()
  unlistenSyncConfigUpdated?.()
  unlistenSyncConfigRemoved?.()
  unlistenSyncTaskStatusChanged?.()
  unlistenSyncTaskQueueChanged?.()
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

    // Auth（票 14 阶段 B：配对 / QR / 生物挑战编排已迁插件，wsGetAuthStatus
    // 零消费者退役）
    wsAuthenticate,
    wsRequestPairing,
    wsVerifyPairingCode,
    wsAuthenticateWithQr,

    // Session（票 04：会话控制/终端输入已迁 HTTP，旧 WS 信封命令退役删除——
    // 会话列表经 httpListSessions，输入经 httpSendSessionInput）

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
// ==================== Terminal Link（票 12：订阅协议客户端已迁插件） ====================
// 协议面（订阅/退订/输入/ack/状态）走 `com.bedcode.terminal-session` 插件命令面
// （plugin_invoke）；段2 页面 Channel 登记（terminal_page_subscribe/unsubscribe）
// 是 Tauri 传输机制，保留为宿主命令。插件未激活时 plugin_invoke 显性报错
// （fail-visible，对齐「插件未激活时前端命令面显性报错」）。

/** 终端订阅协议客户端所在插件的命令 id（D6 选项 A：与桌面同名，职责不同） */
const TERMINAL_PLUGIN_ID = 'com.bedcode.terminal-session'

/**
 * 订阅会话终端输出（进入终端页 / 预加载触发）。fresh subscribe 语义：链路已在
 * 运行时发 subscribe 帧重播环窗口；未建立时建连 + 订阅（认证由宿主代发）。
 * 意外断开由宿主自动重连，插件重连恢复后重新订阅（无续传语义）
 */
export async function terminalSubscribe(sessionId: string): Promise<void> {
  return invoke('plugin_invoke', {
    pluginId: TERMINAL_PLUGIN_ID,
    command: 'terminal-session.subscribe',
    args: { sessionId },
  })
}

/** 取消订阅（离开终端页 / 会话停止 / 手动断开）：关闭连接不再重连 */
export async function terminalUnsubscribe(sessionId: string): Promise<void> {
  return invoke('plugin_invoke', {
    pluginId: TERMINAL_PLUGIN_ID,
    command: 'terminal-session.unsubscribe',
    args: { sessionId },
  })
}

/**
 * 段2：订阅（进入终端页）— 携带页面级 Tauri Channel，Rust 经它以**裸字节**
 * 推送输出（帧的唯一出口；状态/重锚仍走全局事件）。
 *
 * 幂等，且不依赖链路是否已建立（通道与订阅意愿先于链路记录，链路建立后即生效）。
 *
 * 为什么用 Channel 而非全局事件：per-page 通道没有全局广播与事件名匹配开销，负载走
 * Raw 字节省掉 base64（-33% 体积）与 JSON 序列化/解析（与桌面端终端输出同路径）
 */
export async function terminalPageSubscribe(
  sessionId: string,
  channel: Channel<ArrayBuffer>,
): Promise<void> {
  return invoke('terminal_page_subscribe', { sessionId, channel })
}

/** 段2：取消订阅（退出终端页）— 清空推送通道 */
export async function terminalPageUnsubscribe(sessionId: string): Promise<void> {
  return invoke('terminal_page_unsubscribe', { sessionId })
}

/** 全部取消订阅（设备手动断开 / 连接关闭） */
export async function terminalUnsubscribeAll(): Promise<void> {
  return invoke('plugin_invoke', {
    pluginId: TERMINAL_PLUGIN_ID,
    command: 'terminal-session.unsubscribe-all',
    args: {},
  })
}

/** 会话删除：清理插件侧链路与订阅态 */
export async function terminalRemove(sessionId: string): Promise<void> {
  return invoke('plugin_invoke', {
    pluginId: TERMINAL_PLUGIN_ID,
    command: 'terminal-session.remove',
    args: { sessionId },
  })
}

/**
 * 发送终端输入（前端 → 插件 → WS 帧 → 桌面端 PTY）。双形态：可打印文本
 * （data）→ `{"type":"input","data":"<UTF-8>"}`；特殊键（specialKey）→
 * 插件 keys 翻译 → binary 帧原始字节（投递失败上抛：半截输入护栏）
 */
export async function terminalSendInput(
  sessionId: string,
  data: string,
  specialKey?: string | null,
): Promise<void> {
  return invoke('plugin_invoke', {
    pluginId: TERMINAL_PLUGIN_ID,
    command: 'terminal-session.send-input',
    args: { sessionId, data, specialKey: specialKey ?? null },
  })
}

/** 渲染背压 ack：本地已渲染字节数推进插件侧 ack 水位（64KB 阈值节流回发桌面端） */
export async function terminalAckRendered(sessionId: string, offset: number): Promise<void> {
  return invoke('plugin_invoke', {
    pluginId: TERMINAL_PLUGIN_ID,
    command: 'terminal-session.ack-rendered',
    args: { sessionId, offset },
  })
}

/** Rust 侧链路状态（诊断/轮询） */
export interface TerminalLinkState {
  sessionId: string
  phase: string
  /** 本地已收字节（统计；重锚后归零） */
  cursor: number
  /** 前端已渲染字节（本地计数，ack 帧 offset 值） */
  acked: number
  stopped: boolean
  /** 已收 subscribed（门控） */
  subscribed: boolean
}

export async function terminalGetState(sessionId: string): Promise<TerminalLinkState> {
  return invoke('plugin_invoke', {
    pluginId: TERMINAL_PLUGIN_ID,
    command: 'terminal-session.get-state',
    args: { sessionId },
  })
}
