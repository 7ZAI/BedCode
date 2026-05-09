import { ref, watch, onUnmounted } from 'vue'
import type { Ref } from 'vue'

export interface RemoteSession {
  id: string
  name: string
  status: 'running' | 'waiting_input' | 'stopped'
  createdAt: string
  startedAt?: string
}

export interface SessionSummary {
  id: string
  name: string
  status: string
  created_at?: string
  started_at?: string
}

export interface SessionConfigSummary {
  id: string
  name: string
  environment: string
  wsl_distro?: string
  working_dir: string
  command: string
}

export interface UseRemoteConnection {
  state: Ref<{ status: string; error?: string }>
  isConnected: Ref<boolean>
  lastMessage: Ref<{ type: string; payload?: any; session_id?: string } | null>
  sendMessage: (type: string, payload: any, sessionId?: string) => boolean
  sendMessageWithResponse: (type: string, payload: any, sessionId?: string, timeoutMs?: number) => Promise<any>
  setReconnectCallback: (callback: (() => Promise<void>) | null) => void
}

export function useRemoteTerminal(connection: UseRemoteConnection) {
  // === 状态 ===
  const sessions = ref<RemoteSession[]>([])
  const sessionConfigs = ref<SessionConfigSummary[]>([])
  const currentSessionId = ref<string | null>(null)
  // 输出缓冲区：使用数组存储，避免字符串拼接的 O(n) 性能问题
  const outputBuffer = ref('')
  // 待合并的输出块数组（用于批量写入 xterm.js）
  const outputChunks: string[] = []
  // 合并定时器
  let flushTimer: ReturnType<typeof setTimeout> | null = null

  const isWaitingInput = ref(false)
  const isLoading = ref(false)
  const error = ref<string | null>(null)

  // === 输出缓冲区限制 ===
  const MAX_OUTPUT_BYTES = 500000  // 500KB 上限
  const OUTPUT_TRIM_TO = 400000    // 超限后裁剪到 400KB

  // === 定期刷新输出缓冲区到字符串 ===
  function flushOutput() {
    if (outputChunks.length === 0) return

    // 合并所有块
    outputBuffer.value += outputChunks.join('')
    outputChunks.length = 0  // 清空数组

    // 限制缓冲区大小
    if (outputBuffer.value.length > MAX_OUTPUT_BYTES) {
      outputBuffer.value = outputBuffer.value.slice(-OUTPUT_TRIM_TO)
    }
  }

  // 添加输出块（防抖合并，16ms 内合并一次，约 60fps）
  function addOutput(data: string) {
    outputChunks.push(data)

    // 清除之前的定时器，设置新的
    if (flushTimer) clearTimeout(flushTimer)
    flushTimer = setTimeout(flushOutput, 16)
  }

  // === 监听 WebSocket 消息 ===
  watch(
    () => connection.lastMessage.value,
    (message) => {
      if (!message) return

      if (message.type === 'output') {
        handleOutputMessage(message)
      } else if (message.type === 'control') {
        handleControlMessage(message)
      }
    }
  )

  // === 消息处理 ===

  function handleOutputMessage(message: { type: string; payload?: any; session_id?: string }) {
    if (message.session_id !== currentSessionId.value) return

    const payload = message.payload
    if (!payload?.data) return

    // Base64 解码（使用 TextDecoder 支持 UTF-8 多字节字符）
    try {
      const binary = atob(payload.data)
      const bytes = new Uint8Array(binary.length)
      for (let i = 0; i < binary.length; i++) {
        bytes[i] = binary.charCodeAt(i)
      }
      const data = new TextDecoder('utf-8').decode(bytes)

      // 使用优化的输出方法（批量合并）
      addOutput(data)

      // 检测等待输入状态
      isWaitingInput.value = payload.is_waiting || detectWaitingInput(stripAnsi(data))
    } catch (e) {
      console.error('Failed to decode output:', e)
    }
  }

  // 移除 ANSI 转义序列（用于检测等待输入状态）
  function stripAnsi(text: string): string {
    // CSI 序列: \x1b[...字母
    const csiRegex = /\x1b\[[0-9;]*[A-Za-z]/g
    // 其他转义序列
    const otherRegex = /\x1b[^\x1b]*/g
    return text.replace(csiRegex, '').replace(otherRegex, '')
  }

  function handleControlMessage(message: { type: string; payload?: any }) {
    const action = message.payload?.action
    if (!action) return

    if (action.type === 'session_list') {
      sessions.value = action.sessions.map((s: SessionSummary) => ({
        id: s.id,
        name: s.name,
        status: mapSessionStatus(s.status),
        createdAt: s.created_at,
        startedAt: s.started_at || undefined,
      }))
    }
  }

  function mapSessionStatus(status: string): 'running' | 'waiting_input' | 'stopped' {
    switch (status.toLowerCase()) {
      case 'running':
        return 'running'
      case 'waitinginput':
      case 'waiting_input':
        return 'waiting_input'
      case 'stopped':
      case 'error':
        return 'stopped'
      default:
        return 'running'
    }
  }

  function detectWaitingInput(text: string): boolean {
    const patterns = [
      /> $/,           // Claude Code default
      /❯ $/,           // Some shells
      /\?\s*$/,        // Question ending
      /\[Y\/n\]\s*$/,  // Confirmation prompt
      /press any key/i, // Key press prompt
    ]
    return patterns.some(p => p.test(text))
  }

  // === 方法 ===

  /** 获取远程会话列表 */
  async function loadSessions(): Promise<void> {
    if (!connection.isConnected.value) {
      error.value = 'Not connected'
      return
    }

    isLoading.value = true
    error.value = null

    try {
      const response = await connection.sendMessageWithResponse('control', {
        action: { type: 'list_sessions' },
      })

      if (response.payload?.action?.type === 'session_list') {
        sessions.value = response.payload.action.sessions.map((s: SessionSummary) => ({
          id: s.id,
          name: s.name,
          status: mapSessionStatus(s.status),
          createdAt: s.created_at,
          startedAt: s.started_at || undefined,
        }))
      }
    } catch (e) {
      error.value = String(e)
      console.error('Failed to load sessions:', e)
    } finally {
      isLoading.value = false
    }
  }

  /** 获取远程会话配置列表 */
  async function loadSessionConfigs(): Promise<void> {
    if (!connection.isConnected.value) {
      error.value = 'Not connected'
      return
    }

    isLoading.value = true
    error.value = null

    try {
      const response = await connection.sendMessageWithResponse('control', {
        action: { type: 'list_session_configs' },
      })

      if (response.payload?.action?.type === 'session_config_list') {
        sessionConfigs.value = response.payload.action.configs.map((c: SessionConfigSummary) => ({
          id: c.id,
          name: c.name,
          environment: c.environment,
          wsl_distro: c.wsl_distro,
          working_dir: c.working_dir,
          command: c.command,
        }))
      }
    } catch (e) {
      error.value = String(e)
      console.error('Failed to load session configs:', e)
    } finally {
      isLoading.value = false
    }
  }

  /** 启动新会话 */
  async function startSession(configId: string): Promise<string> {
    if (!connection.isConnected.value) {
      throw new Error('Not connected')
    }

    isLoading.value = true
    error.value = null

    try {
      const response = await connection.sendMessageWithResponse('control', {
        action: { type: 'start_session', config_id: configId },
      })

      const sessionId = response.session_id
      if (sessionId) {
        await loadSessions()
        return sessionId
      }

      throw new Error('Failed to start session')
    } catch (e) {
      error.value = String(e)
      throw e
    } finally {
      isLoading.value = false
    }
  }

  /** 停止会话 */
  async function stopSession(sessionId: string): Promise<void> {
    if (!connection.isConnected.value) {
      throw new Error('Not connected')
    }

    try {
      await connection.sendMessageWithResponse('control', {
        action: { type: 'stop_session', session_id: sessionId },
      })

      await loadSessions()
    } catch (e) {
      error.value = String(e)
      throw e
    }
  }

  /** 删除会话 */
  async function removeSession(sessionId: string): Promise<void> {
    if (!connection.isConnected.value) {
      throw new Error('Not connected')
    }

    try {
      await connection.sendMessageWithResponse('control', {
        action: { type: 'remove_session', session_id: sessionId },
      })

      await loadSessions()
    } catch (e) {
      error.value = String(e)
      throw e
    }
  }

  /** 加入会话 (开始接收输出) */
  async function joinSession(sessionId: string): Promise<void> {
    if (!connection.isConnected.value) {
      throw new Error('Not connected')
    }

    // 先离开当前会话
    if (currentSessionId.value) {
      await leaveSession()
    }

    currentSessionId.value = sessionId
    clearOutput()

    try {
      await connection.sendMessageWithResponse('control', {
        action: { type: 'join_session', session_id: sessionId },
      }, sessionId)
    } catch (e) {
      console.error('Failed to join session:', e)
      // 即使失败也保持会话ID，因为可能只是服务器不支持
    }
  }

  /** 离开会话 */
  async function leaveSession(): Promise<void> {
    if (!currentSessionId.value || !connection.isConnected.value) {
      return
    }

    const sessionId = currentSessionId.value
    currentSessionId.value = null

    try {
      await connection.sendMessageWithResponse('control', {
        action: { type: 'leave_session', session_id: sessionId },
      }, sessionId)
    } catch (e) {
      console.error('Failed to leave session:', e)
    }
  }

  /** 发送输入（自动追加换行，与桌面端行为一致） */
  function sendInput(data: string): void {
    if (!currentSessionId.value || !connection.isConnected.value) {
      return
    }

    connection.sendMessage('input', {
      data: data + '\n',
      special_key: null,
    }, currentSessionId.value)
  }

  /** 发送特殊键 */
  function sendSpecialKey(key: string): void {
    if (!currentSessionId.value || !connection.isConnected.value) {
      return
    }

    connection.sendMessage('input', {
      data: '',
      special_key: key,
    }, currentSessionId.value)
  }

  /** 清空输出 */
  function clearOutput(): void {
    // 清除待合并的数据
    if (flushTimer) {
      clearTimeout(flushTimer)
      flushTimer = null
    }
    flushOutput()  // 先刷新 pending 的数据

    outputBuffer.value = ''
    outputChunks.length = 0
    isWaitingInput.value = false
  }

  /** 清理资源（应在组件卸载时调用） */
  function cleanup(): void {
    if (flushTimer) {
      clearTimeout(flushTimer)
      flushTimer = null
    }
    outputChunks.length = 0
  }

  /**
   * 重连后恢复会话订阅
   * 当连接断开后重连成功时自动调用
   */
  async function reconnectAndResume(): Promise<void> {
    if (!connection.isConnected.value) {
      return
    }

    console.log('Reconnecting and resuming session...')

    // 重新加载会话列表
    await loadSessions()

    // 如果之前有订阅的会话，重新订阅
    if (currentSessionId.value) {
      const sessionId = currentSessionId.value
      // 检查会话是否还存在
      const sessionExists = sessions.value.some(s => s.id === sessionId)

      if (sessionExists) {
        try {
          await connection.sendMessageWithResponse('control', {
            action: { type: 'join_session', session_id: sessionId },
          }, sessionId)
          console.log('Successfully rejoined session:', sessionId)
        } catch (e) {
          console.error('Failed to rejoin session:', e)
          currentSessionId.value = null
        }
      } else {
        // 会话不存在了，清除当前会话ID
        console.log('Previous session no longer exists:', sessionId)
        currentSessionId.value = null
      }
    }
  }

  /**
   * 启用自动重连恢复
   * 在连接断开后重连成功时自动恢复会话订阅
   */
  function enableAutoReconnect() {
    connection.setReconnectCallback(reconnectAndResume)
  }

  /**
   * 禁用自动重连恢复
   */
  function disableAutoReconnect() {
    connection.setReconnectCallback(null)
  }

  // === 清理 ===
  onUnmounted(async () => {
    disableAutoReconnect()
    await leaveSession()
  })

  return {
    // 状态
    sessions,
    sessionConfigs,
    currentSessionId,
    outputBuffer,
    isWaitingInput,
    isLoading,
    error,

    // 方法
    loadSessions,
    loadSessionConfigs,
    startSession,
    stopSession,
    removeSession,
    joinSession,
    leaveSession,
    sendInput,
    sendSpecialKey,
    clearOutput,
    cleanup,
    reconnectAndResume,
    enableAutoReconnect,
    disableAutoReconnect,
  }
}
