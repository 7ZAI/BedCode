<template>
  <div class="bg-dark-800 rounded-lg border border-dark-700 overflow-hidden">
    <!-- Session Header -->
    <div
      class="flex items-center gap-4 px-4 py-3 cursor-pointer hover:bg-dark-750 transition-colors"
      @click="toggleExpand"
    >
      <!-- Left: Status Indicator -->
      <div
        :class="[
          'flex-shrink-0 w-3 h-3 rounded-full',
          statusColor
        ]"
      ></div>

      <!-- Center: Session Info -->
      <div class="flex-1 min-w-0">
        <h3 class="font-medium text-white truncate">{{ session.name }}</h3>
        <p class="text-dark-400 text-sm">{{ displayTime }}</p>
      </div>

      <!-- Status Badge -->
      <span
        :class="[
          'flex-shrink-0 text-xs px-2 py-1 rounded',
          statusBadgeClass
        ]"
      >
        {{ statusText }}
      </span>

      <!-- Session Type Badge -->
      <span
        v-if="session.sessionType"
        :class="[
          'flex-shrink-0 text-xs px-2 py-0.5 rounded',
          session.sessionType === 'plugin' ? 'bg-purple-500/20 text-purple-400' : 'bg-blue-500/20 text-blue-400'
        ]"
      >
        {{ session.sessionType === 'plugin' ? 'Plugin' : 'PTY' }}
      </span>

      <!-- Right: Actions -->
      <div class="flex items-center gap-2 flex-shrink-0" @click.stop>
        <Button variant="ghost" size="sm" @click="toggleExpand">
          <svg
            :class="['w-4 h-4 transition-transform', isExpanded ? 'rotate-180' : '']"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
          </svg>
        </Button>
        <Button variant="ghost" size="sm" @click="$emit('view')">
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
          </svg>
        </Button>
        <!-- 运行中显示停止按钮，已停止显示重启按钮 -->
        <Button v-if="isRunning" variant="ghost" size="sm" @click="$emit('stop')">
          <svg class="w-4 h-4 text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 10a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1v-4z" />
          </svg>
        </Button>
        <Button v-else variant="ghost" size="sm" @click="$emit('restart')">
          <svg class="w-4 h-4 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
        </Button>
        <Button variant="ghost" size="sm" @click="$emit('delete')">
          <svg class="w-4 h-4 text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </Button>
      </div>
    </div>

    <!-- Expandable Info Area -->
    <div v-if="isExpanded" class="border-t border-dark-700 px-4 py-3 bg-dark-900">
      <div class="grid grid-cols-2 gap-4 text-sm">
        <div>
          <span class="text-dark-400">会话ID:</span>
          <span class="text-dark-300 ml-2 font-mono text-xs">{{ session.id }}</span>
        </div>
        <div>
          <span class="text-dark-400">配置ID:</span>
          <span class="text-dark-300 ml-2 font-mono text-xs">{{ session.configId }}</span>
        </div>
        <div>
          <span class="text-dark-400">创建时间:</span>
          <span class="text-dark-300 ml-2">{{ formatDateTime(session.createdAt) }}</span>
        </div>
        <div v-if="session.startedAt">
          <span class="text-dark-400">{{ isRunning ? '启动时间' : '停止时间' }}:</span>
          <span class="text-dark-300 ml-2">{{ isRunning ? formatDateTime(session.startedAt) : formatDateTime(session.stoppedAt || '') }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import type { SessionInfo } from '@/stores/session'
import Button from '@/components/common/Button.vue'

const props = defineProps<{
  session: SessionInfo
  showTerminal?: boolean
}>()

const emit = defineEmits<{
  (e: 'view'): void
  (e: 'stop'): void
  (e: 'restart'): void
  (e: 'delete'): void
}>()

const isExpanded = ref(false)

// 判断会话是否在运行
const isRunning = computed(() => {
  return props.session.status === 'running' || props.session.status === 'waitingInput' || props.session.status === 'starting'
})

// 显示的时间
const displayTime = computed(() => {
  if (isRunning.value) {
    return `运行时间: ${runTime.value}`
  } else {
    return `已停止`
  }
})

// 计算运行时间
const runTime = computed(() => {
  const start = props.session.startedAt || props.session.createdAt
  if (!start) return '--'

  const startTime = new Date(start).getTime()
  const now = Date.now()
  const diff = Math.floor((now - startTime) / 1000)

  if (diff < 60) return `${diff}秒`
  if (diff < 3600) return `${Math.floor(diff / 60)}分${diff % 60}秒`
  const hours = Math.floor(diff / 3600)
  const minutes = Math.floor((diff % 3600) / 60)
  return `${hours}小时${minutes}分`
})

// 实时更新时间
let intervalId: ReturnType<typeof setInterval> | null = null

onMounted(() => {
  intervalId = setInterval(() => {
    // 触发响应式更新 - 通过空操作保持组件活跃
  }, 1000)
})

onUnmounted(() => {
  if (intervalId) {
    clearInterval(intervalId)
  }
})

// 状态颜色
const statusColor = computed(() => {
  switch (props.session.status) {
    case 'running': return 'bg-green-500 animate-pulse'
    case 'waitingInput': return 'bg-yellow-500'
    case 'error': return 'bg-red-500'
    default: return 'bg-dark-500'
  }
})

// 状态文字
const statusText = computed(() => {
  switch (props.session.status) {
    case 'starting': return '启动中'
    case 'running': return '运行中'
    case 'waitingInput': return '等待输入'
    case 'error': return '错���'
    case 'stopped': return '已停止'
    default: return '未知'
  }
})

// 状态徽章样式
const statusBadgeClass = computed(() => {
  switch (props.session.status) {
    case 'running': return 'bg-green-900/50 text-green-300'
    case 'waitingInput': return 'bg-yellow-900/50 text-yellow-300'
    case 'error': return 'bg-red-900/50 text-red-300'
    case 'stopped': return 'bg-dark-600 text-dark-400'
    default: return 'bg-dark-600 text-dark-400'
  }
})

function toggleExpand() {
  isExpanded.value = !isExpanded.value
}

function formatDateTime(dateStr: string): string {
  if (!dateStr) return '--'
  const date = new Date(dateStr)
  return date.toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
}
</script>