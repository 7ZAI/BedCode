//! Mobile Connection Composable
//!
//! 移动端连接管理 - 基于 useMobileCommands 的高级封装

import { ref, computed, readonly } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useToast } from '@/modules/shared/composables/useToast'
import {
  wsConnect,
  wsDisconnect,
  wsGetStatus,
  wsIsConnected,
  wsReconnect,
  wsAuthenticate,
  wsRequestPairing,
  wsVerifyPairingCode,
  wsLoadSessionConfigs,
  wsLoadSessions,
  wsStartSession,
  wsStopSession,
  wsSendInput,
  initMobileEventListeners,
  cleanupMobileEventListeners,
  saveAuthCredentials,
  loadAuthCredentials,
  clearAuthCredentials,
  type ConnectionStatus,
  type RemoteDevice,
  type AuthCredentials,
} from './useMobileCommands'

// Re-export types
export type { ConnectionStatus, RemoteDevice, AuthCredentials } from './useMobileCommands'

// ==================== State ====================

const connectionStatus = ref<ConnectionStatus>('disconnected')
const currentDevice = ref<RemoteDevice | null>(null)
const connectionError = ref<string | null>(null)
const isConnecting = ref(false)

// 连接超时控制
let connectionTimeout: ReturnType<typeof setTimeout> | null = null
const CONNECTION_TIMEOUT_MS = 15000 // 15秒超时

// 意外断开监听器
let unlistenUnexpectedDisconnect: UnlistenFn | null = null

// 认证凭据
const authCredentials = ref<AuthCredentials | null>(null)

// 当前活跃会话 ID
const activeSessionId = ref<string | null>(null)

// 最后收到的消息
const lastMessage = ref<any>(null)

// ==================== Global Session State ====================
// 会话配置列表（全局状态，切换页面不丢失）
interface SessionConfigSummary {
  id: string
  name: string
  environment: string
  wsl_distro?: string
  working_dir: string
  command: string
}
const sessionConfigs = ref<SessionConfigSummary[]>([])
const isLoadingConfigs = ref(false)
const hasLoadedConfigs = ref(false)

// 活跃会话列表（全局状态）
const activeSessions = ref<any[]>([])

// 连接历史（全局状态）
interface ConnectionHistoryItem {
  address: string
  name: string
  lastConnected: string
}
const connectionHistory = ref<ConnectionHistoryItem[]>([])
const historyLoaded = ref(false)

// ==================== Computed ====================

export const isConnected = computed(() =>
  connectionStatus.value === 'connected' ||
  connectionStatus.value === 'paired'
)

export const isPaired = computed(() =>
  connectionStatus.value === 'paired'
)

// ==================== Initialization ====================

let initialized = false

/**
 * 初始化事件监听和凭据（全局单例，仅执行一次）
 * 模块加载时立即执行，确保事件监听在用户操作前注册完毕
 * Tauri 事件不会缓冲，监听器必须在事件触发前注册
 */
async function init() {
  if (initialized) return
  initialized = true
  // 加载保存的凭据
  const savedCreds = loadAuthCredentials()
  console.log('[MobileConnection] init() loaded credentials:', savedCreds ? { ...savedCreds, sessionToken: savedCreds.sessionToken ? `length=${savedCreds.sessionToken.length}` : 'missing' } : null)
  if (savedCreds) {
    authCredentials.value = savedCreds
  }

  // 初始化事件监听 - 状态由后端事件驱动
  await initMobileEventListeners({
    onConnecting: () => {
      connectionStatus.value = 'connecting'
      isConnecting.value = true
      connectionError.value = null
      console.log('[MobileConnection] Connecting...')
    },
    onConnected: () => {
      clearConnectionTimeout()
      connectionStatus.value = 'connected'
      isConnecting.value = false
      connectionError.value = null
      console.log('[MobileConnection] Connected')
    },
    onDisconnected: () => {
      clearConnectionTimeout()
      connectionStatus.value = 'disconnected'
      isConnecting.value = false
      console.log('[MobileConnection] Disconnected')
    },
    onPaired: () => {
      clearConnectionTimeout()
      connectionStatus.value = 'paired'
      isConnecting.value = false
      console.log('[MobileConnection] Paired')
    },
    onAuthSuccess: () => {
      // ws_paired 会触发 onPaired
      console.log('[MobileConnection] Auth success')
    },
    onAuthFailed: (reason) => {
      connectionStatus.value = 'error'
      connectionError.value = reason
      isConnecting.value = false
      console.log('[MobileConnection] Auth failed:', reason)
    },
    onPairingRequest: () => {
      clearConnectionTimeout()
      connectionStatus.value = 'pairing'
      console.log('[MobileConnection] Pairing requested')
    },
    onPairingVerified: () => {
      // 等待 ws_paired 事件
      console.log('[MobileConnection] Pairing verified')
    },
    onError: (message) => {
      clearConnectionTimeout()
      connectionError.value = message
      connectionStatus.value = 'error'
      isConnecting.value = false
      console.error('[MobileConnection] Error:', message)
    },
    onServerClosed: (reason) => {
      clearConnectionTimeout()
      connectionStatus.value = 'disconnected'
      connectionError.value = reason
      isConnecting.value = false
      console.log('[MobileConnection] Server closed:', reason)
    },
    // 同步事件回调
    onSyncConfigCreated: (data) => {
      console.log('[MobileConnection] SyncConfigCreated:', data.config.id, 'source:', data.source_device)
      // 添加新配置到列表
      const newConfig = {
        id: data.config.id,
        name: data.config.name,
        environment: data.config.environment,
        wsl_distro: data.config.wsl_distro,
        working_dir: data.config.working_dir,
        command: data.config.command,
      }
      // 避免重复添加
      if (!sessionConfigs.value.find(c => c.id === newConfig.id)) {
        sessionConfigs.value.push(newConfig)
      }
    },
    onSyncConfigUpdated: (data) => {
      console.log('[MobileConnection] SyncConfigUpdated:', data.config.id, 'source:', data.source_device)
      // 更新现有配置
      const index = sessionConfigs.value.findIndex(c => c.id === data.config.id)
      if (index !== -1) {
        sessionConfigs.value[index] = {
          id: data.config.id,
          name: data.config.name,
          environment: data.config.environment,
          wsl_distro: data.config.wsl_distro,
          working_dir: data.config.working_dir,
          command: data.config.command,
        }
      } else {
        // 如果配置不存在，添加它
        sessionConfigs.value.push({
          id: data.config.id,
          name: data.config.name,
          environment: data.config.environment,
          wsl_distro: data.config.wsl_distro,
          working_dir: data.config.working_dir,
          command: data.config.command,
        })
      }
    },
    onSyncConfigRemoved: (data) => {
      console.log('[MobileConnection] SyncConfigRemoved:', data.config_id, data.config_name)
      // 从列表移除配置
      sessionConfigs.value = sessionConfigs.value.filter(c => c.id !== data.config_id)
    },
    onSyncSessionCreated: (data) => {
      console.log('[MobileConnection] SyncSessionCreated:', data.session.id, 'source:', data.source_device)
      // 添加新会话到列表
      if (!activeSessions.value.find(s => s.id === data.session.id)) {
        activeSessions.value.push(data.session)
      }
    },
    onSyncSessionStatusChanged: (data) => {
      console.log('[MobileConnection] SyncSessionStatusChanged:', data.session_id, data.old_status, '->', data.new_status)
      // 更新会话状态
      const index = activeSessions.value.findIndex(s => s.id === data.session_id)
      if (index !== -1) {
        activeSessions.value[index].status = data.new_status
      }
    },
    onSyncSessionStopped: (data) => {
      console.log('[MobileConnection] SyncSessionStopped:', data.session_id, data.session_name)
      // 从活跃列表移除
      activeSessions.value = activeSessions.value.filter(s => s.id !== data.session_id)
    },
    onSyncSessionRemoved: (data) => {
      console.log('[MobileConnection] SyncSessionRemoved:', data.session_id, data.session_name)
      // 从列表移除会话
      activeSessions.value = activeSessions.value.filter(s => s.id !== data.session_id)
    },
  })

  // 监听意外断开事件（Rust 端 WsClient 检测到异常断开时发射）
  unlistenUnexpectedDisconnect = await listen<{ reason: string }>('ws_unexpected_disconnect', (event) => {
    console.warn('[MobileConnection] Unexpected disconnect:', event.payload.reason)
    connectionStatus.value = 'disconnected'
    connectionError.value = event.payload.reason
    isConnecting.value = false

    // 弹出 Toast 通知（手动断开不会触发此事件）
    const toast = useToast()
    toast.error(`连接已断开: ${event.payload.reason}`, 5000)

    // 触发重连
    handleUnexpectedDisconnect(event.payload.reason)
  })
}

// 模块加载时立即初始化，确保事件监听尽早注册
init()

// ==================== Operations ====================

/**
 * 连接到设备
 */
export async function connect(device: RemoteDevice): Promise<void> {
  console.log('[MobileConnection] Starting connection to:', device.address, device.port)
  currentDevice.value = device
  connectionError.value = null
  isConnecting.value = true

  // 设置连接超时
  clearConnectionTimeout()
  connectionTimeout = setTimeout(() => {
    if (isConnecting.value) {
      console.warn('[MobileConnection] Connection timeout, disconnecting...')
      connectionError.value = '连接超时 (15秒)'
      connectionStatus.value = 'error'
      isConnecting.value = false
      disconnect()
    }
  }, CONNECTION_TIMEOUT_MS)

  try {
    // 调用后端连接，状态由后端事件驱动更新
    const result = await wsConnect(device.address, device.port, device.name)
    console.log('[MobileConnection] wsConnect returned:', result)
  } catch (error) {
    clearConnectionTimeout()
    console.error('[MobileConnection] wsConnect failed:', error)
    // 如果命令本身失败（如地址无效），抛出错误
    // 状态由后端事件驱动更新，不需要手动设置
    isConnecting.value = false
    throw error
  }
}

/**
 * 取消连接
 */
export async function cancelConnection(): Promise<void> {
  clearConnectionTimeout()
  if (isConnecting.value) {
    console.log('[MobileConnection] Cancelling connection...')
    connectionError.value = '用户取消连接'
    connectionStatus.value = 'disconnected'
    isConnecting.value = false
    await disconnect()
  }
}

/**
 * 清除连接超时定时器
 */
function clearConnectionTimeout() {
  if (connectionTimeout) {
    clearTimeout(connectionTimeout)
    connectionTimeout = null
  }
}

/**
 * 处理意外断开，尝试重连
 */
async function handleUnexpectedDisconnect(reason: string) {
  console.log('[MobileConnection] Handling unexpected disconnect, reason:', reason)

  // 从 localStorage 读取凭据
  const creds = loadAuthCredentials()
  if (!creds) {
    console.log('[MobileConnection] No credentials found, cannot reconnect')
    return
  }

  // 检查是否有目标设备
  if (!currentDevice.value) {
    console.log('[MobileConnection] No target device, cannot reconnect')
    return
  }

  console.log('[MobileConnection] Starting reconnect with token, length:', creds.sessionToken.length)

  try {
    await wsReconnect(creds.sessionToken)
    console.log('[MobileConnection] Reconnect initiated successfully')
  } catch (error) {
    console.error('[MobileConnection] Reconnect failed:', error)
    connectionError.value = `重连失败: ${error}`
  }
}

/**
 * 断开连接
 */
export async function disconnect(): Promise<void> {
  try {
    await wsDisconnect()
    // 状态由后端 ws_disconnected 事件驱动更新
  } finally {
    currentDevice.value = null
  }
}

/**
 * 使用已存储的 JWT token 重新认证（重连时调用）
 * 带 5 秒超时，超时后自动降级到配对流程
 */
export async function authenticate(): Promise<boolean> {
  console.log('[MobileConnection] authenticate() called')
  console.log('[MobileConnection]   authCredentials.value =', authCredentials.value)
  console.log('[MobileConnection]   localStorage auth_session_token =', localStorage.getItem('auth_session_token'))
  console.log('[MobileConnection]   localStorage auth_pairing_id =', localStorage.getItem('auth_pairing_id'))
  console.log('[MobileConnection]   localStorage auth_fingerprint =', localStorage.getItem('auth_fingerprint'))

  if (!authCredentials.value?.sessionToken) {
    console.log('[MobileConnection] No stored credentials, skipping auth -> false')
    return false
  }

  console.log('[MobileConnection] Attempting JWT re-auth, token length:', authCredentials.value.sessionToken.length)
  try {
    const result = await Promise.race([
      wsAuthenticate(authCredentials.value.sessionToken),
      new Promise<boolean>((_, reject) =>
        setTimeout(() => reject(new Error('Auth timeout')), 5000)
      ),
    ])
    console.log('[MobileConnection] Auth result:', result)
    if (!result) {
      clearAuthCredentials()
      authCredentials.value = null
    }
    return result
  } catch (error) {
    console.error('[MobileConnection] Auth failed/timeout:', error)
    clearAuthCredentials()
    authCredentials.value = null
    return false
  }
}

/**
 * 请求配对
 */
export async function requestPairing(): Promise<void> {
  console.log('[MobileConnection] requestPairing: calling wsRequestPairing (invoke)...')
  await wsRequestPairing()
  console.log('[MobileConnection] requestPairing: wsRequestPairing returned')
  // 状态由后端事件驱动
}

/**
 * 验证配对码，成功后保存凭据
 */
export async function verifyPairingCode(code: string): Promise<boolean> {
  try {
    const creds = await wsVerifyPairingCode(code)
    if (creds) {
      // 成功时后端会 emit ws_pairing_verified 和 ws_paired 事件
      // 保存 JWT 凭据到 localStorage，后续请求携带此 token
      saveCredentials(creds)
      return true
    }
    return false
  } catch (error) {
    console.error('[MobileConnection] Pairing verification failed:', error)
    return false
  }
}

/**
 * 加载会话配置列表
 */
export async function loadSessionConfigs(): Promise<any[]> {
  const configs = await wsLoadSessionConfigs()
  sessionConfigs.value = configs.map((c: any) => ({
    id: c.id,
    name: c.name,
    environment: c.environment,
    wsl_distro: c.wsl_distro,
    working_dir: c.working_dir,
    command: c.command,
  }))
  hasLoadedConfigs.value = true
  return configs
}

/**
 * 加载活跃会话列表
 */
export async function loadActiveSessions(): Promise<any[]> {
  const sessions = await wsLoadSessions()
  activeSessions.value = sessions
  return sessions
}

/**
 * 启动会话，返回完整会话信息
 */
export async function startSession(configId: string, sessionName?: string): Promise<{ sessionId: string; session?: any }> {
  const result = await wsStartSession(configId, sessionName)
  return result
}

/**
 * 停止会话
 */
export async function stopSession(sessionId: string): Promise<void> {
  await wsStopSession(sessionId)
  // 从本地列表移除
  activeSessions.value = activeSessions.value.filter(s => s.id !== sessionId)
}

/**
 * 加载连接历史
 */
export function loadConnectionHistory(): void {
  if (historyLoaded.value) return
  const stored = localStorage.getItem('connection_history')
  if (stored) {
    try {
      connectionHistory.value = JSON.parse(stored)
    } catch {
      connectionHistory.value = []
    }
  }
  historyLoaded.value = true
}

/**
 * 保存连接历史到 localStorage
 */
export function saveConnectionHistory(): void {
  localStorage.setItem('connection_history', JSON.stringify(connectionHistory.value))
}

/**
 * 添加到连接历史
 */
export function addToConnectionHistory(address: string, name?: string): void {
  connectionHistory.value = connectionHistory.value.filter(item => item.address !== address)
  connectionHistory.value.unshift({
    address,
    name: name || address.split(':')[0],
    lastConnected: new Date().toISOString(),
  })
  if (connectionHistory.value.length > 10) {
    connectionHistory.value = connectionHistory.value.slice(0, 10)
  }
  saveConnectionHistory()
}

/**
 * 从连接历史移除
 */
export function removeFromConnectionHistory(address: string): void {
  connectionHistory.value = connectionHistory.value.filter(item => item.address !== address)
  saveConnectionHistory()
}

/**
 * 清除连接历史
 */
export function clearConnectionHistory(): void {
  connectionHistory.value = []
  saveConnectionHistory()
}

/**
 * 清除会话配置（断开连接时）
 */
export function clearSessionConfigs(): void {
  sessionConfigs.value = []
  hasLoadedConfigs.value = false
}

/**
 * 清除活跃会话（断开连接时）
 */
export function clearActiveSessions(): void {
  activeSessions.value = []
}

/**
 * 发送输入到会话
 */
export async function sendInput(sessionId: string, data: string, specialKey?: string): Promise<void> {
  await wsSendInput(sessionId, data, specialKey)
}

/**
 * 保存认证凭据
 */
export function saveCredentials(creds: AuthCredentials) {
  authCredentials.value = creds
  saveAuthCredentials(creds)
}

/**
 * 清除认证凭据
 */
export function clearCredentials() {
  authCredentials.value = null
  clearAuthCredentials()
}

// ==================== Main Composable ====================

/**
 * 移动端连接管理 composable
 *
 * 全局单例模式：连接状态在 app 生命周期内共享。
 * 首次调用时延迟初始化事件监听，后续组件复用同一份状态。
 */
export function useMobileConnection() {
  return {
    // State
    connectionStatus: readonly(connectionStatus),
    currentDevice: readonly(currentDevice),
    connectionError: readonly(connectionError),
    isConnecting,  // 不使用 readonly，允许组件设置
    authCredentials: readonly(authCredentials),
    activeSessionId,
    lastMessage,

    // Global Session State
    sessionConfigs,
    activeSessions,
    connectionHistory,
    isLoadingConfigs,
    hasLoadedConfigs,

    // Computed
    isConnected,
    isPaired,

    // Operations
    connect,
    cancelConnection,
    disconnect,
    authenticate,
    requestPairing,
    verifyPairingCode,
    loadSessionConfigs,
    loadActiveSessions,
    startSession,
    stopSession,
    sendInput,
    saveCredentials,
    clearCredentials,

    // Connection History Operations
    loadConnectionHistory,
    saveConnectionHistory,
    addToConnectionHistory,
    removeFromConnectionHistory,
    clearConnectionHistory,

    // Clear Operations
    clearSessionConfigs,
    clearActiveSessions,
  }
}