import { ref, onUnmounted } from 'vue'

export interface WsMessage {
  type: string
  message_id?: string
  session_id?: string
  timestamp: number
  payload: any
  code?: string
}

function generateMessageId(): string {
  return `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`
}

// 心跳配置
const HEARTBEAT_INTERVAL_MS = 30000 // 30秒发送一次心跳
const HEARTBEAT_TIMEOUT_MS = 90000   // 90秒无消息视为断连

// 重连回调类型
type ReconnectCallback = () => Promise<void>

// Singleton state — shared across all useWebSocket() calls so the WebSocket
// connection survives Vue component mount/unmount cycles during route navigation.
const ws = ref<WebSocket | null>(null)
const isConnected = ref(false)
const lastMessage = ref<WsMessage | null>(null)
const connectionError = ref<string | null>(null)
const reconnectAttempts = ref(0)
const maxReconnectAttempts = 5

let reconnectTimer: ReturnType<typeof setTimeout> | null = null
let heartbeatTimer: ReturnType<typeof setInterval> | null = null
let heartbeatTimeoutTimer: ReturnType<typeof setTimeout> | null = null

let onReconnectCallback: ReconnectCallback | null = null
let connectionParams: { address: string; port: number; secure: boolean } | null = null

const pendingRequests = new Map<string, {
  resolve: (response: WsMessage) => void
  reject: (error: Error) => void
  timeout: ReturnType<typeof setTimeout>
}>()

let lastMessageTime: number = 0

// Track how many components are using the connection
let usageCount = 0

export function useWebSocket() {

  function connect(address: string, port: number = 8765, secure: boolean = false) {
    // Clear any existing connection
    disconnect()

    // 保存连接参数
    connectionParams = { address, port, secure }

    const protocol = secure ? 'wss' : 'ws'
    const url = `${protocol}://${address}:${port}`

    try {
      ws.value = new WebSocket(url)

      ws.value.onopen = async () => {
        isConnected.value = true
        connectionError.value = null
        lastMessageTime = Date.now()
        console.log('WebSocket connected to', url)

        // 启动心跳定时器
        startHeartbeat()

        // 只有在重连成功时才执行回调（reconnectAttempts > 0 表示这是重连）
        // 首次连接不执行回调，由调用方处理认证和数据加载
        if (reconnectAttempts.value > 0 && onReconnectCallback) {
          try {
            await onReconnectCallback()
            console.log('Reconnect callback executed successfully')
          } catch (e) {
            console.error('Reconnect callback failed:', e)
          }
        }

        // 重连成功后重置计数（保持在这里以确保状态正确）
        reconnectAttempts.value = 0
      }

      ws.value.onmessage = (event) => {
        lastMessageTime = Date.now()
        try {
          const message = JSON.parse(event.data) as WsMessage
          lastMessage.value = message

          // 心跳响应，无需进一步处理
          if (message.type === 'heartbeat') return

          // Handle server closed notification (desktop app shutting down)
          if (message.type === 'server_closed') {
            const reason = (message as any).reason || 'Server closed'
            const willReconnect = (message as any).will_reconnect ?? false
            console.warn('[WebSocket] Server closed:', reason, 'willReconnect:', willReconnect)

            // Dispatch event for UI to handle
            window.dispatchEvent(new CustomEvent('server-closed', {
              detail: { reason, willReconnect }
            }))

            // 标记为断开连接状态
            isConnected.value = false
            // 不自动重连，因为桌面端已经退出
            return
          }

          // Handle application-level errors (even without message_id)
          if (message.type === 'error') {
            const errMsg = (message as any).message || message.code || 'Unknown error'
            console.error('[WebSocket] Server error:', errMsg)

            // Try to match to a pending request by message_id
            if (message.message_id && pendingRequests.has(message.message_id)) {
              const pending = pendingRequests.get(message.message_id)!
              pendingRequests.delete(message.message_id)
              clearTimeout(pending.timeout)
              pending.reject(new Error(errMsg))
            }
            return
          }

          // Check if this is a response to a pending request
          if (message.message_id && pendingRequests.has(message.message_id)) {
            const pending = pendingRequests.get(message.message_id)!
            pendingRequests.delete(message.message_id)
            clearTimeout(pending.timeout)
            pending.resolve(message)
          }

          // Handle waiting input notification
          if (message.type === 'output' && message.payload?.is_waiting) {
            window.dispatchEvent(new CustomEvent('claude-waiting-input', {
              detail: message
            }))
          }
        } catch (e) {
          console.error('Failed to parse WebSocket message:', e)
        }
      }

      ws.value.onclose = (event) => {
        isConnected.value = false
        console.log('WebSocket closed:', event.code, event.reason)

        // 停止心跳
        stopHeartbeat()

        // 非主动关闭时自动重连，使用 connectionParams
        if (event.code !== 1000 && reconnectAttempts.value < maxReconnectAttempts && connectionParams) {
          scheduleReconnect()
        } else if (reconnectAttempts.value >= maxReconnectAttempts) {
          // 重连次数超过最大值，通知用户连接失败
          const wsUrl = connectionParams
            ? `${connectionParams.secure ? 'wss' : 'ws'}://${connectionParams.address}:${connectionParams.port}`
            : 'unknown'
          connectionError.value = `连接失败，已达到最大重试次数。请检查：\n1. 桌面端是否已启动\n2. 设备是否在同一网络下\n3. 防火墙是否阻止了连接`
        }
      }

      ws.value.onerror = (error) => {
        // 收集更多诊断信息
        const wsUrl = connectionParams
          ? `${connectionParams.secure ? 'wss' : 'ws'}://${connectionParams.address}:${connectionParams.port}`
          : 'unknown'
        connectionError.value = `无法连接到 ${wsUrl}，请检查：\n1. 桌面端是否已启动\n2. 设备是否在同一网络下\n3. 防火墙是否阻止了连接`
        console.error('WebSocket error:', error, 'Target:', wsUrl)
      }
    } catch (error) {
      connectionError.value = 'Failed to create WebSocket connection'
      console.error('WebSocket creation error:', error)
    }
  }

  function setOnReconnect(callback: ReconnectCallback | null) {
    onReconnectCallback = callback
  }

  function startHeartbeat() {
    stopHeartbeat()
    lastMessageTime = Date.now()

    // 定时发送心跳
    heartbeatTimer = setInterval(() => {
      if (ws.value && isConnected.value) {
        const heartbeat = {
          type: 'heartbeat',
          timestamp: Date.now()
        }
        try {
          ws.value.send(JSON.stringify(heartbeat))
        } catch (e) {
          console.error('Failed to send heartbeat:', e)
        }
      }
    }, HEARTBEAT_INTERVAL_MS)

    // 心跳超时检测：每 10 秒检查一次，超过 90 秒无消息则断开
    checkHeartbeatTimeout()
  }

  function checkHeartbeatTimeout() {
    if (heartbeatTimeoutTimer) {
      clearInterval(heartbeatTimeoutTimer)
    }
    // 每 10 秒检查一次，而不是一次性延迟
    heartbeatTimeoutTimer = setInterval(() => {
      if (isConnected.value) {
        const elapsed = Date.now() - lastMessageTime
        if (elapsed > HEARTBEAT_TIMEOUT_MS) {
          console.warn(`No message received for ${elapsed}ms, treating as disconnected`)
          ws.value?.close(3001, 'Heartbeat timeout')
        }
      }
    }, 10000)
  }

  function stopHeartbeat() {
    if (heartbeatTimer) {
      clearInterval(heartbeatTimer)
      heartbeatTimer = null
    }
    if (heartbeatTimeoutTimer) {
      clearInterval(heartbeatTimeoutTimer)
      heartbeatTimeoutTimer = null
    }
  }

  function scheduleReconnect() {
    if (reconnectTimer) {
      clearTimeout(reconnectTimer)
    }

    reconnectAttempts.value++
    const delay = Math.min(1000 * Math.pow(2, reconnectAttempts.value), 30000)

    console.log(`Reconnecting in ${delay}ms (attempt ${reconnectAttempts.value}/${maxReconnectAttempts})`)

    reconnectTimer = setTimeout(() => {
      if (connectionParams) {
        connect(connectionParams.address, connectionParams.port, connectionParams.secure)
      }
    }, delay)
  }

  function disconnect() {
    stopHeartbeat()

    if (reconnectTimer) {
      clearTimeout(reconnectTimer)
      reconnectTimer = null
    }

    if (ws.value) {
      ws.value.close(1000, 'User disconnect')
      ws.value = null
    }

    isConnected.value = false
    reconnectAttempts.value = 0
    // 清除错误状态
    connectionError.value = null
    // 只清除连接参数，保留 pendingRequests 以允许多次 disconnect/connect
  }

  function sendMessage(type: string, payload: any, sessionId?: string): boolean {
    if (!ws.value || !isConnected.value) {
      console.warn('WebSocket not connected')
      return false
    }

    const message: WsMessage = {
      type,
      message_id: generateMessageId(),
      session_id: sessionId,
      timestamp: Date.now(),
      payload
    }

    try {
      ws.value.send(JSON.stringify(message))
      return true
    } catch (error) {
      console.error('Failed to send message:', error)
      return false
    }
  }

  function sendMessageWithResponse(
    type: string,
    payload: any,
    sessionId?: string,
    timeoutMs: number = 30000
  ): Promise<WsMessage> {
    return new Promise((resolve, reject) => {
      if (!ws.value || !isConnected.value) {
        reject(new Error('WebSocket not connected'))
        return
      }

      const messageId = generateMessageId()
      const message: WsMessage = {
        type,
        message_id: messageId,
        session_id: sessionId,
        timestamp: Date.now(),
        payload
      }

      const timeout = setTimeout(() => {
        pendingRequests.delete(messageId)
        reject(new Error(`Request timeout for message ${messageId}`))
      }, timeoutMs)

      pendingRequests.set(messageId, { resolve, reject, timeout })

      try {
        ws.value.send(JSON.stringify(message))
      } catch (error) {
        pendingRequests.delete(messageId)
        clearTimeout(timeout)
        reject(error)
      }
    })
  }

  function sendInput(data: string, sessionId: string, specialKey?: string) {
    return sendMessage('input', {
      data: data + '\n',
      special_key: specialKey || null
    }, sessionId)
  }

  function sendSpecialKey(key: string, sessionId: string) {
    return sendMessage('input', {
      data: '',
      special_key: key
    }, sessionId)
  }

  function resize(cols: number, rows: number, sessionId: string) {
    return sendMessage('control', {
      action: { type: 'resize_session', session_id: sessionId, cols, rows }
    }, sessionId)
  }

  // Track usage for singleton lifecycle
  usageCount++

  // Cleanup on unmount — WebSocket 是 singleton，组件卸载时不断开连接
  // 只清理引用，避免内存泄漏
  onUnmounted(() => {
    usageCount--
    if (usageCount <= 0) {
      usageCount = 0
      // 不调用 disconnect()，保持 WebSocket 连接供其他组件使用
      // pendingRequests 保留，因为连接仍然活跃
    }
  })

  return {
    ws,
    isConnected,
    lastMessage,
    connectionError,
    reconnectAttempts,
    connect,
    disconnect,
    sendMessage,
    sendMessageWithResponse,
    sendInput,
    sendSpecialKey,
    resize,
    setOnReconnect,
  }
}
