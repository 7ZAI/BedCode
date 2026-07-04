/**
 * Server Management Composable
 *
 * 服务器状态管理、生命周期控制和指标轮询
 */
import { ref, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import i18n from '@/locales'

/** 服务器状态 */
export type ServerStatus = 'stopped' | 'starting' | 'running'

/** 服务器状态信息 */
export interface ServerStatusInfo {
  status: ServerStatus
  port: number
  auto_start: boolean
  local_ips: string[]
}

/** 网络配置 — Actix Web + WebSocket 参数 */
export interface NetworkConfig {
  port: number
  auto_start: boolean
  prevent_sleep: boolean
  workers: number
  keep_alive_secs: number
  client_request_timeout_secs: number
  client_disconnect_timeout_secs: number
  max_connections: number
  backlog: number
  tcp_nodelay: boolean
  shutdown_timeout_secs: number
  ws_max_frame_size_kb: number
  ws_max_message_size_mb: number
}

/** 服务器性能指标 */
export interface ServerMetrics {
  uptime_secs: number
  connections: number
  total_http_requests: number
  http_requests_per_sec: number
  ws_messages_sent: number
  ws_messages_received: number
  ws_sent_rate: number
  ws_recv_rate: number
  cpu_usage_percent: number
  memory_usage_bytes: number
}

/** 带时间戳的指标（ECharts 时序图用） */
export interface TimestampedMetrics {
  timestamp_secs: number
  ws_sent_rate: number
  ws_recv_rate: number
}

const MAX_HISTORY = 60

export function useServer() {
  const status = ref<ServerStatus>('stopped')
  const port = ref(8765)
  const autoStart = ref(true)
  const localIps = ref<string[]>([])
  const metrics = ref<ServerMetrics | null>(null)
  const metricsHistory = ref<TimestampedMetrics[]>([])
  const loading = ref(false)
  const networkConfig = ref<NetworkConfig | null>(null)

  let pollTimer: ReturnType<typeof setInterval> | null = null

  /** 加载服务器状态 */
  async function loadStatus() {
    try {
      const info: ServerStatusInfo = await invoke('get_server_status')
      status.value = info.status
      port.value = info.port
      autoStart.value = info.auto_start
      localIps.value = info.local_ips
    } catch (e) {
      console.error('Failed to load server status:', e)
    }
  }

  /** 启动服务器 */
  async function startServer() {
    loading.value = true
    try {
      await invoke('server_start', { port: port.value })
      status.value = 'running'
    } catch (e: any) {
      throw new Error(i18n.global.t('desktop.server.startFailed', { error: e }))
    } finally {
      loading.value = false
    }
  }

  /** 停止服务器 */
  async function stopServer() {
    loading.value = true
    try {
      await invoke('server_stop')
      status.value = 'stopped'
    } catch (e: any) {
      throw new Error(i18n.global.t('desktop.server.stopFailed', { error: e }))
    } finally {
      loading.value = false
    }
  }

  /** 重启服务器 */
  async function restartServer() {
    loading.value = true
    try {
      await invoke('server_restart')
      status.value = 'running'
    } catch (e: any) {
      throw new Error(i18n.global.t('desktop.server.restartFailed', { error: e }))
    } finally {
      loading.value = false
    }
  }

  /** 更新端口配置 */
  async function updatePort(newPort: number) {
    await invoke('update_server_port', { port: newPort })
    port.value = newPort
  }

  /** 更新自启动配置 */
  async function updateAutoStart(value: boolean) {
    await invoke('update_server_auto_start', { autoStart: value })
    autoStart.value = value
  }

  /** 加载网络配置 */
  async function loadNetworkConfig() {
    try {
      const config: NetworkConfig = await invoke('get_server_network_config')
      networkConfig.value = config
      port.value = config.port
      autoStart.value = config.auto_start
    } catch (e) {
      console.error('Failed to load network config:', e)
    }
  }

  /** 更新网络配置（需重启生效） */
  async function updateNetworkConfig(config: NetworkConfig) {
    await invoke('update_server_network_config', { networkConfig: config })
    networkConfig.value = { ...config }
    port.value = config.port
    autoStart.value = config.auto_start
  }

  /** 重置网络配置为默认值（需重启生效） */
  async function resetNetworkConfig(): Promise<NetworkConfig> {
    const config: NetworkConfig = await invoke('reset_server_network_config')
    networkConfig.value = config
    port.value = config.port
    autoStart.value = config.auto_start
    return config
  }

  /** 轮询指标 */
  async function pollMetrics() {
    if (status.value !== 'running') return
    try {
      const m: ServerMetrics = await invoke('get_server_metrics')
      metrics.value = m

      const entry: TimestampedMetrics = {
        timestamp_secs: m.uptime_secs,
        ws_sent_rate: m.ws_sent_rate,
        ws_recv_rate: m.ws_recv_rate,
      }
      metricsHistory.value.push(entry)
      if (metricsHistory.value.length > MAX_HISTORY) {
        metricsHistory.value.shift()
      }
    } catch {
      // 忽略轮询错误
    }
  }

  /** 开始轮询 */
  function startPolling() {
    stopPolling()
    pollTimer = setInterval(pollMetrics, 2000)
  }

  /** 停止轮询 */
  function stopPolling() {
    if (pollTimer !== null) {
      clearInterval(pollTimer)
      pollTimer = null
    }
  }

  onUnmounted(() => {
    stopPolling()
  })

  return {
    status,
    port,
    autoStart,
    localIps,
    metrics,
    metricsHistory,
    loading,
    networkConfig,
    loadStatus,
    startServer,
    stopServer,
    restartServer,
    updatePort,
    updateAutoStart,
    startPolling,
    stopPolling,
    loadNetworkConfig,
    updateNetworkConfig,
    resetNetworkConfig,
  }
}
