import { ref, watch, onUnmounted } from 'vue'
import type { Ref } from 'vue'
import { useSettingsStore } from '@/stores/settings'
import { useBackgroundMonitor } from './useBackgroundMonitor'
import { useAndroidFeatures } from './useAndroidFeatures'

export interface RemoteSession {
  id: string
  name: string
  status: 'running' | 'waiting_input' | 'stopped'
  createdAt: string
  startedAt?: string
  sessionType?: 'pty' | 'plugin'
}

export interface SessionSummary {
  id: string
  name: string
  status: string
  created_at?: string
  started_at?: string
  session_type?: string
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
  addDisconnectCallback: (callback: () => void) => void
}

// Singleton state — shared across all useRemoteTerminal() calls so session data
// survives Vue component navigation. When user navigates from TerminalView to
// DevicesView, the session data remains available.
const sessions = ref<RemoteSession[]>([])
const sessionConfigs = ref<SessionConfigSummary[]>([])
const currentSessionId = ref<string | null>(null)
const isWaitingInput = ref(false)
const isLoading = ref(false)
const error = ref<string | null>(null)

// 已加入的会话订阅（用于后台接收数据）
const joinedSessions = ref<Set<string>>(new Set())
// 当前活跃显示的会话（用于控制台输出显示）
const activeSessionId = ref<string | null>(null)

export function useRemoteTerminal(connection: UseRemoteConnection) {
  // 输出缓冲区：使用数组存储，避免字符串拼接的 O(n) 性能问题
  const outputBuffer = ref('')
  // 待合并的输出块数组（用于批量写入 xterm.js）
  const outputChunks: string[] = []
  // 合并定时器
  let flushTimer: ReturnType<typeof setTimeout> | null = null

  // === 输出缓冲区限制（按字节计算） ===
  const MAX_OUTPUT_BYTES = 500000   // 500KB 上限
  const OUTPUT_TRIM_TO = 400000     // 超限后裁剪到 400KB

  // 计算字符串的字节长度
  function getByteLength(str: string): number {
    return new Blob([str]).size
  }

  // 截断字符串到指定字节长度
  function sliceByByte(str: string, maxBytes: number): string {
    const encoder = new TextEncoder()
    const bytes = encoder.encode(str)
    if (bytes.length <= maxBytes) return str
    return new TextDecoder().decode(bytes.slice(0, maxBytes))
  }

  // === 定期刷新输出缓冲区到字符串 ===
  function flushOutput() {
    if (outputChunks.length === 0) return

    // 合并所有块
    outputBuffer.value += outputChunks.join('')
    outputChunks.length = 0  // 清空数组

    // 按字节限制缓冲区大小
    const currentBytes = getByteLength(outputBuffer.value)
    if (currentBytes > MAX_OUTPUT_BYTES) {
      outputBuffer.value = sliceByByte(outputBuffer.value, OUTPUT_TRIM_TO)
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
    const payload = message.payload
    if (!payload?.data) return

    // 检查是否已订阅（而不是是否活跃）
    const sessionId = message.session_id
    if (!sessionId) return
    const isSubscribed = joinedSessions.value.has(sessionId)
    if (!isSubscribed) return  // 未订阅的会话，完全忽略

    // Base64 解码（使用 TextDecoder 支持 UTF-8 多字节字符）
    let data: string
    try {
      const binary = atob(payload.data)
      const bytes = new Uint8Array(binary.length)
      for (let i = 0; i < binary.length; i++) {
        bytes[i] = binary.charCodeAt(i)
      }
      data = new TextDecoder('utf-8').decode(bytes)
    } catch (e) {
      console.error('Failed to decode output:', e)
      return
    }

    // 关键改动���数据始终写入缓冲区（无论是否活跃）
    addOutput(data)

    // 只在活跃会话时才写入终端显示并更新等待输入状态
    if (message.session_id !== activeSessionId.value) return

    // 检测等待输入状态
    isWaitingInput.value = payload.is_waiting || detectWaitingInput(stripAnsi(data))

    // 检测重要输出并发送通知（仅在后台时）
    checkAndNotify(sessionId, data, isWaitingInput.value)
  }

  // 移除 ANSI 转义序列（用于检测等待输入状态）
  function stripAnsi(text: string): string {
    // CSI 序列: \x1b[...字母 (如 \x1b[32m 颜色)
    const csiRegex = /\x1b\[[0-9;]*[A-Za-z]/g
    // OSC 序列: \x1b]...BEL (如 \x1b]0;...\x07 标题)
    const oscRegex = /\x1b\][^\x07]*\x07/g
    // 单独 ESC 字符
    const escRegex = /\x1b/g
    return text.replace(csiRegex, '').replace(oscRegex, '').replace(escRegex, '')
  }

  // 检测重要输出并发送通知
  function checkAndNotify(sessionId: string, data: string, isWaiting: boolean) {
    const settingsStore = useSettingsStore()

    // 检查是否启用后台通知
    if (settingsStore.settings.ui.notify_in_background !== true) return

    const { isInBackground } = useBackgroundMonitor()
    if (!isInBackground.value) return  // 只有在后台才发送通知

    const { sendSessionNotification } = useAndroidFeatures()
    const sessionName = getSessionName(sessionId)

    // 检测重要输出类型
    if (isWaiting) {
      sendSessionNotification(sessionName, '等待交互', sessionId)
      return
    }

    // 检测错误输出
    if (/\b(error|Error|failed|Failed|✗|Failed to)\b/i.test(data)) {
      sendSessionNotification(sessionName, '命令执行出错', sessionId)
      return
    }

    // 检测交互式提示
    if (/\[\s*[YynN]\s*\]|\?\s*$|select.*:|^[0-9]+\)/i.test(data)) {
      sendSessionNotification(sessionName, '等待交互', sessionId)
      return
    }
  }

  // 获取会话名称
  function getSessionName(sessionId: string): string {
    const session = sessions.value.find(s => s.id === sessionId)
    return session?.name || '终端会话'
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
        sessionType: (s.session_type || 'pty') as 'pty' | 'plugin',
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

    // 检查是否已认证
    if (connection.state.value.status !== 'paired') {
      error.value = 'Not authenticated'
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
          sessionType: (s.session_type || 'pty') as 'pty' | 'plugin',
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

    // 检查是否已认证
    if (connection.state.value.status !== 'paired') {
      error.value = 'Not authenticated'
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
  async function joinSession(sessionId: string, forceResubscribe: boolean = false): Promise<void> {
    if (!connection.isConnected.value) {
      throw new Error('Not connected')
    }

    // 检查是否已订阅，避免重复订阅
    // forceResubscribe 为 true 时，强制重新订阅（用于重连后恢复）
    const isAlreadySubscribed = joinedSessions.value.has(sessionId)
    const isNewSubscription = !isAlreadySubscribed || forceResubscribe

    if (isNewSubscription) {
      // 获取最大缓存数量限制
      const settingsStore = useSettingsStore()
      const maxCached = settingsStore.getMaxCachedTerminals()
      const currentCount = joinedSessions.value.size

      // 如果已达到限制，不再创建新的订阅
      if (currentCount >= maxCached && !forceResubscribe) {
        throw new Error(`已达到最大终端数量限制 (${maxCached})，请先关闭其他终端页面`)
      }

      try {
        await connection.sendMessageWithResponse('control', {
          action: { type: 'join_session', session_id: sessionId },
        }, sessionId)
        // 订阅成功，添加到已订阅集合
        joinedSessions.value.add(sessionId)
        console.log(`Joined session: ${sessionId}, forceResubscribe: ${forceResubscribe}`)
      } catch (e) {
        console.error('Failed to join session:', e)
        // 即使失败也尝试添加，因为后端可能不支持 join_session
        joinedSessions.value.add(sessionId)
      }
    }

    // 设置为当前活跃会话（控制输出显示）
    currentSessionId.value = sessionId
    activeSessionId.value = sessionId
    // 只有首次订阅时才清空输出，重新加入已订阅的会话保留历史输出
    if (isNewSubscription) {
      clearOutput()
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

  /** 直接发送键盘输入（不添加换行符，用于直接输入模式） */
  function sendKeyboardInput(data: string): void {
    if (!currentSessionId.value || !connection.isConnected.value) {
      return
    }

    connection.sendMessage('input', {
      data: data,
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

  /** 断开连接时清除所有会话相关状态 */
  function clearAllSessionState(): void {
    // 清除会话列表
    sessions.value = []
    // 清除会话配置
    sessionConfigs.value = []
    // 清除订阅状态
    joinedSessions.value.clear()
    // 清除当前活跃会话
    currentSessionId.value = null
    activeSessionId.value = null
    // 清除输出缓冲区
    clearOutput()

    console.log('All session state cleared')
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
   * 只有在认证成功（paired 状态）时才加载数据
   */
  async function reconnectAndResume(): Promise<void> {
    if (!connection.isConnected.value) {
      return
    }

    // 检查是否已认证，只有已认证状态才加载数据
    if (connection.state.value.status !== 'paired') {
      console.log('Not authenticated, skipping session reload')
      return
    }

    console.log('Reconnecting and resuming session...')

    // 重新加载会话列表和配置
    await Promise.all([
      loadSessions(),
      loadSessionConfigs(),
    ])

    // 如果之前有订阅的会话，重新订阅（强制重新订阅，因为后端已清除订阅状态）
    if (currentSessionId.value) {
      const sessionId = currentSessionId.value
      // 检查会��是否还存在
      const sessionExists = sessions.value.some(s => s.id === sessionId)

      if (sessionExists) {
        try {
          // 使用 forceResubscribe = true 强制重新订阅
          // 因为 WebSocket 重连后，后端的订阅状态已丢失
          await joinSession(sessionId, true)
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

  /**
   * 离开会话并清理资源
   * 当确定不再需要该会话时调用（如导航到其他页面）
   */
  async function destroy(): Promise<void> {
    disableAutoReconnect()
    await leaveSession()
    cleanup()
    clearOutput()
  }

  /** 设置当前活跃会话（切换显示的终端内容） */
  function setActiveSession(sessionId: string | null): void {
    // 只有切换到另一个会话时才清空输出缓冲区
    // 设为 null（退出）时保留历史输出，供下次进入时恢复显示
    if (sessionId !== null) {
      clearOutput()
    }
    activeSessionId.value = sessionId
    currentSessionId.value = sessionId
  }

  /** 离开会话（取消订阅） */
  async function unsubscribeSession(sessionId: string): Promise<void> {
    if (!connection.isConnected.value) {
      return
    }

    // 从已订阅集合中移除
    joinedSessions.value.delete(sessionId)

    // 如��是当前活跃会话，清除活跃状态
    if (activeSessionId.value === sessionId) {
      activeSessionId.value = null
      currentSessionId.value = null
      clearOutput()
    }

    try {
      await connection.sendMessageWithResponse('control', {
        action: { type: 'leave_session', session_id: sessionId },
      }, sessionId)
    } catch (e) {
      console.error('Failed to unsubscribe session:', e)
    }
  }

  // 移除自动 onUnmounted 清理
  // 调用者负责在适当时机调用 destroy()

  return {
    // 状态
    sessions,
    sessionConfigs,
    currentSessionId,
    activeSessionId,
    joinedSessions,
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
    sendKeyboardInput,
    sendSpecialKey,
    clearOutput,
    clearAllSessionState,
    cleanup,
    destroy,
    reconnectAndResume,
    enableAutoReconnect,
    disableAutoReconnect,
    setActiveSession,
    unsubscribeSession,
  }
}
