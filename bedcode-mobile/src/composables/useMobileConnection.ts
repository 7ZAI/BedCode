//! Mobile Connection Composable
//!
//! 移动端连接管理 - 基于 useMobileCommands 的高级封装

import { ref, computed, readonly } from 'vue'
import { logger } from '@/utils/frontendLogger'
import i18n from '@/locales'
import { listen } from '@tauri-apps/api/event'
import { useToast } from '@/composables/useToast'
import { useMobileSettings, defaultMobileSettings } from '@/composables/useMobileSettings'
import { completeStartupTask } from '@/composables/useAppStartup'
import {
  wsConnect,
  wsDisconnect,
  wsIsConnected,
  setAutoReconnect,
  wsAuthenticate,
  wsAuthenticateWithBiometric,
  wsRequestPairing,
  wsVerifyPairingCode,
  wsSetToken,
  initMobileEventListeners,
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
//
// **前端不持有重连循环**（2026-10-04 收敛）。自动重连的唯一执行者是 Rust 侧
// `connection/event_ws.rs` 的 `EventWsSupervisor`，退避节奏由
// `ConnectionManager` 的 `ReconnectManager` 策略决定（指数退避 + 抖动 + 同因
// 熔断 + 1s 下限钳制）。
//
// 收敛前本文件另有一套「MAX 3 次 + 用户固定间隔」的递归重试，与 Rust 侧在同一次
// 断开里**并发触发**：Rust 先赢，前端后到的 `wsReconnect` 撞上 `is_reconnecting`
// 直接 skip —— 但前端计数已经 +1，3 次预算被空转烧掉（审计 P0-2）。
//
// 本变量现在只做一件事：**丢弃过期事件**。用户主动 connect/disconnect 后，
// 上一代连接遗留的 ws_reconnecting / ws_reconnected 不应驱动本代 UI。
let autoReconnectAborted = false

// 意外断开监听器（模块级注册，随连接生命周期存在）

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
  logger.log('[MobileConnection] init() loaded credentials:', savedCreds ? { ...savedCreds, sessionToken: savedCreds.sessionToken ? `length=${savedCreds.sessionToken.length}` : 'missing' } : null)
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
      logger.error('[MobileConnection] wsSetToken failed:', e)
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
      logger.log('[MobileConnection] Connecting...')
    },
    onConnected: () => {
      clearConnectionTimeout()
      connectionStatus.value = 'connected'
      connectionError.value = null
      // 连接成功建立：复位过期事件标记。connect()/disconnect() 置位的 aborted
      // 只用于丢弃上一代连接的 ws_reconnecting / ws_reconnected；不复位则连接
      // 稳定后再次意外断开时，本代真实重连事件会被当成过期的丢掉。
      autoReconnectAborted = false
      logger.log('[MobileConnection] Connected')
      autoStartForegroundService()

      // 连接建立时确保全局监听器启动（订阅在 onPaired 认证成功后执行，
      // 因为桌面端要求先认证才能订阅会话输出）
    },
    onDisconnected: () => {
      clearConnectionTimeout()
      connectionStatus.value = 'disconnected'
      isConnecting.value = false
      logger.log('[MobileConnection] Disconnected')
      autoStopForegroundService()

      // 标记所有 buffer 未订阅（重连后按字节游标重新订阅）
      const bufferStore = useTerminalBufferStore()
      bufferStore.markAllUnsubscribed()
    },
    onPaired: () => {
      clearConnectionTimeout()
      connectionStatus.value = 'paired'
      isConnecting.value = false
      logger.log('[MobileConnection] Paired')

      // 认证成功时更新已配对设备信息
      logger.log('[MobileConnection] onPaired - currentDevice:', currentDevice.value)
      logger.log('[MobileConnection] onPaired - authCredentials:', authCredentials.value ? { fingerprint: authCredentials.value.fingerprint } : null)

      if (currentDevice.value && authCredentials.value) {
        addPairedDevice({
          address: currentDevice.value.address,
          port: currentDevice.value.port,
          name: currentDevice.value.name,
          fingerprint: authCredentials.value.fingerprint,
        })
      } else {
        logger.warn('[MobileConnection] onPaired - missing data, currentDevice:', !!currentDevice.value, 'authCredentials:', !!authCredentials.value)
      }
      autoStartForegroundService()

      // 终端订阅由页面驱动（进入终端页 / 会话恢复运行时 fresh subscribe）：
      // 后台会话不建连、不常拉（票 05 生命周期策略）；断开期间的输出由桌面
      // 环窗口保留，重进终端页时重订阅回放补齐（投递由 TerminalView 的
      // isConnected watch 承担）
    },
    onAuthSuccess: () => {
      logger.log('[MobileConnection] Auth success')
    },
    onAuthFailed: (reason) => {
      connectionStatus.value = 'error'
      connectionError.value = reason
      // 认证失败不断开 isConnecting，startConnection 流程可能继续请求配对
      logger.log('[MobileConnection] Auth failed:', reason)
    },
    onPairingRequest: () => {
      clearConnectionTimeout()
      connectionStatus.value = 'pairing'
      logger.log('[MobileConnection] Pairing requested')
    },
    onPairingVerified: () => {
      logger.log('[MobileConnection] Pairing verified')
    },
    onError: (message) => {
      clearConnectionTimeout()
      connectionError.value = message
      connectionStatus.value = 'error'
      isConnecting.value = false
      logger.error('[MobileConnection] Error:', message)
      autoStopForegroundService()
    },
    onServerClosed: (reason) => {
      clearConnectionTimeout()
      connectionStatus.value = 'disconnected'
      connectionError.value = reason
      isConnecting.value = false
      logger.log('[MobileConnection] Server closed:', reason)
      autoStopForegroundService()
    },
    // 同步事件回调
    onSyncConfigCreated: (data) => {
      logger.log('[MobileConnection] SyncConfigCreated:', data.config.id, 'source:', data.source_device)
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
      logger.log('[MobileConnection] SyncConfigUpdated:', data.config.id, 'source:', data.source_device)
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
      logger.log('[MobileConnection] SyncConfigRemoved:', data.config_id, data.config_name)
      // 从列表移除配置
      sessionConfigs.value = sessionConfigs.value.filter(c => c.id !== data.config_id)
    },
    onSyncSessionCreated: (data) => {
      logger.log('[MobileConnection] SyncSessionCreated:', data.session.id, 'source:', data.source_device)
      // 数量限制：运行中会话已达上限时，增量同步的新运行会话直接丢弃（占位槽位不增加）
      const session = data.session
      const isRunning = session.status === 'running' || session.status === 'waitingInput'
      if (isRunning && runningSessionCount() >= maxOpenTerminalsLimit()) {
        logger.warn(`[MobileConnection] Session limit (${maxOpenTerminalsLimit()}) reached, drop synced session ${session.id}`)
        return
      }
      // 添加新会话到列表
      if (!activeSessions.value.find(s => s.id === session.id)) {
        activeSessions.value.push(session)
      }
    },
    onSyncSessionStatusChanged: (data) => {
      logger.log('[MobileConnection] SyncSessionStatusChanged:', data.session_id, data.old_status, '->', data.new_status)
      // 更新会话状态
      const index = activeSessions.value.findIndex(s => s.id === data.session_id)
      if (index !== -1) {
        activeSessions.value[index].status = data.new_status
      }
      // 会话重新运行：复位 buffer 的 sessionStopped（停止→重启同 id 场景，
      // 不复位则 ws_output 监听器永久丢弃新流帧 → 终端只有旧历史、无实时）。
      // 只处理「未跟踪 / 已停止」的会话：running 广播可能重复且无状态迁移，
      // 对存活 buffer 执行复位会清零订阅信念与游标 → 输入被门控永久拒绝、
      // 历史以 from=0 叠加重播（P0-1 现场：运行中输入无反应 + 格式错乱）
      if (data.new_status === 'running') {
        const bufferStore = useTerminalBufferStore()
        const buffer = bufferStore.getBuffer(data.session_id)
        if (!buffer || buffer.sessionStopped) {
          bufferStore.markSessionRunning(data.session_id)
        }
      }
    },
    onSyncSessionStopped: (data) => {
      logger.log('[MobileConnection] SyncSessionStopped:', data.session_id, data.session_name)
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
      logger.log('[MobileConnection] SyncSessionRemoved:', data.session_id, data.session_name)
      // 从列表移除会话（删除操作才移除）
      activeSessions.value = activeSessions.value.filter(s => s.id !== data.session_id)
      // 取消该会话的任务通知
      cancelTaskNotification(data.session_id)
      // 清理 buffer
      const bufferStore = useTerminalBufferStore()
      bufferStore.clearBuffer(data.session_id)
    },
    onSyncTaskStatusChanged: (data) => {
      logger.log('[MobileConnection] SyncTaskStatusChanged:', data.session_id, data.task_status)
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
  //
  // 三种停止自愈的原因已在 Rust 侧判定并带在 payload 里：
  //   `fatal`         —— 认证类致命 close（4001/4003），需重新配对
  //   `non_retryable` —— 协议/策略层不可重试（1002/1003/1007/1008/1009/1010），
  //                      重连必然同样失败，需升级一端
  //   `auto_reconnect_disabled` —— 用户关掉了自动重连，监督任务不自愈（需手动重连）
  // 其余情况由 `EventWsSupervisor` 自愈，本回调**只负责 UI 与通知**，不发起重连
  // （重连循环已收敛到 Rust 侧，见文件头「重连控制」注释）。
  await listen<{
    reason: string
    fatal?: boolean
    non_retryable?: boolean
    auto_reconnect_disabled?: boolean
  }>('ws_unexpected_disconnect', (event) => {
    logger.warn('[MobileConnection] Unexpected disconnect:', event.payload.reason)
    connectionStatus.value = 'disconnected'
    connectionError.value = 'common.notification.connectionDisconnected'
    isConnecting.value = false
    clearConnectionTimeout()

    // 与 ws_disconnected 路径（onDisconnected）对齐：断连即清理订阅信念——
    // 服务端订阅已随连接关闭清理，若这里不清，重连后的 onPaired 重订阅会
    // 被 subscribed=true 跳过，桌面端新连接无订阅 → 终端只有历史没有实时
    const bufferStore = useTerminalBufferStore()
    bufferStore.markAllUnsubscribed()

    // 弹出 Toast 通知（手动断开不会触发此事件）。此刻不弹「连接已断开」作为
    // 最终态：非致命断开且自动重连开启时，后续会自愈，文案必须指向正在恢复，
    // 否则用户会看到「断开」直到重连成功后才回过神。
    const toast = useToast()

    // 发送连接断开系统通知
    showConnectionNotification({
      type: 'disconnected',
      deviceName: currentDevice.value?.name,
    })

    // 更新前台服务通知为断连状态
    const { updateNotification } = useForegroundService()
    updateNotification()

    // M1/ADR 0031：认证类致命关闭（4001/4003）——自愈重连前需重新配对/认证，
    // 重连无意义。只弹一次「需重新配对」提示（Rust 监督任务已跳过自愈）。
    if (event.payload.fatal) {
      logger.warn('[MobileConnection] Auth-fatal disconnect, need re-pair (no auto-reconnect)')
      toast.error(i18n.global.t('common.notification.authFailedRePair', { reason: event.payload.reason }), 5000)
      return
    }

    // 协议/策略层不可重试：重连必然同样失败，正确动作是升级一端而非重新配对。
    // 文案必须与上面那条分开，否则用户按「重新配对」折腾一圈也解决不了。
    if (event.payload.non_retryable) {
      logger.warn('[MobileConnection] Non-retryable disconnect (protocol/policy), no auto-reconnect')
      toast.error(
        i18n.global.t('common.notification.protocolIncompatible', { reason: event.payload.reason }),
        5000,
      )
      return
    }

    // 用户关闭了自动重连：监督任务已跳过自愈。不告知的话前端只能显示通用的
    // 「连接已断开」，用户分不清「正在自愈」与「不会自愈」，只能干等退避耗尽。
    if (event.payload.auto_reconnect_disabled) {
      logger.warn('[MobileConnection] Auto-reconnect disabled by user, no self-heal will happen')
      toast.error(i18n.global.t('common.notification.autoReconnectDisabled'), 5000)
      return
    }

    // 非致命且可自动重连：断开是事实，但应明确「正在自愈」，避免与后续
    // ws_reconnected/ws_paired 出现语义矛盾。
    toast.warning(i18n.global.t('common.notification.connectionInterrupted', { reason: event.payload.reason }), 5000)

    // 取消所有任务通知
    cancelAllTaskNotifications()
  })

  // 监听重连开始事件
  await listen<{ retry: number; max_retry: number }>('ws_reconnecting', async (event) => {
    logger.log('[MobileConnection] Reconnecting:', event.payload)
    // 如果用户已主动发起新连接，忽略过期重连事件
    if (autoReconnectAborted) {
      logger.log('[MobileConnection] Ignoring reconnect event (aborted)')
      return
    }
    connectionStatus.value = 'connecting'
    connectionError.value = null

    // 更新前台服务通知为重连状态
    const { updateNotification } = useForegroundService()
    await updateNotification()
  })

  // 监听重连成功事件
  // 重连成功后重新认证，认证成功后会触发 ws_paired 事件
  // ws_paired 事件会触发 DevicesView 的 watch，进而调用 loadActiveSessions
  await listen('ws_reconnected', async () => {
    logger.log('[MobileConnection] Reconnected successfully')

    // 如果用户已主动发起新连接，跳过过期重连的认证流程
    if (autoReconnectAborted) {
      logger.log('[MobileConnection] Auto-reconnect aborted, skipping re-auth')
      return
    }

    connectionStatus.value = 'connected'
    connectionError.value = null
    // 重连成功后需要重新认证，isConnecting 保持 true 直到认证完成
    isConnecting.value = true
    const toast = useToast()

    // 重连成功后重新认证
    const creds = loadAuthCredentials()
    if (creds?.sessionToken) {
      try {
        logger.log('[MobileConnection] Re-authenticating with token...')
        const authSuccess = await wsAuthenticate(creds.sessionToken)
        if (authSuccess) {
          logger.log('[MobileConnection] Re-authenticated successfully, ws_paired event should follow')
          toast.success(i18n.global.t('common.notification.reconnected'), 3000)
        } else {
          logger.warn('[MobileConnection] Re-auth failed, need to pair again')
          // JWT 被拒绝，必须断开 WebSocket 连接，否则 Rust 端 WsClient 仍为 Connected
          // 后续用户点击历史连接时 conn.connect() 会误判 "Already connected" 拒绝新建
          try { await wsDisconnect() } catch { /* 忽略断开异常 */ }
          isConnecting.value = false
          connectionStatus.value = 'disconnected'
          connectionError.value = 'mobile.connection.reauthFailed'
          showConnectionNotification({ type: 'auth_failed' })
        }
      } catch (e) {
        logger.error('[MobileConnection] Re-auth error:', e)
        // 认证异常（超时/网络错误），同样断开 WebSocket 保持前后端状态一致
        try { await wsDisconnect() } catch { /* 忽略断开异常 */ }
        isConnecting.value = false
        connectionStatus.value = 'disconnected'
        connectionError.value = 'mobile.connection.reauthError'
      }
    } else {
      logger.log('[MobileConnection] No credentials stored, need manual pairing')
      // 无凭据，断开 WebSocket，用户需要手动发起连接
      try { await wsDisconnect() } catch { /* 忽略断开异常 */ }
      isConnecting.value = false
      connectionStatus.value = 'disconnected'
      connectionError.value = 'mobile.connection.noCredentials'
    }
  })

  // 监听事件通道就绪（票 03）：session-control 极简认证首帧发出后 Rust 发射
  // 事件不重放：重连/自愈重建期间的变化只能靠这一次 HTTP 全量拉取补齐；
  // 消费端按 id 去重 / 状态收敛（如 onSyncSessionCreated 的 `!find` 守卫），无需额外去重
  await listen('ws_event_channel_ready', () => {
    logger.log('[MobileConnection] Event channel ready, reconciling active sessions')
    loadActiveSessions().catch((e) => {
      logger.error('[MobileConnection] Reconcile on event channel ready failed:', e)
    })
  })

  // 凭证被桌面端**永久**拒绝（ADR 0033 §F3 / 票 08）
  //
  // 与 `ws_reconnect_failed` 刻意**分开**：后者是「退避重试耗尽」（网络类），
  // 本者是一次就判定的「凭据不认」（入场密钥换手 / 设备被撤销 / 从未配对）——
  // 两者排障路径完全不同，重试无用且正确动作是**重新配对**。
  // 混成一个终态正是本事件存在的理由：迁移前两者都落到「重连失败」。
  await listen<{ reason: string }>('ws_reauth_rejected', (event) => {
    logger.error('[MobileConnection] Credential permanently rejected (needs re-pair):', event.payload.reason)
    clearConnectionTimeout()
    connectionStatus.value = 'error'
    isConnecting.value = false
    autoStopForegroundService()

    const toast = useToast()
    // 复用已有文案 key（与 WS 致命关闭 4001/4003 同一条）：明确指向「重新配对」
    toast.error(i18n.global.t('common.notification.authFailedRePair', { reason: event.payload.reason }), 5000)

    // 后台运行时的系统通知也用「需重新配对」而非「重连失败」
    showConnectionNotification({
      type: 'auth_failed',
      deviceName: currentDevice.value?.name,
    })
  })

  // 监听重连失败事件
  await listen<{ reason: string }>('ws_reconnect_failed', (event) => {
    logger.error('[MobileConnection] Reconnect failed:', event.payload.reason)
    connectionStatus.value = 'disconnected'
    connectionError.value = 'common.notification.connectionDisconnected'
    isConnecting.value = false

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

  // 开屏启动任务打点:监听器与凭据恢复完成即视为连接子系统就绪
  // (不等 WS 实际建连——建连由用户操作驱动,不属于启动期)
  //
  // 自动重连开关同步到 Rust 连接层（与设置页 onMounted 共用同一函数，双入口
  // 幂等）。Rust 侧的 flag 不会随 localStorage 自动恢复，不同步则重启后
  // 用户关掉的开关会悄悄变回开启。
  await syncAutoReconnectSetting()
  completeStartupTask('connection')
}

// 模块加载时立即初始化，确保事件监听尽早注册
init()

// ==================== Operations ====================

/**
 * 连接到设备
 */
export async function connect(device: RemoteDevice): Promise<void> {
  logger.log('[MobileConnection] Starting connection to:', device.address, device.port)

  // 丢弃上一代连接遗留的重连事件
  autoReconnectAborted = true

  // 无论前端 connectionStatus 状态如何，始终先断开 Rust 端可能残留的旧连接
  // 被动断开后前端状态可能是 'disconnected'，但 Rust 端 WsClient 可能仍为 Connected
  // （比如自动重连成功但 JWT 认证失败时，前端认为断开了，Rust 端连接仍活着）
  // 不断开旧连接会导致 Rust conn.connect() 误判 "Already connected" 而拒绝新建连接
  try {
    const alreadyConnected = await wsIsConnected()
    if (alreadyConnected) {
      logger.log('[MobileConnection] Disconnecting stale Rust-side connection before new connect')
      await wsDisconnect()
    }
  } catch (e) {
    // wsIsConnected / wsDisconnect 失败不影响后续连接
    logger.warn('[MobileConnection] Pre-connect cleanup failed (expected if no connection):', e)
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
      logger.warn('[MobileConnection] Connection timeout')
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
    logger.log('[MobileConnection] Probing desktop reachability...')
    const probeResult = await httpProbe(device.address, device.port)
    if (!probeResult.reachable) {
      clearConnectionTimeout()
      logger.warn('[MobileConnection] Desktop unreachable:', probeResult.error)
      connectionError.value = 'mobile.connection.unreachable'
      connectionStatus.value = 'error'
      isConnecting.value = false
      throw new Error('mobile.connection.unreachable')
    }
    logger.log('[MobileConnection] Desktop reachable, proceeding to WS connect')

    // 调用后端连接，状态由后端事件驱动更新
    const result = await wsConnect(device.address, device.port, device.name)
    logger.log('[MobileConnection] wsConnect returned:', result)
  } catch (error) {
    clearConnectionTimeout()
    logger.error('[MobileConnection] wsConnect failed:', error)
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
    logger.log('[MobileConnection] Cancelling connection...')
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
  let settings: Record<string, unknown> = {}
  if (savedSettings) {
    try {
      settings = JSON.parse(savedSettings)
    } catch {
      settings = {} // 损坏的本地设置按缺省处理，不阻断启动
    }
  }
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
 * 把「自动重连」设置项同步到 Rust 连接层
 *
 * 自动重连的执行者是 `EventWsSupervisor`（唯一自愈入口），退避节奏由
 * `ConnectionManager` 的 `ReconnectManager` 策略决定。前端不持有重连循环，
 * 只把用户的开关意图递过去——策略归连接层，UI 只管意图。
 */
export async function syncAutoReconnectSetting(): Promise<void> {
  try {
    const { settings, loadSettings } = useMobileSettings()
    // 必须先 loadSettings：未加载时 settings 还是默认值（开启），直接读会把
    // 用户已经关掉的开关又推回 Rust。loadSettings 幂等，已加载时直接返回。
    await loadSettings()
    await setAutoReconnect(settings.value.autoReconnect)
    logger.log('[MobileConnection] Auto-reconnect setting synced:', settings.value.autoReconnect)
  } catch (error) {
    // 同步失败不阻断连接流程：Rust 侧默认开启，重启后仍能自愈
    logger.error('[MobileConnection] Failed to sync auto-reconnect setting:', error)
  }
}

/**
 * 断开连接
 */
export async function disconnect(): Promise<void> {
  // 丢弃上一代连接遗留的重连事件
  autoReconnectAborted = true
  clearConnectionTimeout()
  try {
    await wsDisconnect()
    // 断开连接时取消所有任务通知
    const { cancelAllTaskNotifications } = useNotification()
    await cancelAllTaskNotifications()
  } catch (e) {
    // wsDisconnect 可能因无活跃连接而失败，确保前端状态仍被重置
    logger.warn('[MobileConnection] wsDisconnect failed (expected if no active connection):', e)
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
  logger.log('[MobileConnection] authenticate() called')
  logger.log('[MobileConnection]   authCredentials.value =', authCredentials.value)
  logger.log('[MobileConnection]   localStorage auth_session_token =', localStorage.getItem('auth_session_token'))
  logger.log('[MobileConnection]   localStorage auth_pairing_id =', localStorage.getItem('auth_pairing_id'))
  logger.log('[MobileConnection]   localStorage auth_fingerprint =', localStorage.getItem('auth_fingerprint'))

  if (!authCredentials.value?.sessionToken) {
    logger.log('[MobileConnection] No stored credentials, skipping auth -> false')
    return false
  }

  logger.log('[MobileConnection] Attempting JWT re-auth, token length:', authCredentials.value.sessionToken.length)
  try {
    const result = await wsAuthenticate(authCredentials.value.sessionToken)
    logger.log('[MobileConnection] Auth result:', result)
    if (!result) {
      // 服务端明确拒绝（JWT 过期或无效），清除凭据需要重新配对
      clearAuthCredentials()
      authCredentials.value = null
    }
    return result
  } catch (error) {
    // 网络错误/超时，不删除 token — Rust 端有 30 秒超时兜底
    // 下次重连仍可复用，避免因临时网络问题导致必须重新配对
    logger.error('[MobileConnection] Auth error (not clearing token):', error)
    return false
  }
}

/**
 * 请求配对
 */
export async function requestPairing(): Promise<void> {
  logger.log('[MobileConnection] requestPairing: calling wsRequestPairing (invoke)...')
  await wsRequestPairing()
  logger.log('[MobileConnection] requestPairing: wsRequestPairing returned')
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
    logger.error('[MobileConnection] Pairing verification failed:', error)
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
    logger.warn('[MobileConnection] Failed to load session configs via HTTP:', result.message)
    return []
  } catch (e: any) {
    logger.error('[MobileConnection] loadSessionConfigs error:', e?.message || e)
    return []
  }
}

// ==================== 终端数量限制（外观设置 maxOpenTerminals） ====================

/** 当前占用槽位的运行中会话数（running / waitingInput 计入，stopped 为历史记录不占槽位） */
function runningSessionCount(): number {
  return activeSessions.value.filter(s => s.status === 'running' || s.status === 'waitingInput').length
}

/** 读取「最大可打开终端数量」设置（1-20，非法值回退默认） */
function maxOpenTerminalsLimit(): number {
  const v = Number(useMobileSettings().settings.value.maxOpenTerminals)
  return Number.isFinite(v) && v > 0
    ? Math.min(20, Math.max(1, Math.round(v)))
    : defaultMobileSettings.maxOpenTerminals
}

/**
 * 加载活跃会话列表（同步桌面端）
 *
 * 数量限制：只同步前 N 个运行中的会话，超出的运行会话丢弃（stopped 会话不占槽位全保留），
 * 有丢弃时 toast 通知用户
 */
export async function loadActiveSessions(): Promise<any[]> {
  const { httpListSessions } = useHttpApi()
  const result = await httpListSessions()
  if (result.code === 0 && result.data) {
    const sessions = result.data.sessions || []
    const limit = maxOpenTerminalsLimit()

    // 保持原列表顺序，仅丢弃超出上限的「运行中」会话
    const kept: any[] = []
    let runningKept = 0
    for (const s of sessions) {
      const isRunning = s.status === 'running' || s.status === 'waitingInput'
      if (isRunning && runningKept >= limit) continue
      if (isRunning) runningKept++
      kept.push(s)
    }

    const dropped = sessions.length - kept.length
    if (dropped > 0) {
      const toast = useToast()
      toast.warning(i18n.global.t('mobile.session.maxSyncLimited', { max: limit, dropped }))
    }

    activeSessions.value = kept
    return kept
  }
  logger.warn('[MobileConnection] Failed to load sessions via HTTP:', result.message)
  return []
}

/**
 * 启动会话，返回完整会话信息
 *
 * size：本端终端组件按设备屏幕预算的默认网格（computeDeviceDefaultGridSize），
 * 随请求传给主机，PTY 以该尺寸创建；同一设备/朝向下数值稳定
 */
export async function startSession(
  configId: string,
  size?: { cols: number; rows: number }
): Promise<{ sessionId: string; session?: any }> {
  // 数量限制：运行中会话已达上限时拒绝启动（错误由调用方 toast 提示）
  const limit = maxOpenTerminalsLimit()
  if (runningSessionCount() >= limit) {
    throw new Error(i18n.global.t('mobile.session.maxReached', { max: limit }))
  }

  const { httpStartSession } = useHttpApi()
  const result = await httpStartSession(configId, size)
  if (result.code === 0 && result.data) {
    const sessionId = result.data.sessionId
    // 终端订阅由页面驱动（进入终端页时 fresh subscribe 回放环窗口）：
    // 不在会话启动时建连（后台常拉违例；输出由桌面环窗口保留，重播覆盖历史）
    return { sessionId, session: undefined }
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
    logger.log('[MobileConnection] Updated paired device:', device.fingerprint,
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
    logger.log('[MobileConnection] Added new paired device:', device.fingerprint, 'address:', fullAddress)
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