import { ref, computed, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useWebSocket } from './useWebSocket'

export interface RemoteDevice {
  id: string
  name: string
  address: string
  port: number
  isPaired: boolean
}

export interface ConnectionState {
  status: 'disconnected' | 'connecting' | 'connected' | 'pairing' | 'paired' | 'error'
  error?: string
}

export interface PairedDeviceRaw {
  id: string
  device_name: string
  device_fingerprint: string
  public_key: string
  address?: string
  session_token?: string
  paired_at: string
  last_seen?: string
  is_active: boolean
}

// 重连回调类型
type ReconnectCallback = () => Promise<void>

// Singleton state — shared across all useRemoteConnection() calls so connection
// state and paired devices survive Vue component navigation.
const state = ref<ConnectionState>({ status: 'disconnected' })
const pairedDevices = ref<RemoteDevice[]>([])
const currentDevice = ref<RemoteDevice | null>(null)
const authCredentials = ref<{
  pairingId: string
  fingerprint: string
  sessionToken: string
} | null>(null)

// 当前活跃的会话 ID，供 QuickActionsView/HistoryView 等跨视图发送输入使用
const activeSessionId = ref<string | null>(null)

export function useRemoteConnection() {
  // === 状态 (singleton) ===
  // state, pairedDevices, currentDevice, authCredentials are module-level

  // === WebSocket 依赖 (singleton) ===
  const {
    isConnected,
    lastMessage,
    connectionError,
    connect: wsConnect,
    disconnect: wsDisconnect,
    sendMessage,
    sendMessageWithResponse,
    setOnReconnect,
  } = useWebSocket()

  // === 计算属性 ===
  const isReady = computed(() => state.value.status === 'paired' && isConnected.value)

  // === 方法 ===

  function setReconnectCallback(callback: ReconnectCallback | null) {
    setOnReconnect(callback)
  }

  /** 连接到设备 */
  async function connect(device: RemoteDevice): Promise<void> {
    state.value = { status: 'connecting' }
    currentDevice.value = device

    try {
      // 清除上一次连接的错误状态
      // connectionError 由 useWebSocket 管理，这里不需要手动清除

      wsConnect(device.address, device.port, false)

      // 等待连接建立
      await new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => {
          reject(new Error('Connection timeout'))
        }, 10000)

        const unwatch = setInterval(() => {
          if (isConnected.value) {
            clearTimeout(timeout)
            clearInterval(unwatch)
            resolve()
          }
          if (connectionError.value) {
            clearTimeout(timeout)
            clearInterval(unwatch)
            reject(new Error(connectionError.value))
          }
        }, 100)
      })

      state.value = { status: 'connected' }
    } catch (error) {
      state.value = { status: 'error', error: String(error) }
      throw error
    }
  }

  /** 使用已存储凭据重新认证（已配对设备重连） */
  async function authenticate(): Promise<boolean> {
    if (!isConnected.value) {
      console.warn('Cannot authenticate: not connected')
      return false
    }

    const creds = authCredentials.value
    if (!creds || !creds.fingerprint) {
      console.warn('Cannot authenticate: no stored credentials')
      return false
    }

    try {
      const response = await sendMessageWithResponse('auth', {
        stage: 'authenticated',
        device_id: creds.pairingId,
        device_fingerprint: creds.fingerprint,
        session_token: creds.sessionToken,
      })

      if (response.payload?.stage === 'authenticated') {
        state.value = { status: 'paired' }
        return true
      } else {
        console.warn('Re-authentication failed:', response.payload?.error)
        return false
      }
    } catch (error) {
      console.error('Re-authentication error:', error)
      return false
    }
  }

  /** 请求配对 */
  async function requestPairing(): Promise<void> {
    if (!isConnected.value || !currentDevice.value) {
      throw new Error('Not connected to any device')
    }

    state.value = { status: 'pairing' }

    try {
      const response = await sendMessageWithResponse('auth', {
        stage: 'request_pairing',
        device_id: generateDeviceId(),
        device_name: getDeviceName(),
        device_fingerprint: generateDeviceFingerprint(),
      })

      if (response.payload?.stage === 'verify_code') {
        // 等待用户输入配对码
      } else {
        throw new Error(response.payload?.error || 'Pairing request failed')
      }
    } catch (error) {
      state.value = { status: 'error', error: String(error) }
      throw error
    }
  }

  /** 验证配对码 */
  async function verifyPairingCode(code: string): Promise<boolean> {
    if (!isConnected.value) {
      throw new Error('Not connected')
    }

    try {
      const response = await sendMessageWithResponse('auth', {
        stage: 'verify_code',
        device_id: generateDeviceId(),
        device_name: getDeviceName(),
        device_fingerprint: generateDeviceFingerprint(),
        pairing_code: code,
      })

      if (response.payload?.stage === 'authenticated') {
        state.value = { status: 'paired' }

        // 存储服务端返回的凭据，用于重连
        const pairingId = response.payload?.device_id || ''
        const fingerprint = response.payload?.device_fingerprint || generateDeviceFingerprint()
        const sessionToken = response.payload?.session_token || ''

        authCredentials.value = {
          pairingId,
          fingerprint,
          sessionToken,
        }

        // 持久化凭据到 localStorage
        localStorage.setItem('auth_pairing_id', pairingId)
        localStorage.setItem('auth_fingerprint', fingerprint)
        localStorage.setItem('auth_session_token', sessionToken)

        // 更新当前设备为已配对
        if (currentDevice.value) {
          currentDevice.value.isPaired = true

          if (!pairedDevices.value.find(d => d.id === currentDevice.value!.id)) {
            pairedDevices.value.push(currentDevice.value)
          }
        }

        return true
      } else {
        state.value = { status: 'error', error: response.payload?.error || 'Pairing failed' }
        return false
      }
    } catch (error) {
      state.value = { status: 'error', error: String(error) }
      return false
    }
  }

  /** 通过 QR token 进行认证 */
  async function sendQrToken(token: string): Promise<boolean> {
    if (!isConnected.value) {
      throw new Error('Not connected')
    }

    const deviceId = generateDeviceId()
    const fingerprint = generateDeviceFingerprint()
    const deviceName = getDeviceName()

    const response = await sendMessageWithResponse('auth', {
      stage: 'qr_connect',
      device_id: deviceId,
      device_name: deviceName,
      device_fingerprint: fingerprint,
      qr_token: token,
    })

    if (response?.payload?.stage === 'authenticated') {
      const payload = response.payload as {
        device_id?: string
        device_fingerprint?: string
        session_token?: string
      }
      const pairingId = payload.device_id || ''
      const fp = payload.device_fingerprint || ''
      const st = payload.session_token || ''

      authCredentials.value = { pairingId, fingerprint: fp, sessionToken: st }

      // 持久化凭据
      localStorage.setItem('auth_pairing_id', pairingId)
      localStorage.setItem('auth_fingerprint', fp)
      localStorage.setItem('auth_session_token', st)

      // 添加到已配对设备列表
      pairedDevices.value.push({
        id: pairingId,
        name: deviceName,
        address: currentDevice.value?.address || '',
        port: currentDevice.value?.port || 8765,
        isPaired: true,
      })

      state.value = { status: 'paired' }
      return true
    }

    state.value = { status: 'error', error: response?.payload?.error || 'QR authentication failed' }
    return false
  }

  /** 发送输入到当前活跃会话（自动追加换行，与桌面端行为一致） */
  function sendInput(data: string, specialKey?: string): boolean {
    if (!isConnected.value || !activeSessionId.value) {
      console.warn('Cannot send input: not connected or no active session')
      return false
    }
    return sendMessage('input', {
      data: data + '\n',
      special_key: specialKey || null,
    }, activeSessionId.value)
  }

  /** 断开连接 */
  function disconnect(): void {
    activeSessionId.value = null
    wsDisconnect()
    state.value = { status: 'disconnected' }
    currentDevice.value = null
  }

  /** 加载已配对设备列表 */
  async function loadPairedDevices(): Promise<void> {
    try {
      const devices = await invoke<PairedDeviceRaw[]>('list_paired_devices')

      pairedDevices.value = devices.map(d => ({
        id: d.id,
        name: d.device_name,
        address: d.address || '',
        port: 8765,
        isPaired: true,
      }))
    } catch (error) {
      console.error('Failed to load paired devices:', error)
    }
  }

  /** 从 localStorage 加载认证凭据 */
  function loadAuthCredentials(): void {
    const pairingId = localStorage.getItem('auth_pairing_id')
    const fingerprint = localStorage.getItem('auth_fingerprint')
    const sessionToken = localStorage.getItem('auth_session_token')

    if (fingerprint && sessionToken) {
      authCredentials.value = {
        pairingId: pairingId || '',
        fingerprint,
        sessionToken,
      }
    }
  }

  /** 生成设备 ID */
  function generateDeviceId(): string {
    const stored = localStorage.getItem('device_id')
    if (stored) return stored

    const id = crypto.randomUUID()
    localStorage.setItem('device_id', id)
    return id
  }

  /** 生成设备指纹 */
  function generateDeviceFingerprint(): string {
    const stored = localStorage.getItem('device_fingerprint')
    if (stored) return stored

    const fp = crypto.randomUUID()
    localStorage.setItem('device_fingerprint', fp)
    return fp
  }

  /** 获取设备名称 */
  function getDeviceName(): string {
    return localStorage.getItem('device_name') || 'Mobile Device'
  }

  // === 初始化：加载持久化凭据（只执行一次） ===
  if (!authCredentials.value?.sessionToken) {
    loadAuthCredentials()
  }

  // === 清理：由 useWebSocket 的 usageCount 管理，这里不再主动断开 ===
  onUnmounted(() => {
    // WebSocket is managed by useWebSocket singleton, which only disconnects
    // when all components have unmounted (usageCount reaches 0).
  })

  return {
    // 状态
    state,
    pairedDevices,
    currentDevice,
    isConnected,
    lastMessage,
    isReady,
    authCredentials,
    activeSessionId,

    // 方法
    connect,
    authenticate,
    requestPairing,
    verifyPairingCode,
    sendQrToken,
    disconnect,
    loadPairedDevices,
    loadAuthCredentials,
    sendMessage,
    sendMessageWithResponse,
    sendInput,
    setReconnectCallback,
  }
}
