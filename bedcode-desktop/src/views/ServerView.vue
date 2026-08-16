<template>
  <div class="h-full overflow-y-auto bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头：左标题+状态，右启停/重启/重置 ==================== -->
    <div class="wb-toolbar sticky top-0 z-10">
      <div class="flex items-center gap-2.5">
        <h1 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">{{ t('desktop.server.title') }}</h1>
        <span class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">{{ statusText }}</span>
      </div>
      <div class="flex items-center gap-2">
        <PluginPageToolbar target="server" />
        <button
          class="wb-btn-primary"
          :class="status !== 'stopped' && '!bg-[var(--bg-hover)] !text-[var(--text-tertiary)] cursor-not-allowed'"
          :disabled="status !== 'stopped' || loading"
          @click="handleStart"
        >
          {{ t('desktop.server.start') }}
        </button>
        <button
          class="wb-btn-ghost"
          :class="status !== 'running' && '!text-[var(--text-tertiary)] cursor-not-allowed'"
          :disabled="status !== 'running' || loading"
          @click="handleStop"
        >
          {{ t('desktop.server.stop') }}
        </button>
        <button
          class="wb-btn-ghost"
          :class="status !== 'running' && '!text-[var(--text-tertiary)] cursor-not-allowed'"
          :disabled="status !== 'running' || loading"
          @click="handleRestart"
        >
          {{ t('desktop.server.restart') }}
        </button>
        <button class="wb-btn-ghost !text-[var(--text-secondary)]" :disabled="loading" @click="handleResetDefaults">
          {{ t('desktop.server.resetDefaults') }}
        </button>
      </div>
    </div>

    <!-- ==================== 加载态 ==================== -->
    <div v-if="initializing" class="p-5 space-y-4 max-w-5xl mx-auto">
      <div v-for="i in 3" :key="i" class="h-24 rounded-[10px] animate-pulse bg-[var(--bg-card)] border border-[var(--border)]"></div>
    </div>

    <!-- ==================== 内容分区 ==================== -->
    <div v-else class="p-5 max-w-5xl mx-auto space-y-6">
      <!-- ---------- SECTION: STATUS ---------- -->
      <section>
        <h2 class="wb-section-title">{{ t('desktop.server.sectionStatus') }}</h2>
        <div class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px]">
          <div class="flex items-center justify-between px-4 h-12 border-b border-[var(--border)]">
            <div class="flex items-center gap-2">
              <span class="w-2 h-2 rounded-full" :class="dotClass"></span>
              <span class="text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-primary)]">{{ statusText }}</span>
            </div>
            <span class="wb-mono text-[var(--text-secondary)]">
              {{ status === 'running' ? formatUptime(uptimeTick) : '-' }}
            </span>
          </div>
          <div class="grid grid-cols-3 divide-x divide-[var(--border)]">
            <div class="px-4 py-3">
              <div class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] mb-1">{{ t('desktop.server.connections') }}</div>
              <div class="wb-mono text-[calc(12.5px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">{{ metrics?.connections ?? '-' }}</div>
            </div>
            <div class="px-4 py-3">
              <div class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] mb-1">{{ t('desktop.server.cpuUsage') }}</div>
              <div class="wb-mono text-[calc(12.5px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">{{ metrics ? `${metrics.cpu_usage_percent.toFixed(1)}%` : '-' }}</div>
            </div>
            <div class="px-4 py-3">
              <div class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] mb-1">{{ t('desktop.server.memoryUsage') }}</div>
              <div class="wb-mono text-[calc(12.5px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">{{ metrics ? formatMemory(metrics.memory_usage_bytes) : '-' }}</div>
            </div>
          </div>
        </div>
      </section>

      <!-- ---------- SECTION: NETWORK ---------- -->
      <section>
        <h2 class="wb-section-title">{{ t('desktop.server.sectionNetwork') }}</h2>
        <div class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] px-4">
          <div class="flex items-center justify-between h-12 border-b border-[var(--border)]">
            <span class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)]">{{ t('desktop.server.port') }}</span>
            <div class="flex items-center gap-3">
              <input
                v-model.number="portInput"
                type="number"
                min="1024"
                max="65535"
                class="w-20 h-7 px-2 wb-mono rounded-[6px] border border-[var(--border-input)] bg-[var(--bg-input)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
              />
              <span class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">{{ t('desktop.server.portHint') }}</span>
            </div>
          </div>
          <div class="flex items-center justify-between min-h-12 py-2 border-b border-[var(--border)]">
            <span class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)]">{{ t('desktop.server.localIp') }}</span>
            <div class="flex flex-wrap justify-end gap-x-4 gap-y-0.5">
              <span v-for="ip in localIps" :key="ip" class="wb-mono text-[var(--text-primary)]">{{ ip }}:{{ port }}</span>
              <span v-if="localIps.length === 0" class="text-[calc(12.5px*var(--ui-scale))] text-[var(--text-tertiary)]">-</span>
            </div>
          </div>
          <div class="flex items-center justify-between h-12">
            <span class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)]">{{ t('desktop.server.autoStart') }}</span>
            <button
              class="relative w-10 h-5 rounded-[4px] border transition-colors flex-shrink-0"
              :class="autoStart ? 'bg-[var(--color-primary)] border-[var(--color-primary)]' : 'bg-[var(--bg-page)] border-[var(--border-strong)]'"
              @click="handleAutoStartToggle(!autoStart)"
            >
              <span
                class="absolute top-[3px] w-3 h-3 rounded-[2px] transition-all"
                :class="autoStart ? 'left-[22px] bg-[var(--color-primary-contrast)]' : 'left-[3px] bg-[var(--border-strong)]'"
              ></span>
            </button>
          </div>
        </div>
      </section>

      <!-- ---------- SECTION: CONFIG ---------- -->
      <section v-if="networkConfig">
        <div class="flex items-center justify-between mb-2">
          <h2 class="wb-section-title mb-0">{{ t('desktop.server.advancedConfig').toUpperCase() }}</h2>
          <button
            class="text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
            @click="advExpanded = !advExpanded"
          >
            {{ advExpanded ? t('desktop.server.collapse') : t('desktop.server.expand') }}
          </button>
        </div>
        <div v-show="advExpanded" class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] overflow-hidden">
          <div class="grid grid-cols-2 md:grid-cols-3 divide-x divide-y divide-[var(--border)]">
            <div
              v-for="field in advFields"
              :key="field.key"
              class="px-4 py-3"
            >
              <div class="flex items-center gap-1.5 mb-1.5">
                <span class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">{{ field.label }}</span>
                <span v-if="field.hint" class="text-[calc(10px*var(--ui-scale))] text-[color:color-mix(in_srgb,var(--text-tertiary)_70%,transparent)]">{{ field.hint }}</span>
              </div>
              <button
                v-if="field.type === 'toggle'"
                class="relative w-10 h-5 rounded-[4px] border transition-colors flex-shrink-0"
                :class="(advConfig as any)[field.key] ? 'bg-[var(--color-primary)] border-[var(--color-primary)]' : 'bg-[var(--bg-page)] border-[var(--border-strong)]'"
                @click="(advConfig as any)[field.key] = !(advConfig as any)[field.key]"
              >
                <span
                  class="absolute top-[3px] w-3 h-3 rounded-[2px] transition-all"
                  :class="(advConfig as any)[field.key] ? 'left-[22px] bg-[var(--color-primary-contrast)]' : 'left-[3px] bg-[var(--border-strong)]'"
                ></span>
              </button>
              <input
                v-else
                v-model.number="(advConfig as any)[field.key]"
                type="number"
                :min="field.min"
                :max="field.max"
                class="w-24 h-7 px-2 wb-mono rounded-[6px] border border-[var(--border-input)] bg-[var(--bg-input)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
              />
            </div>
          </div>
        </div>
      </section>

      <!-- ---------- SECTION: MONITORING ---------- -->
      <section>
        <h2 class="wb-section-title">{{ t('desktop.server.monitoring').toUpperCase() }}</h2>
        <div class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] p-4">
          <div class="grid grid-cols-2 md:grid-cols-6 gap-3 mb-4">
            <div v-for="m in metricRows" :key="m.key">
              <div class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] mb-1">{{ m.label }}</div>
              <div class="wb-mono text-[calc(12.5px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">{{ m.value }}</div>
            </div>
          </div>
          <div class="border-t border-[var(--border)] pt-3">
            <h3 class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] mb-2">{{ t('desktop.server.wsThroughput') }}</h3>
            <VChart :option="chartOption" style="height: 220px; width: 100%;" autoresize />
          </div>
        </div>
      </section>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 服务器管理视图 — 桌面端服务器配置与监控
 * Warm Workbench 风格：STATUS/NETWORK/CONFIG/MONITORING 分区，图表颜色跟随主题变量
 */
import { onMounted, onUnmounted, computed, watch, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useServer } from '@/composables/useServer'
import { useToast } from '@/composables/useToast'
import VChart from 'vue-echarts'
import PluginPageToolbar from '@/plugin/components/PluginPageToolbar.vue'
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

/** 首次加载未完成时显示骨架屏 */
const initializing = ref(true)

const portInput = ref(8765)

/** 后端端口刷新后同步输入框（loadStatus / updatePort 等操作完成后调用） */
function syncPortInput() {
  portInput.value = port.value
}

const statusText = computed(() => {
  switch (status.value) {
    case 'running': return t('desktop.server.status.running')
    case 'starting': return t('desktop.server.status.starting')
    default: return t('desktop.server.status.stopped')
  }
})

const dotClass = computed(() =>
  status.value === 'running' ? 'bg-green-500' : status.value === 'starting' ? 'bg-yellow-500' : 'bg-red-500'
)

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

watch(metrics, (m) => {
  if (m && status.value === 'running' && !uptimeTimer) {
    uptimeTick.value = m.uptime_secs
    uptimeTimer = setInterval(() => { uptimeTick.value++ }, 1000)
  }
})

/** 格式化运行时长 — HH:MM:SS */
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

/** 读取当前主题 CSS 变量（图表颜色跟随亮/暗模式） */
function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || '#1D1A14'
}

/** 监控指标行 — 空数据统一显示 - */
const metricRows = computed(() => [
  { key: 'uptime', label: t('desktop.server.uptime'), value: status.value === 'running' ? formatUptime(uptimeTick.value) : '-' },
  { key: 'connections', label: t('desktop.server.connections'), value: metrics.value ? String(metrics.value.connections) : '-' },
  { key: 'total', label: t('desktop.server.totalRequests'), value: metrics.value ? metrics.value.total_http_requests.toLocaleString() : '-' },
  { key: 'cpu', label: t('desktop.server.cpuUsage'), value: metrics.value ? `${metrics.value.cpu_usage_percent.toFixed(1)}%` : '-' },
  { key: 'mem', label: t('desktop.server.memoryUsage'), value: metrics.value ? formatMemory(metrics.value.memory_usage_bytes) : '-' },
  { key: 'rate', label: t('desktop.server.requestRate'), value: metrics.value ? `${metrics.value.http_requests_per_sec.toFixed(1)}/s` : '-' },
])

/** ECharts 时序图配置（颜色取自主题变量） */
const chartOption = computed(() => {
  const history = metricsHistory.value
  const times = history.map(h => {
    const m = Math.floor(h.timestamp_secs / 60)
    const s = h.timestamp_secs % 60
    return `${m}:${s.toString().padStart(2, '0')}`
  })
  const accent = cssVar('--color-primary')
  const success = cssVar('--color-success')

  return {
    tooltip: { trigger: 'axis' as const },
    legend: {
      data: [t('desktop.server.wsSentRate'), t('desktop.server.wsRecvRate')],
      top: 0,
      textStyle: { color: cssVar('--text-secondary') },
    },
    grid: { left: '3%', right: '4%', bottom: '3%', containLabel: true },
    xAxis: { type: 'category' as const, boundaryGap: false, data: times, axisLabel: { color: cssVar('--text-tertiary') } },
    yAxis: { type: 'value' as const, name: 'msg/s', axisLabel: { color: cssVar('--text-tertiary') } },
    series: [
      {
        name: t('desktop.server.wsSentRate'),
        type: 'line' as const,
        smooth: true,
        symbol: 'none',
        itemStyle: { color: accent },
        areaStyle: { color: hexToRgba(accent, 0.1) },
        data: history.map(h => h.ws_sent_rate.toFixed(2)),
      },
      {
        name: t('desktop.server.wsRecvRate'),
        type: 'line' as const,
        smooth: true,
        symbol: 'none',
        itemStyle: { color: success },
        areaStyle: { color: hexToRgba(success, 0.1) },
        data: history.map(h => h.ws_recv_rate.toFixed(2)),
      },
    ],
  }
})

/** #RRGGBB → rgba(r,g,b,a) */
function hexToRgba(hex: string, alpha: number): string {
  const m = hex.replace('#', '')
  const r = parseInt(m.slice(0, 2), 16)
  const g = parseInt(m.slice(2, 4), 16)
  const b = parseInt(m.slice(4, 6), 16)
  return `rgba(${r},${g},${b},${alpha})`
}

/** 启动服务器（先提交输入框中未应用的端口） */
async function handleStart() {
  try {
    if (portInput.value !== port.value) {
      await updatePort(portInput.value)
    }
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
    if (portInput.value !== port.value) {
      await updatePort(portInput.value)
    }
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
    // 同步服务器实际端口，避免重启后页面仍显示旧端口
    await loadStatus()
    syncPortInput()
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

/** 高级配置折叠状态 — 默认展开（工作台气质，信息直接可见） */
const advExpanded = ref(true)

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

/** 高级配置字段元数据 — 驱动网格渲染 */
const advFields = computed(() => [
  { key: 'workers', label: t('desktop.server.workers'), hint: t('desktop.server.workersHint'), type: 'number', min: 0, max: 64 },
  { key: 'keep_alive_secs', label: t('desktop.server.keepAlive'), hint: t('desktop.server.keepAliveHint'), type: 'number', min: 0, max: 300 },
  { key: 'client_request_timeout_secs', label: t('desktop.server.clientRequestTimeout'), hint: 's', type: 'number', min: 1, max: 120 },
  { key: 'client_disconnect_timeout_secs', label: t('desktop.server.clientDisconnectTimeout'), hint: 's', type: 'number', min: 1, max: 120 },
  { key: 'max_connections', label: t('desktop.server.maxConnections'), hint: t('desktop.server.maxConnectionsHint'), type: 'number', min: 1, max: 100000 },
  { key: 'backlog', label: t('desktop.server.backlog'), hint: '', type: 'number', min: 64, max: 8192 },
  { key: 'tcp_nodelay', label: t('desktop.server.tcpNodelay'), hint: '', type: 'toggle' },
  { key: 'shutdown_timeout_secs', label: t('desktop.server.shutdownTimeout'), hint: 's', type: 'number', min: 1, max: 300 },
  { key: 'ws_max_frame_size_kb', label: t('desktop.server.wsMaxFrameSize'), hint: t('desktop.server.wsMaxFrameSizeHint'), type: 'number', min: 1, max: 16384 },
  { key: 'ws_max_message_size_mb', label: t('desktop.server.wsMaxMessageSize'), hint: t('desktop.server.wsMaxMessageSizeHint'), type: 'number', min: 1, max: 512 },
])

/** 还原所有配置为默认值 */
async function handleResetDefaults() {
  try {
    const defaults = await resetNetworkConfig()
    portInput.value = defaults.port
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
  try {
    await loadStatus()
    await loadNetworkConfig()
    syncPortInput()
    if (status.value === 'running') {
      startPolling()
    }
  } finally {
    initializing.value = false
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
