<template>
  <div class="h-full overflow-y-auto">
    <!-- ==================== 无缝头部 ==================== -->
    <header class="bg-page px-8 h-14 flex items-center justify-between">
      <h1 class="text-lg font-semibold text-[var(--text-primary)]">{{ $t('desktop.server.config') }}</h1>
    </header>

    <!-- ==================== 内容区 ==================== -->
    <div class="p-6 px-8 max-w-4xl mx-auto space-y-4">
    <!-- ==================== 区块 1：服务器配置 ==================== -->
    <div class="bg-card rounded-card p-6 shadow-card animate-fade-slide-up">
      <!-- 状态指示 -->
      <div class="flex items-center gap-3 mb-4">
        <div class="flex items-center gap-1.5 rounded-tag h-7 px-3 text-xs font-medium"
          :class="status === 'running' ? 'bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-400' : status === 'starting' ? 'bg-yellow-100 dark:bg-yellow-900/30 text-yellow-700 dark:text-yellow-400' : 'bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-400'"
        >
          <div class="w-2 h-2 rounded-full"
            :class="status === 'running' ? 'bg-green-500' : status === 'starting' ? 'bg-yellow-500' : 'bg-red-500'"
          ></div>
          {{ statusText }}
        </div>
      </div>

      <!-- 操作按钮行 -->
      <div class="flex flex-wrap items-center gap-2 mb-5">
        <button
          class="px-3 py-1.5 text-sm font-medium rounded-btn transition-colors"
          :class="status === 'stopped' ? 'bg-brand text-white hover:bg-[var(--color-primary-hover)]' : 'bg-[var(--bg-hover)] text-[var(--text-tertiary)] cursor-not-allowed'"
          :disabled="status !== 'stopped' || loading"
          @click="handleStart"
        >
          {{ $t('desktop.server.start') }}
        </button>
        <button
          class="px-3 py-1.5 text-sm font-medium rounded-btn transition-colors"
          :class="status === 'running' ? 'bg-[var(--color-danger-light)] text-red-600 hover:bg-red-100' : 'bg-[var(--bg-hover)] text-[var(--text-tertiary)] cursor-not-allowed'"
          :disabled="status !== 'running' || loading"
          @click="handleStop"
        >
          {{ $t('desktop.server.stop') }}
        </button>
        <button
          class="px-3 py-1.5 text-sm font-medium rounded-btn transition-colors"
          :class="status === 'running' ? 'bg-blue-600 text-white hover:bg-blue-700' : 'bg-[var(--bg-hover)] text-[var(--text-tertiary)] cursor-not-allowed'"
          :disabled="status !== 'running' || loading"
          @click="handleRestart"
        >
          {{ $t('desktop.server.restart') }}
        </button>
        <button
          class="px-3 py-1.5 text-sm font-medium rounded-btn transition-colors border border-[var(--border)] text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
          :disabled="loading"
          @click="handleResetDefaults"
        >
          {{ $t('desktop.server.resetDefaults') }}
        </button>
      </div>

      <!-- 配置项网格 -->
      <div class="grid grid-cols-[auto_1fr] gap-x-6 gap-y-3 items-center">
        <!-- 端口 -->
        <label class="text-sm text-[var(--text-tertiary)] text-right">{{ $t('desktop.server.port') }}</label>
        <input
          v-model.number="portInput"
          type="number"
          min="1024"
          max="65535"
          class="w-20 h-7 px-2 text-sm rounded-input border border-[var(--border-input)] bg-[var(--bg-input)] text-[var(--text-primary)]"
        />

        <!-- 本地 IP -->
        <label class="text-sm text-[var(--text-tertiary)] text-right">{{ $t('desktop.server.localIp') }}</label>
        <div class="flex flex-wrap gap-x-4 gap-y-0.5">
          <span v-for="ip in localIps" :key="ip" class="text-sm text-[var(--text-primary)] font-mono">{{ ip }}</span>
          <span v-if="localIps.length === 0" class="text-sm text-[var(--text-tertiary)]">-</span>
        </div>

        <!-- 自启动 -->
        <label class="text-sm text-[var(--text-tertiary)] text-right">{{ $t('desktop.server.autoStart') }}</label>
        <button
          class="relative w-10 h-5 rounded-full transition-colors"
          :class="autoStart ? 'bg-brand' : 'bg-[var(--border)]'"
          @click="handleAutoStartToggle(!autoStart)"
        >
          <span
            class="absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full transition-transform shadow-sm"
            :class="autoStart ? 'translate-x-5' : 'translate-x-0'"
          ></span>
        </button>
      </div>

      <!-- ==================== 高级配置（折叠区域） ==================== -->
      <div v-if="networkConfig" class="mt-5 border-t border-[var(--border)] pt-4">
        <!-- 折叠标题行 -->
        <button
          class="flex items-center gap-2 w-full text-left group"
          @click="advExpanded = !advExpanded"
        >
          <svg
            class="w-4 h-4 text-[var(--text-tertiary)] transition-transform duration-200"
            :class="advExpanded ? 'rotate-90' : 'rotate-0'"
            fill="none" stroke="currentColor" viewBox="0 0 24 24"
          >
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
          </svg>
          <h2 class="text-sm font-semibold text-[var(--text-secondary)] group-hover:text-[var(--text-primary)] transition-colors">
            {{ $t('desktop.server.advancedConfig') }}
          </h2>
        </button>

        <!-- 折叠内容 -->
        <Transition name="adv-collapse">
          <div v-show="advExpanded" class="mt-4">
            <div class="grid grid-cols-[auto_1fr] gap-x-6 gap-y-3 items-center">
              <!-- Workers -->
              <label class="text-sm text-[var(--text-tertiary)] text-right">{{ $t('desktop.server.workers') }}</label>
              <div class="flex items-center gap-2">
                <input v-model.number="advConfig!.workers" type="number" min="0" max="64"
                  class="w-20 h-7 px-2 text-sm rounded-input border border-[var(--border-input)] bg-[var(--bg-input)] text-[var(--text-primary)]" />
                <span class="text-xs text-[var(--text-tertiary)]">{{ $t('desktop.server.workersHint') }}</span>
              </div>

              <!-- Keep-Alive -->
              <label class="text-sm text-[var(--text-tertiary)] text-right">{{ $t('desktop.server.keepAlive') }}</label>
              <div class="flex items-center gap-2">
                <input v-model.number="advConfig!.keep_alive_secs" type="number" min="0" max="300"
                  class="w-20 h-7 px-2 text-sm rounded-input border border-[var(--border-input)] bg-[var(--bg-input)] text-[var(--text-primary)]" />
                <span class="text-xs text-[var(--text-tertiary)]">{{ $t('desktop.server.keepAliveHint') }}</span>
              </div>

              <!-- Client Request Timeout -->
              <label class="text-sm text-[var(--text-tertiary)] text-right">{{ $t('desktop.server.clientRequestTimeout') }}</label>
              <div class="flex items-center gap-2">
                <input v-model.number="advConfig!.client_request_timeout_secs" type="number" min="1" max="120"
                  class="w-20 h-7 px-2 text-sm rounded-input border border-[var(--border-input)] bg-[var(--bg-input)] text-[var(--text-primary)]" />
                <span class="text-xs text-[var(--text-tertiary)]">s</span>
              </div>

              <!-- Client Disconnect Timeout -->
              <label class="text-sm text-[var(--text-tertiary)] text-right">{{ $t('desktop.server.clientDisconnectTimeout') }}</label>
              <div class="flex items-center gap-2">
                <input v-model.number="advConfig!.client_disconnect_timeout_secs" type="number" min="1" max="120"
                  class="w-20 h-7 px-2 text-sm rounded-input border border-[var(--border-input)] bg-[var(--bg-input)] text-[var(--text-primary)]" />
                <span class="text-xs text-[var(--text-tertiary)]">s</span>
              </div>

              <!-- Max Connections -->
              <label class="text-sm text-[var(--text-tertiary)] text-right">{{ $t('desktop.server.maxConnections') }}</label>
              <div class="flex items-center gap-2">
                <input v-model.number="advConfig!.max_connections" type="number" min="1" max="100000"
                  class="w-24 h-7 px-2 text-sm rounded-input border border-[var(--border-input)] bg-[var(--bg-input)] text-[var(--text-primary)]" />
                <span class="text-xs text-[var(--text-tertiary)]">{{ $t('desktop.server.maxConnectionsHint') }}</span>
              </div>

              <!-- Backlog -->
              <label class="text-sm text-[var(--text-tertiary)] text-right">{{ $t('desktop.server.backlog') }}</label>
              <div class="flex items-center gap-2">
                <input v-model.number="advConfig!.backlog" type="number" min="64" max="8192"
                  class="w-24 h-7 px-2 text-sm rounded-input border border-[var(--border-input)] bg-[var(--bg-input)] text-[var(--text-primary)]" />
              </div>

              <!-- TCP_NODELAY -->
              <label class="text-sm text-[var(--text-tertiary)] text-right">{{ $t('desktop.server.tcpNodelay') }}</label>
              <button
                class="relative w-10 h-5 rounded-full transition-colors"
                :class="advConfig!.tcp_nodelay ? 'bg-brand' : 'bg-[var(--border)]'"
                @click="advConfig!.tcp_nodelay = !advConfig!.tcp_nodelay"
              >
                <span
                  class="absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full transition-transform shadow-sm"
                  :class="advConfig!.tcp_nodelay ? 'translate-x-5' : 'translate-x-0'"
                ></span>
              </button>

              <!-- Shutdown Timeout -->
              <label class="text-sm text-[var(--text-tertiary)] text-right">{{ $t('desktop.server.shutdownTimeout') }}</label>
              <div class="flex items-center gap-2">
                <input v-model.number="advConfig!.shutdown_timeout_secs" type="number" min="1" max="300"
                  class="w-20 h-7 px-2 text-sm rounded-input border border-[var(--border-input)] bg-[var(--bg-input)] text-[var(--text-primary)]" />
                <span class="text-xs text-[var(--text-tertiary)]">s</span>
              </div>

              <!-- WS Max Frame Size -->
              <label class="text-sm text-[var(--text-tertiary)] text-right">{{ $t('desktop.server.wsMaxFrameSize') }}</label>
              <div class="flex items-center gap-2">
                <input v-model.number="advConfig!.ws_max_frame_size_kb" type="number" min="1" max="16384"
                  class="w-24 h-7 px-2 text-sm rounded-input border border-[var(--border-input)] bg-[var(--bg-input)] text-[var(--text-primary)]" />
                <span class="text-xs text-[var(--text-tertiary)]">{{ $t('desktop.server.wsMaxFrameSizeHint') }}</span>
              </div>

              <!-- WS Max Message Size -->
              <label class="text-sm text-[var(--text-tertiary)] text-right">{{ $t('desktop.server.wsMaxMessageSize') }}</label>
              <div class="flex items-center gap-2">
                <input v-model.number="advConfig!.ws_max_message_size_mb" type="number" min="1" max="512"
                  class="w-24 h-7 px-2 text-sm rounded-input border border-[var(--border-input)] bg-[var(--bg-input)] text-[var(--text-primary)]" />
                <span class="text-xs text-[var(--text-tertiary)]">{{ $t('desktop.server.wsMaxMessageSizeHint') }}</span>
              </div>
            </div>
          </div>
        </Transition>
      </div>
    </div>

    <!-- ==================== 区块 2：性能监控 ==================== -->
    <div class="bg-card rounded-card p-6 shadow-card animate-fade-slide-up" style="animation-delay: 80ms">
      <h2 class="text-lg font-semibold text-[var(--text-primary)] mb-4">
        {{ $t('desktop.server.monitoring') }}
      </h2>

      <!-- 指标卡片：始终显示，无数据时显示 - -->
      <div class="grid grid-cols-2 md:grid-cols-3 gap-3 mb-6">
        <div class="bg-[var(--bg-hover)]/50 rounded-input p-4">
          <div class="text-xs text-[var(--text-tertiary)]">{{ $t('desktop.server.uptime') }}</div>
          <div class="text-lg font-semibold text-[var(--text-primary)]">{{ status === 'running' ? formatUptime(uptimeTick) : '-' }}</div>
        </div>
        <div class="bg-[var(--bg-hover)]/50 rounded-input p-4">
          <div class="text-xs text-[var(--text-tertiary)]">{{ $t('desktop.server.connections') }}</div>
          <div class="text-lg font-semibold text-[var(--text-primary)]">{{ metrics?.connections ?? '-' }}</div>
        </div>
        <div class="bg-[var(--bg-hover)]/50 rounded-input p-4">
          <div class="text-xs text-[var(--text-tertiary)]">{{ $t('desktop.server.totalRequests') }}</div>
          <div class="text-lg font-semibold text-[var(--text-primary)]">{{ metrics ? (metrics.total_http_requests).toLocaleString() : '-' }}</div>
        </div>
        <div class="bg-[var(--bg-hover)]/50 rounded-input p-4">
          <div class="text-xs text-[var(--text-tertiary)]">{{ $t('desktop.server.cpuUsage') }}</div>
          <div class="text-lg font-semibold text-[var(--text-primary)]">{{ metrics ? `${metrics.cpu_usage_percent.toFixed(1)}%` : '-' }}</div>
        </div>
        <div class="bg-[var(--bg-hover)]/50 rounded-input p-4">
          <div class="text-xs text-[var(--text-tertiary)]">{{ $t('desktop.server.memoryUsage') }}</div>
          <div class="text-lg font-semibold text-[var(--text-primary)]">{{ metrics ? formatMemory(metrics.memory_usage_bytes) : '-' }}</div>
        </div>
        <div class="bg-[var(--bg-hover)]/50 rounded-input p-4">
          <div class="text-xs text-[var(--text-tertiary)]">{{ $t('desktop.server.requestRate') }}</div>
          <div class="text-lg font-semibold text-[var(--text-primary)]">{{ metrics ? `${metrics.http_requests_per_sec.toFixed(1)}/s` : '-' }}</div>
        </div>
      </div>

      <!-- WS 消息时序图：始终显示，无数据时展示空图表 -->
      <div>
        <h3 class="text-sm font-medium text-[var(--text-secondary)] mb-2">
          {{ $t('desktop.server.wsThroughput') }}
        </h3>
        <VChart :option="chartOption" style="height: 250px; width: 100%;" autoresize />
      </div>
    </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 服务器管理页面 — 配置、启停控制、性能监控
 */
import { onMounted, onUnmounted, computed, watch, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useServer } from '@/composables/useServer'
import { useToast } from '@/composables/useToast'
import VChart from 'vue-echarts'
import { use } from 'echarts/core'
import { LineChart } from 'echarts/charts'
import {
  TitleComponent,
  TooltipComponent,
  LegendComponent,
  GridComponent,
} from 'echarts/components'
import { CanvasRenderer } from 'echarts/renderers'

use([TitleComponent, TooltipComponent, LegendComponent, GridComponent, LineChart, CanvasRenderer])

const { t } = useI18n()
const toast = useToast()

const {
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
} = useServer()

const portInput = computed({
  get: () => port.value,
  set: (v: number) => { port.value = v },
})

const statusText = computed(() => {
  switch (status.value) {
    case 'running': return t('desktop.server.status.running')
    case 'starting': return t('desktop.server.status.starting')
    default: return t('desktop.server.status.stopped')
  }
})

/** 运行时长本地计时 — 每秒刷新，不依赖后端轮询 */
const uptimeTick = ref(0)
let uptimeTimer: ReturnType<typeof setInterval> | null = null

watch(status, (val) => {
  if (uptimeTimer) { clearInterval(uptimeTimer); uptimeTimer = null }
  if (val === 'running' && metrics.value) {
    uptimeTick.value = metrics.value.uptime_secs
    uptimeTimer = setInterval(() => { uptimeTick.value++ }, 1000)
  }
})

// 首次 metrics 到达时启动计时
watch(metrics, (m) => {
  if (m && status.value === 'running' && !uptimeTimer) {
    uptimeTick.value = m.uptime_secs
    uptimeTimer = setInterval(() => { uptimeTick.value++ }, 1000)
  }
})

/** 格式化运行时长 — HH:MM:SS 格式 */
function formatUptime(secs: number): string {
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  const s = secs % 60
  return `${h}:${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`
}

/** 格式化内存 */
function formatMemory(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`
  return `${(bytes / 1024).toFixed(0)} KB`
}

/** ECharts 时序图配置 */
const chartOption = computed(() => {
  const history = metricsHistory.value
  const times = history.map(h => {
    const m = Math.floor(h.timestamp_secs / 60)
    const s = h.timestamp_secs % 60
    return `${m}:${s.toString().padStart(2, '0')}`
  })

  return {
    tooltip: { trigger: 'axis' as const },
    legend: {
      data: [t('desktop.server.wsSentRate'), t('desktop.server.wsRecvRate')],
      top: 0,
    },
    grid: { left: '3%', right: '4%', bottom: '3%', containLabel: true },
    xAxis: {
      type: 'category' as const,
      boundaryGap: false,
      data: times,
    },
    yAxis: {
      type: 'value' as const,
      name: 'msg/s',
    },
    series: [
      {
        name: t('desktop.server.wsSentRate'),
        type: 'line' as const,
        smooth: true,
        symbol: 'none',
        itemStyle: { color: '#3b82f6' },
        areaStyle: { color: 'rgba(59,130,246,0.1)' },
        data: history.map(h => h.ws_sent_rate.toFixed(2)),
      },
      {
        name: t('desktop.server.wsRecvRate'),
        type: 'line' as const,
        smooth: true,
        symbol: 'none',
        itemStyle: { color: '#22c55e' },
        areaStyle: { color: 'rgba(34,197,94,0.1)' },
        data: history.map(h => h.ws_recv_rate.toFixed(2)),
      },
    ],
  }
})

/** 启动服务器 */
async function handleStart() {
  try {
    await startServer()
    toast.success(t('desktop.server.startSuccess'))
    startPolling()
  } catch (e: any) {
    toast.error(e.message)
  }
}

/** 停止服务器 */
async function handleStop() {
  try {
    await stopServer()
    toast.success(t('desktop.server.stopSuccess'))
    stopPolling()
  } catch (e: any) {
    toast.error(e.message)
  }
}

/** 重启服务器（自动保存未应用的端口和高级配置） */
async function handleRestart() {
  try {
    // 端口变更时先保存
    if (portInput.value !== port.value) {
      await updatePort(portInput.value)
    }
    // 高级配置变更时先保存
    if (networkConfig.value && advConfig.value) {
      const dirty = Object.entries(advConfig.value).some(
        ([k, v]) => (networkConfig.value as any)[k] !== v
      )
      if (dirty) {
        const merged = { ...networkConfig.value, ...advConfig.value }
        await updateNetworkConfig(merged)
      }
    }
    await restartServer()
    toast.success(t('desktop.server.restartSuccess'))
    startPolling()
  } catch (e: any) {
    toast.error(e.message)
  }
}

/** 切换自启动 */
async function handleAutoStartToggle(val: boolean) {
  try {
    await updateAutoStart(val)
  } catch (e: any) {
    toast.error(String(e))
  }
}

/** 高级配置折叠状态 — 默认折叠 */
const advExpanded = ref(false)

/** 高级配置本地编辑副本 */
const advConfig = computed({
  get: () => networkConfig.value ? {
    workers: networkConfig.value.workers,
    keep_alive_secs: networkConfig.value.keep_alive_secs,
    client_request_timeout_secs: networkConfig.value.client_request_timeout_secs,
    client_disconnect_timeout_secs: networkConfig.value.client_disconnect_timeout_secs,
    max_connections: networkConfig.value.max_connections,
    backlog: networkConfig.value.backlog,
    tcp_nodelay: networkConfig.value.tcp_nodelay,
    shutdown_timeout_secs: networkConfig.value.shutdown_timeout_secs,
    ws_max_frame_size_kb: networkConfig.value.ws_max_frame_size_kb,
    ws_max_message_size_mb: networkConfig.value.ws_max_message_size_mb,
  } : null,
  set: (v) => { if (v && networkConfig.value) Object.assign(networkConfig.value, v) },
})

/** 还原所有配置为默认值（端口、自启动、高级配置） */
async function handleResetDefaults() {
  try {
    const defaults = await resetNetworkConfig()
    // 同步端口和自启动
    portInput.value = defaults.port
    // 同步高级配置编辑副本
    advConfig.value = {
      workers: defaults.workers,
      keep_alive_secs: defaults.keep_alive_secs,
      client_request_timeout_secs: defaults.client_request_timeout_secs,
      client_disconnect_timeout_secs: defaults.client_disconnect_timeout_secs,
      max_connections: defaults.max_connections,
      backlog: defaults.backlog,
      tcp_nodelay: defaults.tcp_nodelay,
      shutdown_timeout_secs: defaults.shutdown_timeout_secs,
      ws_max_frame_size_kb: defaults.ws_max_frame_size_kb,
      ws_max_message_size_mb: defaults.ws_max_message_size_mb,
    }
    toast.success(t('desktop.server.resetSuccess'))
  } catch (e: any) {
    toast.error(e.message)
  }
}

onMounted(async () => {
  await loadStatus()
  await loadNetworkConfig()
  if (status.value === 'running') {
    startPolling()
  }
})

onUnmounted(() => {
  if (uptimeTimer) { clearInterval(uptimeTimer); uptimeTimer = null }
})

watch(status, (newVal) => {
  if (newVal === 'running') {
    startPolling()
  } else {
    stopPolling()
  }
})
</script>

<style scoped>
.adv-collapse-enter-active,
.adv-collapse-leave-active {
  transition: all 0.2s ease;
  overflow: hidden;
}
.adv-collapse-enter-from,
.adv-collapse-leave-to {
  opacity: 0;
  max-height: 0;
  margin-top: 0;
}
.adv-collapse-enter-to,
.adv-collapse-leave-from {
  opacity: 1;
  max-height: 500px;
  margin-top: 1rem;
}
</style>
