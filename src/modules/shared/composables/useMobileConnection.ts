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
function init() {
  // 加载保存的凭据
  const savedCreds = loadAuthCredentials()
  if (savedCreds) {
    authCredentials.value = savedCreds
  }

  // 初始化事件监听
  initMobileEventListeners({
    onConnected: () => {
      connectionStatus.value = 'connected'
      isConnecting.value = false
      connectionError.value = null
      console.log('[MobileConnection] Connected')
    },
    onDisconnected: () => {
      connectionStatus.value = 'disconnected'
      console.log('[MobileConnection] Disconnected')
    },
    onAuthSuccess: () => {
      connectionStatus.value = 'paired'
      console.log('[MobileConnection] Auth success')
    },
    onAuthFailed: (reason) => {
      connectionStatus.value = 'error'
      connectionError.value = reason
      console.log('[MobileConnection] Auth failed:', reason)
    },
    onPairingRequest: () => {
      connectionStatus.value = 'pairing'
      console.log('[MobileConnection] Pairing requested')
    },
    onPairingVerified: () => {
      connectionStatus.value = 'paired'
      console.log('[MobileConnection] Pairing verified')
    },
    onError: (message) => {
      connectionError.value = message
      connectionStatus.value = 'error'
      isConnecting.value = false
      console.error('[MobileConnection] Error:', message)
    },
    onServerClosed: (reason) => {
      connectionStatus.value = 'disconnected'
      connectionError.value = reason
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
  connectionStatus.value = 'connecting'
  isConnecting.value = true
  currentDevice.value = device
  connectionError.value = null

  try {
    await wsConnect(device.address, device.port, device.name)

    // 等待连接成功事件触发状态更新
    await new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error('连接超时'))
      }, 15000)

      const checkConnection = setInterval(() => {
        if (connectionStatus.value === 'connected' || connectionStatus.value === 'paired') {
          clearTimeout(timeout)
          clearInterval(checkConnection)
          resolve()
        }
        if (connectionError.value) {
          clearTimeout(timeout)
          clearInterval(checkConnection)
          reject(new Error(connectionError.value))
        }
      }, 100)
    })
  } catch (error) {
    connectionStatus.value = 'error'
    connectionError.value = String(error)
    isConnecting.value = false
    throw error
  }
}

/**
 * 断开连接
 */
export async function disconnect(): Promise<void> {
  try {
    await wsDisconnect()
  } finally {
    connectionStatus.value = 'disconnected'
    currentDevice.value = null
    cleanupMobileEventListeners()
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
    if (result) {
      connectionStatus.value = 'paired'
    }
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
  connectionStatus.value = 'pairing'
}

/**
 * 验证配对码
 */
export async function verifyPairingCode(code: string): Promise<boolean> {
  try {
    const result = await wsVerifyPairingCode(code)
    if (result) {
      connectionStatus.value = 'paired'
    }
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
    isConnecting: readonly(isConnecting),
    authCredentials: readonly(authCredentials),
    activeSessionId,
    lastMessage,

    // Computed
    isConnected,
    isPaired,

    // Operations
    connect,
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