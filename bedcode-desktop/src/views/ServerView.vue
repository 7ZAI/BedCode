<template>
  <div class="p-6 max-w-4xl mx-auto space-y-6 h-full overflow-y-auto">
    <!-- ==================== 区块 1：服务器配置 ==================== -->
    <div class="bg-white dark:bg-dark-800 rounded-xl border border-slate-200 dark:border-dark-700 p-6">
      <!-- 标题行：配置 + 状态 + 控制按钮 -->
      <div class="flex items-center justify-between mb-5">
        <div class="flex items-center gap-3">
          <h2 class="text-lg font-semibold text-slate-800 dark:text-white">
            {{ $t('desktop.server.config') }}
          </h2>
          <div class="flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium"
            :class="status === 'running' ? 'bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-400' : status === 'starting' ? 'bg-yellow-100 dark:bg-yellow-900/30 text-yellow-700 dark:text-yellow-400' : 'bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-400'"
          >
            <div class="w-2 h-2 rounded-full"
              :class="status === 'running' ? 'bg-green-500' : status === 'starting' ? 'bg-yellow-500' : 'bg-red-500'"
            ></div>
            {{ statusText }}
          </div>
        </div>
        <div class="flex gap-2">
          <button
            class="px-3 py-1.5 text-sm font-medium rounded-lg transition-colors"
            :class="status === 'stopped' ? 'bg-green-600 text-white hover:bg-green-700' : 'bg-slate-100 dark:bg-dark-700 text-slate-400 dark:text-dark-500 cursor-not-allowed'"
            :disabled="status !== 'stopped' || loading"
            @click="handleStart"
          >
            {{ $t('desktop.server.start') }}
          </button>
          <button
            class="px-3 py-1.5 text-sm font-medium rounded-lg transition-colors"
            :class="status === 'running' ? 'bg-red-600 text-white hover:bg-red-700' : 'bg-slate-100 dark:bg-dark-700 text-slate-400 dark:text-dark-500 cursor-not-allowed'"
            :disabled="status !== 'running' || loading"
            @click="handleStop"
          >
            {{ $t('desktop.server.stop') }}
          </button>
          <button
            class="px-3 py-1.5 text-sm font-medium rounded-lg transition-colors"
            :class="status === 'running' ? 'bg-blue-600 text-white hover:bg-blue-700' : 'bg-slate-100 dark:bg-dark-700 text-slate-400 dark:text-dark-500 cursor-not-allowed'"
            :disabled="status !== 'running' || loading"
            @click="handleRestart"
          >
            {{ $t('desktop.server.restart') }}
          </button>
        </div>
      </div>

      <!-- 配置项网格 -->
      <div class="grid grid-cols-[auto_1fr] gap-x-6 gap-y-3 items-center">
        <!-- 端口 -->
        <label class="text-sm text-slate-500 dark:text-dark-400 text-right">{{ $t('desktop.server.port') }}</label>
        <div class="flex items-center gap-2">
          <input
            v-model.number="portInput"
            type="number"
            min="1024"
            max="65535"
            class="w-28 px-2.5 py-1.5 text-sm border border-slate-200 dark:border-dark-600 rounded-lg bg-white dark:bg-dark-700 text-slate-800 dark:text-white"
          />
          <button
            class="px-2.5 py-1.5 text-xs bg-primary-600 text-white rounded-lg hover:bg-primary-700 transition-colors disabled:opacity-50"
            :disabled="loading"
            @click="handleApplyPort"
          >
            {{ $t('desktop.server.applyAndRestart') }}
          </button>
        </div>

        <!-- 本地 IP -->
        <label class="text-sm text-slate-500 dark:text-dark-400 text-right">{{ $t('desktop.server.localIp') }}</label>
        <div class="flex flex-wrap gap-x-4 gap-y-0.5">
          <span v-for="ip in localIps" :key="ip" class="text-sm text-slate-800 dark:text-white font-mono">{{ ip }}</span>
          <span v-if="localIps.length === 0" class="text-sm text-slate-400 dark:text-dark-500">-</span>
        </div>

        <!-- 自启动 -->
        <label class="text-sm text-slate-500 dark:text-dark-400 text-right">{{ $t('desktop.server.autoStart') }}</label>
        <button
          class="relative w-10 h-5 rounded-full transition-colors"
          :class="autoStart ? 'bg-primary-600' : 'bg-slate-300 dark:bg-dark-600'"
          @click="handleAutoStartToggle(!autoStart)"
        >
          <span
            class="absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full transition-transform shadow-sm"
            :class="autoStart ? 'translate-x-5' : 'translate-x-0'"
          ></span>
        </button>
      </div>
    </div>

    <!-- ==================== 区块 2：性能监控 ==================== -->
    <div class="bg-white dark:bg-dark-800 rounded-xl border border-slate-200 dark:border-dark-700 p-6">
      <h2 class="text-lg font-semibold text-slate-800 dark:text-white mb-4">
        {{ $t('desktop.server.monitoring') }}
      </h2>

      <!-- 使用 v-show 保持 DOM 存活，避免切换页面时闪变 -->
      <div v-show="status === 'running' && metrics">
        <!-- 指标卡片 -->
        <div class="grid grid-cols-2 md:grid-cols-3 gap-3 mb-6">
          <div class="bg-slate-50 dark:bg-dark-700 rounded-lg p-3">
            <div class="text-xs text-slate-500 dark:text-dark-400">{{ $t('desktop.server.uptime') }}</div>
            <div class="text-lg font-semibold text-slate-800 dark:text-white">{{ formatUptime(metrics?.uptime_secs ?? 0) }}</div>
          </div>
          <div class="bg-slate-50 dark:bg-dark-700 rounded-lg p-3">
            <div class="text-xs text-slate-500 dark:text-dark-400">{{ $t('desktop.server.connections') }}</div>
            <div class="text-lg font-semibold text-slate-800 dark:text-white">{{ metrics?.connections ?? 0 }}</div>
          </div>
          <div class="bg-slate-50 dark:bg-dark-700 rounded-lg p-3">
            <div class="text-xs text-slate-500 dark:text-dark-400">{{ $t('desktop.server.totalRequests') }}</div>
            <div class="text-lg font-semibold text-slate-800 dark:text-white">{{ (metrics?.total_http_requests ?? 0).toLocaleString() }}</div>
          </div>
          <div class="bg-slate-50 dark:bg-dark-700 rounded-lg p-3">
            <div class="text-xs text-slate-500 dark:text-dark-400">{{ $t('desktop.server.cpuUsage') }}</div>
            <div class="text-lg font-semibold text-slate-800 dark:text-white">{{ (metrics?.cpu_usage_percent ?? 0).toFixed(1) }}%</div>
          </div>
          <div class="bg-slate-50 dark:bg-dark-700 rounded-lg p-3">
            <div class="text-xs text-slate-500 dark:text-dark-400">{{ $t('desktop.server.memoryUsage') }}</div>
            <div class="text-lg font-semibold text-slate-800 dark:text-white">{{ formatMemory(metrics?.memory_usage_bytes ?? 0) }}</div>
          </div>
          <div class="bg-slate-50 dark:bg-dark-700 rounded-lg p-3">
            <div class="text-xs text-slate-500 dark:text-dark-400">{{ $t('desktop.server.requestRate') }}</div>
            <div class="text-lg font-semibold text-slate-800 dark:text-white">{{ (metrics?.http_requests_per_sec ?? 0).toFixed(1) }}/s</div>
          </div>
        </div>

        <!-- WS 消息时序图 -->
        <div>
          <h3 class="text-sm font-medium text-slate-600 dark:text-dark-300 mb-2">
            {{ $t('desktop.server.wsThroughput') }}
          </h3>
          <VChart :option="chartOption" style="height: 250px; width: 100%;" autoresize />
        </div>
      </div>

      <!-- 服务器未运行时 -->
      <div v-show="!(status === 'running' && metrics)" class="text-center py-8 text-slate-400 dark:text-dark-500">
        {{ $t('desktop.server.status.stopped') }}
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 服务器管理页面 — 配置、启停控制、性能监控
 */
import { onMounted, computed, watch } from 'vue'
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
  loadStatus,
  startServer,
  stopServer,
  restartServer,
  updatePort,
  updateAutoStart,
  startPolling,
  stopPolling,
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

/** 格式化运行时长 */
function formatUptime(secs: number): string {
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  const s = secs % 60
  if (h > 0) return `${h}h ${m}m`
  if (m > 0) return `${m}m ${s}s`
  return `${s}s`
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

/** 重启服务器 */
async function handleRestart() {
  try {
    await restartServer()
    toast.success(t('desktop.server.restartSuccess'))
    startPolling()
  } catch (e: any) {
    toast.error(e.message)
  }
}

/** 应用端口并重启 */
async function handleApplyPort() {
  try {
    await updatePort(portInput.value)
    if (status.value === 'running') {
      await restartServer()
      toast.success(t('desktop.server.restartSuccess'))
    } else {
      toast.success(t('desktop.server.portSaved'))
    }
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

onMounted(async () => {
  await loadStatus()
  if (status.value === 'running') {
    startPolling()
  }
})

watch(status, (newVal) => {
  if (newVal === 'running') {
    startPolling()
  } else {
    stopPolling()
  }
})
</script>
