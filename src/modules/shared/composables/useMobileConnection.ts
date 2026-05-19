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
  return await wsLoadSessionConfigs()
}

/**
 * 启动会话
 */
export async function startSession(configId: string, sessionName?: string): Promise<string> {
  return await wsStartSession(configId, sessionName)
}

/**
 * 停止会话
 */
export async function stopSession(sessionId: string): Promise<void> {
  await wsStopSession(sessionId)
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
    startSession,
    stopSession,
    sendInput,
    saveCredentials,
    clearCredentials,
  }
}