<template>
  <div class="bg-card rounded-card shadow-card hover:shadow-card-hover transition-all duration-200 overflow-hidden">
    <!-- Session Header -->
    <div
      class="flex items-center gap-4 px-6 py-5 cursor-pointer"
      @click="toggleExpand"
    >
      <!-- Left: Status Indicator -->
      <div
        :class="[
          'flex-shrink-0 w-2.5 h-2.5 rounded-full',
          statusColor
        ]"
      ></div>

      <!-- Center: Session Info -->
      <div class="flex-1 min-w-0">
        <h3 class="font-semibold text-[var(--text-primary)] text-sm truncate">{{ session.name }}</h3>
        <p class="text-[var(--text-secondary)] text-[13px] mt-0.5">{{ displayTime }}</p>
      </div>

      <!-- Status Tag -->
      <span
        :class="[
          'flex-shrink-0 inline-flex items-center gap-1.5 h-7 px-3 rounded-tag text-xs font-medium',
          statusTagClass
        ]"
      >
        <span v-if="session.status === 'running'" class="w-1.5 h-1.5 rounded-full bg-green-500"></span>
        {{ statusText }}
      </span>

      <!-- Session Type Tag -->
      <span
        v-if="session.sessionType"
        class="flex-shrink-0 inline-flex items-center h-7 px-3 rounded-tag text-xs font-medium bg-[var(--color-primary-light)] text-blue-600 dark:text-blue-400"
      >
        PTY
      </span>

      <!-- Right: Actions -->
      <div class="flex items-center gap-1 flex-shrink-0 ml-2" @click.stop>
        <button
          class="w-9 h-9 rounded-btn flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-all duration-200"
          :title="$t('desktop.terminal.expandDetail')"
          @click="toggleExpand"
        >
          <svg
            :class="['w-4 h-4 transition-transform', isExpanded ? 'rotate-180' : '']"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
          </svg>
        </button>
        <button
          class="w-9 h-9 rounded-btn flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-all duration-200"
          :title="$t('desktop.terminal.viewTerminal')"
          @click="handleView"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
          </svg>
        </button>
        <!-- 运行中显示停止按钮，已停止显示重启按钮 -->
        <button
          v-if="isRunning"
          class="w-9 h-9 rounded-btn flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--color-danger-light)] hover:text-red-600 dark:hover:text-red-400 transition-all duration-200"
          :title="$t('desktop.terminal.stopSession')"
          @click="$emit('stop')"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 10a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1v-4z" />
          </svg>
        </button>
        <button
          v-else
          class="w-9 h-9 rounded-btn flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--color-success-light)] hover:text-green-600 dark:hover:text-green-400 transition-all duration-200"
          :title="$t('desktop.terminal.restartSession')"
          @click="$emit('restart')"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
        </button>
        <button
          class="w-9 h-9 rounded-btn flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--color-danger-light)] hover:text-red-600 dark:hover:text-red-400 transition-all duration-200"
          :title="$t('desktop.terminal.deleteSession')"
          @click="$emit('delete')"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </button>
      </div>
    </div>

    <!-- Expandable Info Area -->
    <div v-if="isExpanded" class="border-t border-[var(--border)] px-6 py-4 bg-[var(--bg-hover)]/30">
      <div class="grid grid-cols-2 gap-4 text-sm">
        <div>
          <span class="text-[var(--text-secondary)]">{{ t('desktop.session.sessionId') }}</span>
          <span class="text-[var(--text-primary)] ml-2 font-mono text-xs">{{ session.id }}</span>
        </div>
        <div>
          <span class="text-[var(--text-secondary)]">{{ t('desktop.session.configId') }}</span>
          <span class="text-[var(--text-primary)] ml-2 font-mono text-xs">{{ session.configId }}</span>
        </div>
        <div>
          <span class="text-[var(--text-secondary)]">{{ t('desktop.session.createdAt') }}</span>
          <span class="text-[var(--text-primary)] ml-2">{{ formatDateTime(session.createdAt || session.created_at || '') }}</span>
        </div>
        <div v-if="session.startedAt">
          <span class="text-[var(--text-secondary)]">{{ isRunning ? t('desktop.session.startTime') : t('desktop.session.stopTime') }}</span>
          <span class="text-[var(--text-primary)] ml-2">{{ isRunning ? formatDateTime(session.startedAt) : formatDateTime(session.stoppedAt || '') }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * SessionItem - 运行中会话卡片
 *
 * 卡片式设计，pill 状态标签，36x36 操作按钮
 */
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { SessionInfo } from '@/stores/session'
import { useSessionWindows } from '@/composables/useSessionWindows'
import { useRunTime } from '@/composables/useRunTime'

const { t } = useI18n()

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

const { openTerminalWindow } = useSessionWindows()
const isExpanded = ref(false)

// 判断会话是否在运行
const isRunning = computed(() => {
  return props.session.status === 'running' || props.session.status === 'waitingInput' || props.session.status === 'starting'
})

// 使用 useRunTime composable 实现每秒更新
const { runTime: runTimeValue } = useRunTime(
  () => props.session.startedAt || props.session.createdAt,
  isRunning
)

// 显示的时间
const displayTime = computed(() => {
  if (isRunning.value) {
    return t('desktop.session.runTime', { time: runTimeValue.value })
  } else {
    return t('common.status.stopped')
  }
})

// 状态颜色
const statusColor = computed(() => {
  switch (props.session.status) {
    case 'running': return 'bg-green-500 animate-pulse'
    case 'waitingInput': return 'bg-amber-500'
    case 'error': return 'bg-red-500'
    default: return 'bg-[var(--text-tertiary)]'
  }
})

// 状态文字
const statusText = computed(() => {
  switch (props.session.status) {
    case 'starting': return t('common.status.starting')
    case 'running': return t('common.status.running')
    case 'waitingInput': return t('common.status.asking')
    case 'error': return t('common.status.error')
    case 'stopped': return t('common.status.stopped')
    default: return t('common.status.unknown')
  }
})

// 状态标签样式 — pill 形状，浅色背景 + 饱和文字
const statusTagClass = computed(() => {
  switch (props.session.status) {
    case 'running': return 'bg-[var(--color-success-light)] text-green-600 dark:text-green-400'
    case 'waitingInput': return 'bg-[var(--color-warning-light)] text-amber-600 dark:text-amber-400'
    case 'error': return 'bg-[var(--color-danger-light)] text-red-600 dark:text-red-400'
    case 'stopped': return 'bg-[var(--bg-hover)] text-[var(--text-secondary)]'
    default: return 'bg-[var(--bg-hover)] text-[var(--text-secondary)]'
  }
})

function toggleExpand() {
  isExpanded.value = !isExpanded.value
}

function handleView() {
  // 打开独立终端窗口
  openTerminalWindow(props.session)
  // 保留原有事件
  emit('view')
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
