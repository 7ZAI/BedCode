import { ref, onUnmounted } from 'vue'

export interface WsMessage {
  type: string
  message_id?: string
  session_id?: string
  timestamp: number
  payload: any
  code?: string
}

// Message ID generator for request-response tracking
function generateMessageId(): string {
  return `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`
}

// Pending requests waiting for response
const pendingRequests = new Map<string, {
  resolve: (response: WsMessage) => void
  reject: (error: Error) => void
  timeout: ReturnType<typeof setTimeout>
}>()

// 心跳配置
const HEARTBEAT_INTERVAL_MS = 30000 // 30秒发送一次心跳

// 重连回调类型
type ReconnectCallback = () => Promise<void>

export function useWebSocket() {
  const ws = ref<WebSocket | null>(null)
  const isConnected = ref(false)
  const lastMessage = ref<WsMessage | null>(null)
  const connectionError = ref<string | null>(null)
  const reconnectAttempts = ref(0)
  const maxReconnectAttempts = 5

  let reconnectTimer: ReturnType<typeof setTimeout> | null = null
  let heartbeatTimer: ReturnType<typeof setInterval> | null = null

  // 重连后的回调函数
  let onReconnectCallback: ReconnectCallback | null = null

  // 保存连接参数用于重连
  let connectionParams: { address: string; port: number; secure: boolean } | null = null

  function connect(address: string, port: number = 8765, secure: boolean = false) {
    // Clear any existing connection
    disconnect()

    // 保存连接参数
    connectionParams = { address, port, secure }

    // Use appropriate protocol based on configuration
    // For local development, use ws:// as we don't have TLS certificates
    // For production with reverse proxy (nginx, etc.), use wss://
    const protocol = secure ? 'wss' : 'ws'
    const url = `${protocol}://${address}:${port}`

    try {
      ws.value = new WebSocket(url)

      ws.value.onopen = async () => {
        isConnected.value = true
        connectionError.value = null
        reconnectAttempts.value = 0
        console.log('WebSocket connected to', url)

        // 启动心跳定时器
        startHeartbeat()

        // 如果是重连且有回调，执行重连回调
        if (onReconnectCallback) {
          try {
            await onReconnectCallback()
            console.log('Reconnect callback executed successfully')
          } catch (e) {
            console.error('Reconnect callback failed:', e)
          }
        }
      }

      ws.value.onmessage = (event) => {
        try {
          const message = JSON.parse(event.data) as WsMessage
          lastMessage.value = message

          // Check if this is a response to a pending request
          if (message.message_id && pendingRequests.has(message.message_id)) {
            const pending = pendingRequests.get(message.message_id)!
            pendingRequests.delete(message.message_id)
            clearTimeout(pending.timeout)

            // Check if it's an error response
            if (message.type === 'error') {
              pending.reject(new Error((message.payload as any)?.message || message.code || 'Unknown error'))
            } else {
              pending.resolve(message)
            }
          }

          // Handle waiting input notification
          if (message.type === 'output' && message.payload?.is_waiting) {
            // Trigger notification through custom event
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

        // Auto reconnect if not intentional close
        if (event.code !== 1000 && reconnectAttempts.value < maxReconnectAttempts) {
          scheduleReconnect(address, port, secure)
        }
      }

      ws.value.onerror = (error) => {
        connectionError.value = 'Connection failed'
        console.error('WebSocket error:', error)
      }
    } catch (error) {
      connectionError.value = 'Failed to create WebSocket connection'
      console.error('WebSocket creation error:', error)
    }
  }

  /**
   * 设置重连后的回调函数
   * @param callback 重连后执行的回调
   */
  function setOnReconnect(callback: ReconnectCallback | null) {
    onReconnectCallback = callback
  }

  /**
   * 启动心跳定时器
   */
  function startHeartbeat() {
    stopHeartbeat()
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
  }

  /**
   * 停止心跳定时器
   */
  function stopHeartbeat() {
    if (heartbeatTimer) {
      clearInterval(heartbeatTimer)
      heartbeatTimer = null
    }
  }

  function scheduleReconnect(address: string, port: number, secure: boolean = false) {
    if (reconnectTimer) {
      clearTimeout(reconnectTimer)
    }

    reconnectAttempts.value++
    const delay = Math.min(1000 * Math.pow(2, reconnectAttempts.value), 30000)

    console.log(`Reconnecting in ${delay}ms (attempt ${reconnectAttempts.value}/${maxReconnectAttempts})`)

    reconnectTimer = setTimeout(() => {
      connect(address, port, secure)
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
    connectionParams = null
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

  /**
   * Send a message and wait for a response (request-response pattern)
   * @param type Message type
   * @param payload Message payload
   * @param sessionId Optional session ID
   * @param timeoutMs Timeout in milliseconds (default: 30000)
   * @returns Promise that resolves with the response message
   */
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

      // Set up timeout
      const timeout = setTimeout(() => {
        pendingRequests.delete(messageId)
        reject(new Error(`Request timeout for message ${messageId}`))
      }, timeoutMs)

      // Store pending request
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
      data,
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

  // Cleanup on unmount
  onUnmounted(() => {
    // Clear all pending requests
    for (const [id, pending] of pendingRequests) {
      clearTimeout(pending.timeout)
      pending.reject(new Error('WebSocket disconnected'))
      pendingRequests.delete(id)
    }
    disconnect()
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
