/**
 * useServer 测试
 *
 * 覆盖可纯逻辑测部分：状态加载、生命周期命令参数构造、网络配置、
 * 指标轮询（含 60 条历史上限）、轮询定时器启停。
 * 事件监听（listen）不在本 composable 中，无需 mock。
 * 注 1：status 为模块级共享 ref，beforeEach 中重置，保证用例相互隔离。
 * 注 2：pollMetrics 未从 useServer 导出（仅内部 setInterval 使用），
 *       轮询行为统一通过 startPolling + fake timers 驱动验证。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { useServer, type ServerStatusInfo, type NetworkConfig, type ServerMetrics } from '@/composables/useServer'

// Mock Tauri invoke
const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}))

const statusInfo: ServerStatusInfo = {
  status: 'running',
  port: 9000,
  auto_start: false,
  local_ips: ['192.168.1.5', '127.0.0.1'],
}

const networkConfig: NetworkConfig = {
  port: 9000,
  auto_start: false,
  prevent_sleep: true,
  workers: 4,
  keep_alive_secs: 30,
  client_request_timeout_secs: 30,
  client_disconnect_timeout_secs: 30,
  max_connections: 100,
  backlog: 1024,
  tcp_nodelay: true,
  shutdown_timeout_secs: 10,
  ws_max_frame_size_kb: 64,
  ws_max_message_size_mb: 16,
  metrics_enabled: false,
}

const metrics: ServerMetrics = {
  uptime_secs: 120,
  connections: 3,
  total_http_requests: 1000,
  http_requests_per_sec: 5,
  ws_messages_sent: 500,
  ws_messages_received: 400,
  ws_sent_rate: 1.5,
  ws_recv_rate: 2.5,
  cpu_usage_percent: 10,
  memory_usage_bytes: 1024,
}

let consoleWarnSpy: ReturnType<typeof vi.spyOn>
let consoleErrorSpy: ReturnType<typeof vi.spyOn>

describe('useServer', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    // useServer 的 onUnmounted 在非组件上下文调用会触发 Vue warning，先静音再实例化
    consoleWarnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
    consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
    // 重置模块级共享 status，并清理上一个用例遗留的轮询定时器
    const s = useServer()
    s.status.value = 'stopped'
    s.stopPolling()
  })

  afterEach(() => {
    consoleWarnSpy.mockRestore()
    consoleErrorSpy.mockRestore()
    vi.useRealTimers()
  })

  describe('initial state', () => {
    it('should initialize with defaults', () => {
      const server = useServer()

      expect(server.status.value).toBe('stopped')
      expect(server.port.value).toBe(8765)
      expect(server.autoStart.value).toBe(true)
      expect(server.localIps.value).toEqual([])
      expect(server.loading.value).toBe(false)
      expect(server.metrics.value).toBeNull()
      expect(server.metricsHistory.value).toEqual([])
      expect(server.networkConfig.value).toBeNull()
    })
  })

  describe('loadStatus', () => {
    it('should fetch server status and update state', async () => {
      mockInvoke.mockResolvedValueOnce(statusInfo)

      const server = useServer()
      await server.loadStatus()

      expect(mockInvoke).toHaveBeenCalledWith('get_server_status')
      expect(server.status.value).toBe('running')
      expect(server.port.value).toBe(9000)
      expect(server.autoStart.value).toBe(false)
      expect(server.localIps.value).toEqual(['192.168.1.5', '127.0.0.1'])
    })

    it('should keep state unchanged on failure', async () => {
      mockInvoke.mockRejectedValueOnce(new Error('backend down'))

      const server = useServer()
      server.status.value = 'starting'
      await server.loadStatus()

      expect(server.status.value).toBe('starting')
      expect(server.port.value).toBe(8765)
      expect(consoleErrorSpy).toHaveBeenCalledWith('Failed to load server status:', expect.any(Error))
    })
  })

  describe('lifecycle commands', () => {
    it('startServer should invoke with current port and set status to running', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const server = useServer()
      server.port.value = 7777
      await server.startServer()

      expect(mockInvoke).toHaveBeenCalledWith('server_start', { port: 7777 })
      expect(server.status.value).toBe('running')
      expect(server.loading.value).toBe(false)
    })

    it('startServer should hold loading flag while request is in flight', async () => {
      let resolveFn: () => void
      mockInvoke.mockReturnValueOnce(new Promise<void>(resolve => { resolveFn = resolve }))

      const server = useServer()
      const pending = server.startServer()
      expect(server.loading.value).toBe(true)

      resolveFn!()
      await pending
      expect(server.loading.value).toBe(false)
    })

    it('startServer should throw i18n error and keep status on failure', async () => {
      mockInvoke.mockRejectedValueOnce(new Error('boom'))

      const server = useServer()
      await expect(server.startServer()).rejects.toThrow('启动失败: Error: boom')
      expect(server.status.value).toBe('stopped')
      expect(server.loading.value).toBe(false)
    })

    it('stopServer should invoke server_stop and set status to stopped', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const server = useServer()
      server.status.value = 'running'
      await server.stopServer()

      expect(mockInvoke).toHaveBeenCalledWith('server_stop')
      expect(server.status.value).toBe('stopped')
    })

    it('restartServer should invoke server_restart and set status to running', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const server = useServer()
      await server.restartServer()

      expect(mockInvoke).toHaveBeenCalledWith('server_restart')
      expect(server.status.value).toBe('running')
    })

    it('updatePort should pass the new port to backend and update local state', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const server = useServer()
      await server.updatePort(9999)

      expect(mockInvoke).toHaveBeenCalledWith('update_server_port', { port: 9999 })
      expect(server.port.value).toBe(9999)
    })

    it('updateAutoStart should pass the flag and update local state', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const server = useServer()
      await server.updateAutoStart(false)

      expect(mockInvoke).toHaveBeenCalledWith('update_server_auto_start', { autoStart: false })
      expect(server.autoStart.value).toBe(false)
    })
  })

  describe('network config', () => {
    it('loadNetworkConfig should fetch and sync port/autoStart', async () => {
      mockInvoke.mockResolvedValueOnce(networkConfig)

      const server = useServer()
      await server.loadNetworkConfig()

      expect(mockInvoke).toHaveBeenCalledWith('get_server_network_config')
      expect(server.networkConfig.value).toEqual(networkConfig)
      expect(server.port.value).toBe(9000)
      expect(server.autoStart.value).toBe(false)
    })

    it('loadNetworkConfig should keep state on failure', async () => {
      mockInvoke.mockRejectedValueOnce(new Error('no config'))

      const server = useServer()
      await server.loadNetworkConfig()

      expect(server.networkConfig.value).toBeNull()
      expect(consoleErrorSpy).toHaveBeenCalledWith('Failed to load network config:', expect.any(Error))
    })

    it('updateNetworkConfig should pass config and update state with a copy', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const server = useServer()
      const updated = { ...networkConfig, port: 8080 }
      await server.updateNetworkConfig(updated)

      expect(mockInvoke).toHaveBeenCalledWith('update_server_network_config', { networkConfig: updated })
      expect(server.networkConfig.value).toEqual(updated)
      expect(server.networkConfig.value).not.toBe(updated) // 存副本，防止外部改动穿透
      expect(server.port.value).toBe(8080)
      expect(server.autoStart.value).toBe(false)
    })

    it('resetNetworkConfig should return the reset config and sync state', async () => {
      const reset = { ...networkConfig, port: 8765, auto_start: true }
      mockInvoke.mockResolvedValueOnce(reset)

      const server = useServer()
      const result = await server.resetNetworkConfig()

      expect(mockInvoke).toHaveBeenCalledWith('reset_server_network_config')
      expect(result).toEqual(reset)
      expect(server.networkConfig.value).toEqual(reset)
      expect(server.port.value).toBe(8765)
      expect(server.autoStart.value).toBe(true)
    })
  })

  describe('metrics polling', () => {
    it('should skip polling when server is not running', async () => {
      vi.useFakeTimers()

      const server = useServer()
      server.status.value = 'stopped'
      server.startPolling()
      await vi.advanceTimersByTimeAsync(2000)

      expect(mockInvoke).not.toHaveBeenCalledWith('get_server_metrics')
      expect(server.metrics.value).toBeNull()
      server.stopPolling()
    })

    it('should record metrics with timestamp entry', async () => {
      vi.useFakeTimers()
      mockInvoke.mockResolvedValue(metrics)

      const server = useServer()
      server.status.value = 'running'
      server.startPolling()
      await vi.advanceTimersByTimeAsync(2000)

      expect(mockInvoke).toHaveBeenCalledWith('get_server_metrics')
      expect(server.metrics.value).toEqual(metrics)
      expect(server.metricsHistory.value).toEqual([{
        timestamp_secs: 120,
        ws_sent_rate: 1.5,
        ws_recv_rate: 2.5,
      }])
      server.stopPolling()
    })

    it('should cap history at 60 entries', async () => {
      vi.useFakeTimers()
      mockInvoke.mockResolvedValue(metrics)

      const server = useServer()
      server.status.value = 'running'
      server.startPolling()
      // 61 次轮询，超过 60 条上限后最旧条目被丢弃
      await vi.advanceTimersByTimeAsync(2000 * 61)

      expect(server.metricsHistory.value).toHaveLength(60)
      expect(server.metricsHistory.value[59].timestamp_secs).toBe(120)
      server.stopPolling()
    })

    it('should swallow polling errors silently and recover on next poll', async () => {
      vi.useFakeTimers()
      // 预挂 handler 消费 rejected promise，避免 unhandled rejection 干扰 fake timers 微任务队列
      const rejection = Promise.reject(new Error('metrics unavailable'))
      rejection.catch(() => {})
      mockInvoke.mockReturnValueOnce(rejection)
      mockInvoke.mockResolvedValue(metrics)

      const server = useServer()
      server.status.value = 'running'
      server.startPolling()

      // 第一次轮询失败：错误被静默吞掉，metrics 保持 null
      await vi.advanceTimersByTimeAsync(2000)
      expect(server.metrics.value).toBeNull()

      // 第二次轮询恢复正常
      await vi.advanceTimersByTimeAsync(2000)
      expect(server.metrics.value).toEqual(metrics)
      expect(server.metricsHistory.value).toHaveLength(1)
      server.stopPolling()
    })

    it('should poll on interval and stopPolling should stop', async () => {
      vi.useFakeTimers()
      mockInvoke.mockResolvedValue(metrics)

      const server = useServer()
      server.status.value = 'running'
      server.startPolling()

      await vi.advanceTimersByTimeAsync(2000)
      expect(mockInvoke).toHaveBeenCalledWith('get_server_metrics')
      expect(server.metricsHistory.value).toHaveLength(1)

      await vi.advanceTimersByTimeAsync(4000)
      expect(server.metricsHistory.value).toHaveLength(3)

      server.stopPolling()
      const callCount = mockInvoke.mock.calls.length
      await vi.advanceTimersByTimeAsync(6000)
      expect(mockInvoke.mock.calls.length).toBe(callCount)
    })
  })
})
