//! Mobile Connection Composable
//!
//! 移动端连接管理 - 基于 useMobileCommands 的高级封装

import { ref, computed, readonly } from 'vue'
import i18n from '@/locales'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useToast } from '@/composables/useToast'
import {
  wsConnect,
  wsDisconnect,
  wsGetStatus,
  wsIsConnected,
  wsReconnect,
  wsAuthenticate,
  wsAuthenticateWithBiometric,
  wsRequestPairing,
  wsVerifyPairingCode,
  wsSetToken,
  initMobileEventListeners,
  cleanupMobileEventListeners,
  saveAuthCredentials,
  loadAuthCredentials,
  clearAuthCredentials,
  type ConnectionStatus,
  type RemoteDevice,
  type AuthCredentials,
} from './useMobileCommands'
import { useHttpApi, httpSendSessionInput, httpProbe } from './useHttpApi'
import { useForegroundService } from './useForegroundService'
import { useNotification } from './useNotification'
import { useTerminalBufferStore } from '@/stores/terminalBuffer'

// Re-export types
export type { ConnectionStatus, RemoteDevice, AuthCredentials } from './useMobileCommands'

// ==================== State ====================

const connectionStatus = ref<ConnectionStatus>('disconnected')
const currentDevice = ref<RemoteDevice | null>(null)
const connectionError = ref<string | null>(null)
const isConnecting = ref(false)

// 连接超时控制
let connectionTimeout: ReturnType<typeof setTimeout> | null = null
const CONNECTION_TIMEOUT_MS = 12000 // 12秒超时（比 Rust 端 10 秒稍长作为兜底）

// 重连控制
const MAX_AUTO_RECONNECT_ATTEMPTS = 3
let autoReconnectAttemptCount = 0
// 用户主动连接/断开时设为 true，取消正在进行的自动重连
let autoReconnectAborted = false

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

// 已配对设备列表（全局状态，认证成功后记录）
// 与连接历史不同：连接历史记录所有尝试连接的设备，已配对设备只记录成功认证的设备
export interface PairedDevice {
  address: string
  port: number
  name: string
  fingerprint: string  // 设备指纹，用于识别同一设备
  pairedAt: string     // 配对时间
  lastConnected: string // 最后连接时间
  connectCount: number  // 连接次数统计
}
const pairedDevices = ref<PairedDevice[]>([])
const pairedDevicesLoaded = ref(false)

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

  // 恢复 Rust 侧全局 token（GLOBAL_TOKEN 为内存态，进程重启后为空）：
  // 插件对桌面端文件服务（/api/plugins/*）的 HTTP 调用依赖它作为 JWT；
  // JWT 重连响应经 RequestResponseManager 消费不会触发 AuthHandler 补写，
  // 只能在此显式恢复，否则文件服务请求永远无 Authorization 头（桌面端 401）
  if (savedCreds?.sessionToken) {
    try {
      await wsSetToken(savedCreds.sessionToken)
    } catch (e) {
      console.error('[MobileConnection] wsSetToken failed:', e)
    }
  }

  // 加载已配对设备列表
  loadPairedDevices()

  // DEV 模式 UI 审查 mock：localStorage 开关 mock_connected=1 时注入已连接状态，
  // 配合 public/mock-harness.html 纯前端审查使用；生产构建 DEV=false 自动移除
  if (import.meta.env.DEV && localStorage.getItem('mock_connected') === '1') {
    currentDevice.value = {
      id: 'mock-device',
      name: 'DESKTOP-7ZAI',
      address: '192.168.1.100',
      port: 8765,
      isPaired: true,
      fingerprint: 'mock-fingerprint',
    }
    connectionStatus.value = 'connected'
  }

  // 初始化通知
  const { showTaskNotification, cancelTaskNotification, cancelAllTaskNotifications, showConnectionNotification } = useNotification()

  // 初始化事件监听 - 状态由后端事件驱动
  await initMobileEventListeners({
    onConnecting: () => {
      connectionStatus.value = 'connecting'
      connectionError.value = null
      console.log('[MobileConnection] Connecting...')
    },
    onConnected: () => {
      clearConnectionTimeout()
      connectionStatus.value = 'connected'
      connectionError.value = null
      console.log('[MobileConnection] Connected')
      autoStartForegroundService()

      // 连接建立时确保全局监听器启动（订阅在 onPaired 认证成功后执行，
      // 因为桌面端要求先认证才能订阅会话输出）
      const bufferStore = useTerminalBufferStore()
      bufferStore.startGlobalListener().catch((e) => {
        console.warn('[MobileConnection] Global listener start failed:', e)
      })
    },
    onDisconnected: () => {
      clearConnectionTimeout()
      connectionStatus.value = 'disconnected'
      isConnecting.value = false
      console.log('[MobileConnection] Disconnected')
      autoStopForegroundService()

      // 标记所有 buffer 未订阅（重连后按字节游标重新订阅）
      const bufferStore = useTerminalBufferStore()
      bufferStore.markAllUnsubscribed()
    },
    onPaired: () => {
      clearConnectionTimeout()
      connectionStatus.value = 'paired'
      isConnecting.value = false
      // 配对/认证成功，重置自动重连计数
      autoReconnectAttemptCount = 0
      console.log('[MobileConnection] Paired')

      // 认证成功时更新已配对设备信息
      console.log('[MobileConnection] onPaired - currentDevice:', currentDevice.value)
      console.log('[MobileConnection] onPaired - authCredentials:', authCredentials.value ? { fingerprint: authCredentials.value.fingerprint } : null)

      if (currentDevice.value && authCredentials.value) {
        addPairedDevice({
          address: currentDevice.value.address,
          port: currentDevice.value.port,
          name: currentDevice.value.name,
          fingerprint: authCredentials.value.fingerprint,
        })
      } else {
        console.warn('[MobileConnection] onPaired - missing data, currentDevice:', !!currentDevice.value, 'authCredentials:', !!authCredentials.value)
      }
      autoStartForegroundService()

      // 认证成功后重新订阅所有后台会话的终端输出
      // 必须在 onPaired 而非 onConnected 中执行，因为桌面端要求先认证才能订阅
      const bufferStore = useTerminalBufferStore()
      bufferStore.startGlobalListener().catch((e) => {
        console.warn('[MobileConnection] Global listener start failed:', e)
      })
      for (const [sid, buffer] of bufferStore.buffers.entries()) {
        // 无条件重订阅（仅跳过已停止会话）：服务端订阅随连接关闭清理，
        // subscribed 只是前端信念且可能残留（意外断开路径已由
        // markAllUnsubscribed 兜底，但重订阅本身幂等——桌面端按
        // (client_id, session_id) 替换订阅者，cursor 续传无重复帧）
        if (buffer.sessionStopped) continue
        bufferStore.subscribeSession(sid).catch((e) => {
          console.warn(`[useMobileConnection] Resubscribe ${sid} failed:`, e)
        })
      }
    },
    onAuthSuccess: () => {
      console.log('[MobileConnection] Auth success')
    },
    onAuthFailed: (reason) => {
      connectionStatus.value = 'error'
      connectionError.value = reason
      // 认证失败不断开 isConnecting，startConnection 流程可能继续请求配对
      console.log('[MobileConnection] Auth failed:', reason)
    },
    onPairingRequest: () => {
      clearConnectionTimeout()
      connectionStatus.value = 'pairing'
      console.log('[MobileConnection] Pairing requested')
    },
    onPairingVerified: () => {
      console.log('[MobileConnection] Pairing verified')
    },
    onError: (message) => {
      clearConnectionTimeout()
      connectionError.value = message
      connectionStatus.value = 'error'
      isConnecting.value = false
      console.error('[MobileConnection] Error:', message)
      autoStopForegroundService()
    },
    onServerClosed: (reason) => {
      clearConnectionTimeout()
      connectionStatus.value = 'disconnected'
      connectionError.value = reason
      isConnecting.value = false
      console.log('[MobileConnection] Server closed:', reason)
      autoStopForegroundService()
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
      // 会话重新运行：复位 buffer 的 sessionStopped（停止→重启同 id 场景，
      // 不复位则 ws_output 监听器永久丢弃新流帧 → 终端只有旧历史、无实时）
      if (data.new_status === 'running') {
        const bufferStore = useTerminalBufferStore()
        bufferStore.markSessionRunning(data.session_id)
      }
    },
    onSyncSessionStopped: (data) => {
      console.log('[MobileConnection] SyncSessionStopped:', data.session_id, data.session_name)
      // 更新会话状态为 stopped，而不是移除（保留记录显示灰色）
      const index = activeSessions.value.findIndex(s => s.id === data.session_id)
      if (index !== -1) {
        activeSessions.value[index].status = 'stopped'
      }
      // 取消该会话的任务通知
      cancelTaskNotification(data.session_id)
      // 标记 buffer 会话停止
      const bufferStore = useTerminalBufferStore()
      bufferStore.markSessionStopped(data.session_id)
    },
    onSyncSessionRemoved: (data) => {
      console.log('[MobileConnection] SyncSessionRemoved:', data.session_id, data.session_name)
      // 从列表移除会话（删除操作才移除）
      activeSessions.value = activeSessions.value.filter(s => s.id !== data.session_id)
      // 取消该会话的任务通知
      cancelTaskNotification(data.session_id)
      // 清理 buffer
      const bufferStore = useTerminalBufferStore()
      bufferStore.clearBuffer(data.session_id)
    },
    onSyncTaskStatusChanged: (data) => {
      console.log('[MobileConnection] SyncTaskStatusChanged:', data.session_id, data.task_status)
      // 更新对应会话的任务状态
      const index = activeSessions.value.findIndex(s => s.id === data.session_id)
      if (index !== -1) {
        activeSessions.value[index].taskStatus = data.task_status
        activeSessions.value[index].taskReason = data.task_reason ?? null
      }
      // 发送任务通知
      const session = activeSessions.value.find(s => s.id === data.session_id)
      showTaskNotification({
        sessionId: data.session_id,
        sessionName: session?.name || data.session_id.slice(0, 8),
        taskStatus: data.task_status,
        taskReason: data.task_reason ?? undefined,
      })
    },
  })

  // 监听意外断开事件（Rust 端 WsClient 检测到异常断开时发射）
  unlistenUnexpectedDisconnect = await listen<{ reason: string }>('ws_unexpected_disconnect', (event) => {
    console.warn('[MobileConnection] Unexpected disconnect:', event.payload.reason)
    connectionStatus.value = 'disconnected'
    connectionError.value = 'common.notification.connectionDisconnected'
    isConnecting.value = false
    clearConnectionTimeout()

    // 与 ws_disconnected 路径（onDisconnected）对齐：断连即清理订阅信念——
    // 服务端订阅已随连接关闭清理，若这里不清，重连后的 onPaired 重订阅会
    // 被 subscribed=true 跳过，桌面端新连接无订阅 → 终端只有历史没有实时
    const bufferStore = useTerminalBufferStore()
    bufferStore.markAllUnsubscribed()

    // 弹出 Toast 通知（手动断开不会触发此事件）
    const toast = useToast()
    toast.error(i18n.global.t('common.notification.connectionDisconnected', { reason: event.payload.reason }), 5000)

    // 发送连接断开系统通知
    showConnectionNotification({
      type: 'disconnected',
      deviceName: currentDevice.value?.name,
    })

    // 更新前台服务通知为断连状态
    const { updateNotification } = useForegroundService()
    updateNotification()

    // 取消所有任务通知
    cancelAllTaskNotifications()

    // 异步触发重连（不在监听回调中直接 await，避免阻塞事件循环）
    handleUnexpectedDisconnect(event.payload.reason)
  })

  // 监听重连开始事件
  await listen<{ retry: number; max_retry: number }>('ws_reconnecting', async (event) => {
    console.log('[MobileConnection] Reconnecting:', event.payload)
    // 如果用户已主动发起新连接，忽略过期重连事件
    if (autoReconnectAborted) {
      console.log('[MobileConnection] Ignoring reconnect event (aborted)')
      return
    }
    connectionStatus.value = 'connecting'

    // 更新前台服务通知为重连状态
    const { updateNotification } = useForegroundService()
    await updateNotification()
  })

  // 监听重连成功事件
  // 重连成功后重新认证，认证成功后会触发 ws_paired 事件
  // ws_paired 事件会触发 DevicesView 的 watch，进而调用 loadActiveSessions
  await listen('ws_reconnected', async () => {
    console.log('[MobileConnection] Reconnected successfully')

    // 如果用户已主动发起新连接，跳过过期重连的认证流程
    if (autoReconnectAborted) {
      console.log('[MobileConnection] Auto-reconnect aborted, skipping re-auth')
      return
    }

    connectionStatus.value = 'connected'
    connectionError.value = null
    // 重连成功后需要重新认证，isConnecting 保持 true 直到认证完成
    isConnecting.value = true

    // 重连成功，重置自动重连计数（认证成功时 onPaired 也会重置）
    autoReconnectAttemptCount = 0

    // 重连成功后重新认证
    const creds = loadAuthCredentials()
    if (creds?.sessionToken) {
      try {
        console.log('[MobileConnection] Re-authenticating with token...')
        const authSuccess = await wsAuthenticate(creds.sessionToken)
        if (authSuccess) {
          console.log('[MobileConnection] Re-authenticated successfully, ws_paired event should follow')
        } else {
          console.warn('[MobileConnection] Re-auth failed, need to pair again')
          // JWT 被拒绝，必须断开 WebSocket 连接，否则 Rust 端 WsClient 仍为 Connected
          // 后续用户点击历史连接时 conn.connect() 会误判 "Already connected" 拒绝新建
          try { await wsDisconnect() } catch (_) {}
          isConnecting.value = false
          connectionStatus.value = 'disconnected'
          connectionError.value = 'mobile.connection.reauthFailed'
          showConnectionNotification({ type: 'auth_failed' })
        }
      } catch (e) {
        console.error('[MobileConnection] Re-auth error:', e)
        // 认证异常（超时/网络错误），同样断开 WebSocket 保持前后端状态一致
        try { await wsDisconnect() } catch (_) {}
        isConnecting.value = false
        connectionStatus.value = 'disconnected'
        connectionError.value = 'mobile.connection.reauthError'
      }
    } else {
      console.log('[MobileConnection] No credentials stored, need manual pairing')
      // 无凭据，断开 WebSocket，用户需要手动发起连接
      try { await wsDisconnect() } catch (_) {}
      isConnecting.value = false
      connectionStatus.value = 'disconnected'
      connectionError.value = 'mobile.connection.noCredentials'
    }
  })

  // 监听重连失败事件
  await listen<{ reason: string }>('ws_reconnect_failed', (event) => {
    console.error('[MobileConnection] Reconnect failed:', event.payload.reason)
    connectionStatus.value = 'disconnected'
    connectionError.value = 'common.notification.connectionDisconnected'
    isConnecting.value = false
    autoReconnectAttemptCount = MAX_AUTO_RECONNECT_ATTEMPTS // 标记重连已耗尽

    // 重连失败时停止前台服务
    autoStopForegroundService()

    // 发送重连失败系统通知
    showConnectionNotification({
      type: 'reconnect_failed',
      deviceName: currentDevice.value?.name,
      reason: event.payload.reason,
    })

    const toast = useToast()
    toast.error(i18n.global.t('common.notification.reconnectFailed', { reason: event.payload.reason }), 5000)
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

  // 取消正在进行的自动重连
  autoReconnectAborted = true
  autoReconnectAttemptCount = MAX_AUTO_RECONNECT_ATTEMPTS

  // 无论前端 connectionStatus 状态如何，始终先断开 Rust 端可能残留的旧连接
  // 被动断开后前端状态可能是 'disconnected'，但 Rust 端 WsClient 可能仍为 Connected
  // （比如自动重连成功但 JWT 认证失败时，前端认为断开了，Rust 端连接仍活着）
  // 不断开旧连接会导致 Rust conn.connect() 误判 "Already connected" 而拒绝新建连接
  try {
    const alreadyConnected = await wsIsConnected()
    if (alreadyConnected) {
      console.log('[MobileConnection] Disconnecting stale Rust-side connection before new connect')
      await wsDisconnect()
    }
  } catch (e) {
    // wsIsConnected / wsDisconnect 失败不影响后续连接
    console.warn('[MobileConnection] Pre-connect cleanup failed (expected if no connection):', e)
  }
  // 确保前端状态也重置干净，无论之前是什么状态
  connectionStatus.value = 'disconnected'
  isConnecting.value = false
  const bufferStore = useTerminalBufferStore()
  bufferStore.markAllUnsubscribed()

  currentDevice.value = device
  connectionError.value = null
  isConnecting.value = true
  clearConnectionTimeout()
  connectionTimeout = setTimeout(async () => {
    if (isConnecting.value && connectionStatus.value === 'connecting') {
      console.warn('[MobileConnection] Connection timeout')
      connectionError.value = 'mobile.connection.timeoutToast'
      connectionStatus.value = 'error'
      isConnecting.value = false

      const toast = useToast()
      toast.error(i18n.global.t('mobile.connection.timeout'))
    }
  }, CONNECTION_TIMEOUT_MS)

  try {
    // 设置 HTTP API 基础 URL
    const { setApiBaseUrl } = useHttpApi()
    setApiBaseUrl(device.address, device.port)

    // HTTP 探测桌面端是否可达（3 秒超时，快速判断网络连通性）
    console.log('[MobileConnection] Probing desktop reachability...')
    const probeResult = await httpProbe(device.address, device.port)
    if (!probeResult.reachable) {
      clearConnectionTimeout()
      console.warn('[MobileConnection] Desktop unreachable:', probeResult.error)
      connectionError.value = 'mobile.connection.unreachable'
      connectionStatus.value = 'error'
      isConnecting.value = false
      throw new Error('mobile.connection.unreachable')
    }
    console.log('[MobileConnection] Desktop reachable, proceeding to WS connect')

    // 调用后端连接，状态由后端事件驱动更新
    const result = await wsConnect(device.address, device.port, device.name)
    console.log('[MobileConnection] wsConnect returned:', result)
  } catch (error) {
    clearConnectionTimeout()
    console.error('[MobileConnection] wsConnect failed:', error)
    connectionStatus.value = 'error'
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
    connectionError.value = 'mobile.connection.userCancelled'
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
 * 读取 keepAlive 设置，如果开启则启动前台服务
 */
async function autoStartForegroundService() {
  const savedSettings = localStorage.getItem('mobile-settings')
  const settings = savedSettings ? JSON.parse(savedSettings) : {}
  if (settings.keepAlive) {
    const { startService } = useForegroundService()
    await startService()
  }
}

/**
 * 停止前台服务
 */
async function autoStopForegroundService() {
  const { stopService } = useForegroundService()
  await stopService()
}

/**
 * 处理意外断开，尝试重连
 * 最多自动重连 MAX_AUTO_RECONNECT_ATTEMPTS 次，超出后放弃并保持 disconnected 状态
 */
async function handleUnexpectedDisconnect(reason: string) {
  console.log('[MobileConnection] Handling unexpected disconnect, reason:', reason, 'attempt:', autoReconnectAttemptCount + 1, '/', MAX_AUTO_RECONNECT_ATTEMPTS)

  // 用户已主动发起新连接或断开，取消自动重连
  if (autoReconnectAborted) {
    console.log('[MobileConnection] Auto-reconnect aborted by user action')
    return
  }

  // 读取用户设置
  const savedSettings = localStorage.getItem('mobile-settings')
  const settings = savedSettings
    ? JSON.parse(savedSettings)
    : { autoReconnect: true, reconnectInterval: 5 }

  // 检查是否启用自动重连
  if (!settings.autoReconnect) {
    console.log('[MobileConnection] Auto-reconnect disabled by user setting')
    return
  }

  // 检查重连次数限制
  if (autoReconnectAttemptCount >= MAX_AUTO_RECONNECT_ATTEMPTS) {
    console.warn('[MobileConnection] Max auto-reconnect attempts reached, giving up')
    connectionStatus.value = 'disconnected'
    connectionError.value = 'common.notification.reconnectAbandoned'
    const toast = useToast()
    toast.error(i18n.global.t('common.notification.reconnectAbandoned'), 5000)
    return
  }

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

  autoReconnectAttemptCount++
  console.log('[MobileConnection] Starting reconnect attempt', autoReconnectAttemptCount, 'token length:', creds.sessionToken.length)
  // 开始自动重连前重置取消标记
  autoReconnectAborted = false
  isConnecting.value = true
  connectionStatus.value = 'connecting'
  connectionError.value = null

  try {
    // 使用用户设置的重连间隔
    if (settings.reconnectInterval > 0) {
      await new Promise(resolve => setTimeout(resolve, settings.reconnectInterval * 1000))
      // 等待期间用户可能已发起新连接，检查取消标记
      if (autoReconnectAborted) {
        console.log('[MobileConnection] Auto-reconnect aborted during delay wait')
        return
      }
    }
    await wsReconnect(creds.sessionToken)
    console.log('[MobileConnection] Reconnect initiated successfully')
  } catch (error) {
    console.error('[MobileConnection] Reconnect failed:', error)
    connectionStatus.value = 'disconnected'
    connectionError.value = 'mobile.connection.reconnectFailedMsg'
    isConnecting.value = false

    // 重连失败后，如果还有重试次数且未被用户取消，继续尝试
    if (autoReconnectAttemptCount < MAX_AUTO_RECONNECT_ATTEMPTS && !autoReconnectAborted) {
      handleUnexpectedDisconnect(reason)
    }
  }
}

/**
 * 断开连接
 */
export async function disconnect(): Promise<void> {
  // 取消正在进行的自动重连
  autoReconnectAborted = true
  autoReconnectAttemptCount = MAX_AUTO_RECONNECT_ATTEMPTS
  clearConnectionTimeout()
  try {
    await wsDisconnect()
    // 断开连接时取消所有任务通知
    const { cancelAllTaskNotifications } = useNotification()
    await cancelAllTaskNotifications()
  } catch (e) {
    // wsDisconnect 可能因无活跃连接而失败，确保前端状态仍被重置
    console.warn('[MobileConnection] wsDisconnect failed (expected if no active connection):', e)
  } finally {
    // 无论后端是否成功断开，前端状态必须重置
    connectionStatus.value = 'disconnected'
    isConnecting.value = false
    currentDevice.value = null

    // 手动断开不触发 Rust 端 ws_disconnected 事件，需显式重置 buffer 订阅状态
    const bufferStore = useTerminalBufferStore()
    bufferStore.markAllUnsubscribed()
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
    const result = await wsAuthenticate(authCredentials.value.sessionToken)
    console.log('[MobileConnection] Auth result:', result)
    if (!result) {
      // 服务端明确拒绝（JWT 过期或无效），清除凭据需要重新配对
      clearAuthCredentials()
      authCredentials.value = null
    }
    return result
  } catch (error) {
    // 网络错误/超时，不删除 token — Rust 端有 30 秒超时兜底
    // 下次重连仍可复用，避免因临时网络问题导致必须重新配对
    console.error('[MobileConnection] Auth error (not clearing token):', error)
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
      // onPaired 回调会保存已配对设备信息
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
 * 生物认证登录（挑战-应答握手），成功后保存凭据
 *
 * 失败时抛出错误（透传桌面端拒绝原因如 CREDENTIAL_NOT_BOUND），
 * 由调用方决定展示具体文案；不再吞掉错误以免用户只看到笼统提示。
 */
export async function authenticateWithBiometric(): Promise<boolean> {
  const creds = await wsAuthenticateWithBiometric()
  if (creds) {
    saveCredentials(creds)
    return true
  }
  return false
}

/**
 * 加载会话配置列表
 */
export async function loadSessionConfigs(): Promise<any[]> {
  const { httpListConfigs } = useHttpApi()
  try {
    const result = await httpListConfigs()
    if (result.code === 0 && result.data) {
      const configs = result.data.configs || []
      sessionConfigs.value = configs.map((c: any) => ({
        id: c.id,
        name: c.name,
        environment: c.environment,
        wsl_distro: c.wslDistro,
        working_dir: c.workingDir,
        command: c.command,
      }))
      hasLoadedConfigs.value = true
      return configs
    }
    console.warn('[MobileConnection] Failed to load session configs via HTTP:', result.message)
    return []
  } catch (e: any) {
    console.error('[MobileConnection] loadSessionConfigs error:', e?.message || e)
    return []
  }
}

/**
 * 加载活跃会话列表
 */
export async function loadActiveSessions(): Promise<any[]> {
  const { httpListSessions } = useHttpApi()
  const result = await httpListSessions()
  if (result.code === 0 && result.data) {
    activeSessions.value = result.data.sessions || []
    return result.data.sessions
  }
  console.warn('[MobileConnection] Failed to load sessions via HTTP:', result.message)
  return []
}

/**
 * 启动会话，返回完整会话信息
 */
export async function startSession(configId: string, sessionName?: string): Promise<{ sessionId: string; session?: any }> {
  const { httpStartSession } = useHttpApi()
  const result = await httpStartSession(configId)
  if (result.code === 0 && result.data) {
    return { sessionId: result.data.sessionId, session: undefined }
  }
  throw new Error(result.message || 'Failed to start session')
}

/**
 * 停止会话：通过 HTTP API 发送停止请求，成功后更新本地状态
 */
export async function stopSession(sessionId: string): Promise<void> {
  const { httpStopSession } = useHttpApi()
  const result = await httpStopSession(sessionId)
  if (result.code === 0) {
    const index = activeSessions.value.findIndex(s => s.id === sessionId)
    if (index !== -1) {
      activeSessions.value[index].status = 'stopped'
    }
  } else {
    throw new Error(result.message || 'Failed to stop session')
  }
}

/**
 * 删除会话：通过 HTTP API 发送删除请求，成功后更新本地状态
 */
export async function removeSession(sessionId: string): Promise<void> {
  const { httpRemoveSession } = useHttpApi()
  const result = await httpRemoveSession(sessionId)
  if (result.code === 0) {
    activeSessions.value = activeSessions.value.filter(s => s.id !== sessionId)
  } else {
    throw new Error(result.message || 'Failed to remove session')
  }
}

/**
 * 加载连接历史
 * @param force - 强制从 localStorage 重新读取（页面切换回来时需要，因为其他页面可能直接修改了 localStorage）
 */
export function loadConnectionHistory(force: boolean = false): void {
  if (historyLoaded.value && !force) return
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

// ==================== Paired Devices Management ====================

/**
 * 加载已配对设备列表
 */
export function loadPairedDevices(): void {
  if (pairedDevicesLoaded.value) return
  const stored = localStorage.getItem('paired_devices')
  if (stored) {
    try {
      pairedDevices.value = JSON.parse(stored)
    } catch {
      pairedDevices.value = []
    }
  }
  pairedDevicesLoaded.value = true
}

/**
 * 保存已配对设备列表到 localStorage
 */
function savePairedDevices(): void {
  localStorage.setItem('paired_devices', JSON.stringify(pairedDevices.value))
}

/**
 * 添加或更新已配对设备
 * 使用设备指纹作为唯一标识，同一设备只记录一次
 */
export function addPairedDevice(device: { address: string; port: number; name: string; fingerprint: string }): void {
  const fullAddress = `${device.address}:${device.port}`
  const now = new Date().toISOString()

  // 查找是否已存在相同指纹的设备（同一设备）
  const existingIndex = pairedDevices.value.findIndex(d => d.fingerprint === device.fingerprint)

  if (existingIndex !== -1) {
    // 已存在，更新信息并增加连接次数
    const existing = pairedDevices.value[existingIndex]
    pairedDevices.value[existingIndex] = {
      ...existing,
      address: device.address,
      port: device.port,
      name: device.name,
      lastConnected: now,
      connectCount: existing.connectCount + 1,
    }
    console.log('[MobileConnection] Updated paired device:', device.fingerprint,
      'new address:', fullAddress, 'connectCount:', existing.connectCount + 1)
  } else {
    // 新设备，添加到列表开头，初始连接次数为 1
    pairedDevices.value.unshift({
      address: device.address,
      port: device.port,
      name: device.name,
      fingerprint: device.fingerprint,
      pairedAt: now,
      lastConnected: now,
      connectCount: 1,
    })
    console.log('[MobileConnection] Added new paired device:', device.fingerprint, 'address:', fullAddress)
  }

  // 限制最多保存 10 个设备
  if (pairedDevices.value.length > 10) {
    pairedDevices.value = pairedDevices.value.slice(0, 10)
  }

  savePairedDevices()
}

/**
 * 从已配对设备列表移除
 */
export function removePairedDevice(fingerprint: string): void {
  pairedDevices.value = pairedDevices.value.filter(d => d.fingerprint !== fingerprint)
  savePairedDevices()
}

/**
 * 清除所有已配对设备
 */
export function clearPairedDevices(): void {
  pairedDevices.value = []
  savePairedDevices()
}

/**
 * 根据指纹查找已配对设备
 */
export function findPairedDeviceByFingerprint(fingerprint: string): PairedDevice | undefined {
  return pairedDevices.value.find(d => d.fingerprint === fingerprint)
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
 * 发送输入到会话（通过 HTTP API，绕过 WebSocket 阻塞）
 */
export async function sendInput(sessionId: string, data: string, specialKey?: string): Promise<void> {
  const result = await httpSendSessionInput(sessionId, data, specialKey)
  if (result.code !== 0) {
    throw new Error(result.message || 'Send input failed')
  }
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
    pairedDevices,
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
    authenticateWithBiometric,
    requestPairing,
    verifyPairingCode,
    loadSessionConfigs,
    loadActiveSessions,
    startSession,
    stopSession,
    removeSession,
    sendInput,
    saveCredentials,
    clearCredentials,

    // Connection History Operations
    loadConnectionHistory,
    saveConnectionHistory,
    addToConnectionHistory,
    removeFromConnectionHistory,
    clearConnectionHistory,

    // Paired Devices Operations
    loadPairedDevices,
    addPairedDevice,
    removePairedDevice,
    clearPairedDevices,
    findPairedDeviceByFingerprint,

    // Clear Operations
    clearSessionConfigs,
    clearActiveSessions,
  }
}