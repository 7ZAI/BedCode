//! Mobile Connection Composable
//!
//! 移动端连接管理 - 基于 useMobileCommands 的高级封装

import { ref, computed, readonly } from 'vue'
import {
  wsConnect,
  wsDisconnect,
  wsGetStatus,
  wsIsConnected,
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

/**
 * 初始化事件监听和凭据
 */
async function init() {
  // 加载保存的凭据
  const savedCreds = loadAuthCredentials()
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
}

// 立即初始化
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
 * 使用已存储凭据认证
 */
export async function authenticate(): Promise<boolean> {
  if (!authCredentials.value) {
    console.warn('[MobileConnection] No stored credentials')
    return false
  }

  try {
    const result = await wsAuthenticate()
    // 成功时后端会 emit ws_auth_success 和 ws_paired 事件
    // 失败时后端会 emit ws_auth_failed 事件
    // 状态由事件驱动，不需要手动设置
    return result
  } catch (error) {
    console.error('[MobileConnection] Auth failed:', error)
    clearAuthCredentials()
    authCredentials.value = null
    return false
  }
}

/**
 * 请求配对
 */
export async function requestPairing(): Promise<void> {
  await wsRequestPairing()
  // 状态由后端事件驱动
}

/**
 * 验证配对码
 */
export async function verifyPairingCode(code: string): Promise<boolean> {
  try {
    const result = await wsVerifyPairingCode(code)
    // 成功时后端会 emit ws_pairing_verified 和 ws_paired 事件
    // 失败时后端会 emit ws_auth_failed 事件
    // 状态由事件驱动，不需要手动设置
    return result
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
export async function startSession(configId: string): Promise<string> {
  return await wsStartSession(configId)
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