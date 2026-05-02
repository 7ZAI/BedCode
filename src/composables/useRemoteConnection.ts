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

export interface DiscoveredDeviceRaw {
  name: string
  address: string
  port: number
  properties: Record<string, string>
  discovered_at: string
}

export interface PairedDeviceRaw {
  id: string
  device_name: string
  device_fingerprint: string
  public_key: string
  paired_at: string
  last_seen?: string
  is_active: boolean
}

// 重连回调类型
type ReconnectCallback = () => Promise<void>

export function useRemoteConnection() {
  // === 状态 ===
  const state = ref<ConnectionState>({ status: 'disconnected' })
  const discoveredDevices = ref<RemoteDevice[]>([])
  const pairedDevices = ref<RemoteDevice[]>([])
  const currentDevice = ref<RemoteDevice | null>(null)

  // === WebSocket 依赖 ===
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

  /**
   * 设置重连后的回调函数
   * 用于在重连后恢复之前的会话订阅
   */
  function setReconnectCallback(callback: ReconnectCallback | null) {
    setOnReconnect(callback)
  }

  // === 方法 ===

  /** 发现局域网设备 (mDNS) */
  async function discoverDevices(): Promise<void> {
    state.value = { status: 'connecting' }

    try {
      // 启动 mDNS 发现服务
      await invoke('start_discovery')

      // 等待一段时间收集设备
      await new Promise(resolve => setTimeout(resolve, 3000))

      // 获取发现的设备列表
      const devices = await invoke<DiscoveredDeviceRaw[]>('get_discovered_devices')

      discoveredDevices.value = devices.map(d => ({
        id: `${d.address}:${d.port}`,
        name: d.name,
        address: d.address,
        port: d.port,
        isPaired: false,
      }))

      state.value = { status: 'disconnected' }
    } catch (error) {
      state.value = { status: 'error', error: String(error) }
      console.error('Discovery failed:', error)
    }
  }

  /** 连接到设备 */
  async function connect(device: RemoteDevice): Promise<void> {
    state.value = { status: 'connecting' }
    currentDevice.value = device

    try {
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

  /** 请求配对 - 发起配对流程 */
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
        pairing_code: code,
      })

      if (response.payload?.stage === 'authenticated') {
        state.value = { status: 'paired' }

        // 更新当前设备为已配对
        if (currentDevice.value) {
          currentDevice.value.isPaired = true

          // 添加到已配对设备列表
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

  /** 断开连接 */
  function disconnect(): void {
    wsDisconnect()
    state.value = { status: 'disconnected' }
    currentDevice.value = null
  }

  /** 加载已配对设备列表 (本地存储) */
  async function loadPairedDevices(): Promise<void> {
    try {
      const devices = await invoke<PairedDeviceRaw[]>('list_paired_devices')

      pairedDevices.value = devices.map(d => ({
        id: d.id,
        name: d.device_name,
        address: '', // 需要从其他来源获取
        port: 8765,
        isPaired: true,
      }))
    } catch (error) {
      console.error('Failed to load paired devices:', error)
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

  /** 获取设备名称 */
  function getDeviceName(): string {
    return localStorage.getItem('device_name') || 'Mobile Device'
  }

  // === 清理 ===
  onUnmounted(() => {
    disconnect()
  })

  return {
    // 状态
    state,
    discoveredDevices,
    pairedDevices,
    currentDevice,
    isConnected,
    lastMessage,
    isReady,

    // 方法
    discoverDevices,
    connect,
    requestPairing,
    verifyPairingCode,
    disconnect,
    loadPairedDevices,
    sendMessage,
    sendMessageWithResponse,
    setReconnectCallback,
  }
}
