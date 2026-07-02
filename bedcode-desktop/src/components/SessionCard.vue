<template>
  <div class="bg-card rounded-card shadow-card hover:shadow-card-hover transition-all duration-200 overflow-hidden">
    <!-- Config Header (always visible) -->
    <div
      class="flex items-center gap-4 px-6 py-4 cursor-pointer"
      @click="$emit('edit')"
    >
      <!-- Left: Environment Badge -->
      <span
        :class="[
          'flex-shrink-0 inline-flex items-center h-7 px-3 rounded-tag text-xs font-medium',
          config.environment === 'wsl2'
            ? 'bg-purple-50 dark:bg-purple-900/30 text-purple-600 dark:text-purple-400'
            : 'bg-[var(--color-primary-light)] text-blue-600 dark:text-blue-400'
        ]"
      >
        {{ config.environment === 'wsl2' ? 'WSL2' : 'Windows' }}
      </span>

      <!-- Center: Config Info -->
      <div class="flex-1 min-w-0">
        <h3 class="font-semibold text-[var(--text-primary)] text-sm truncate">{{ config.name }}</h3>
        <p class="text-[var(--text-secondary)] text-[13px] truncate">{{ config.workingDir }}</p>
      </div>

      <!-- Command (always visible on desktop) -->
      <div class="flex items-center gap-2 text-[var(--text-secondary)] text-sm flex-shrink-0">
        <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
        <span class="font-mono truncate max-w-32">{{ config.command }}</span>
      </div>

      <!-- Right: Actions -->
      <div class="flex items-center gap-2 flex-shrink-0" @click.stop>
        <!-- Running Sessions Count -->
        <div
          v-if="runningSessions.length > 0"
          class="flex items-center gap-1.5 mr-2 text-green-600 dark:text-green-400 text-sm"
        >
          <span class="w-2 h-2 rounded-full bg-green-500 animate-pulse"></span>
          {{ runningSessions.length }}
        </div>

        <Button variant="primary" size="sm" class="whitespace-nowrap" @click.stop="$emit('start')">
          <template #icon>
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z" />
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
          </template>
          {{ $t('common.button.start') }}
        </Button>
        <button
          class="w-9 h-9 rounded-btn flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-all duration-200"
          @click.stop="$emit('edit')"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
          </svg>
        </button>
        <button
          class="w-9 h-9 rounded-btn flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--color-danger-light)] hover:text-red-600 dark:hover:text-red-400 transition-all duration-200"
          @click.stop="$emit('delete')"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </button>
      </div>
    </div>

    <!-- Expandable Running Sessions -->
    <div v-if="runningSessions.length > 0" class="border-t border-[var(--border)]">
      <div
        class="flex items-center gap-2 px-6 py-2.5 cursor-pointer text-[var(--text-secondary)] text-sm hover:bg-[var(--bg-hover)] transition-colors duration-200"
        @click.stop="toggleExpand"
      >
        <svg
          :class="['w-4 h-4 transition-transform', isExpanded ? 'rotate-90' : '']"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
        </svg>
        <span>{{ $t('desktop.session.runningSessions', { count: runningSessions.length }) }}</span>
      </div>

      <!-- Running Sessions List -->
      <div v-if="isExpanded" class="bg-[var(--bg-hover)]/30">
        <div
          v-for="session in runningSessions"
          :key="session.id"
          class="flex items-center justify-between px-6 py-3 border-t border-[var(--border)] first:border-t-0 hover:bg-[var(--bg-hover)] cursor-pointer transition-colors duration-200"
          @click="$emit('viewSession', session)"
        >
          <div class="flex items-center gap-3">
            <!-- Status Indicator -->
            <div
              :class="[
                'w-2 h-2 rounded-full',
                session.status === 'running' ? 'bg-green-500' :
                session.status === 'waitingInput' ? 'bg-amber-500' :
                session.status === 'error' ? 'bg-red-500' : 'bg-[var(--text-tertiary)]'
              ]"
            ></div>
            <span class="text-[var(--text-primary)] text-sm">{{ session.name }}</span>
            <span
              v-if="session.sessionType"
              class="inline-flex items-center h-6 px-2 rounded-tag text-[11px] font-medium bg-[var(--color-primary-light)] text-blue-600 dark:text-blue-400"
            >
              PTY
            </span>
            <span
              v-if="session.taskStatus"
              :class="[
                'inline-flex items-center h-6 px-2 rounded-tag text-[11px] font-medium',
                taskStatusBadgeClass(session.taskStatus)
              ]"
            >
              {{ taskStatusText(session.taskStatus) }}
            </span>
          </div>
          <div class="flex items-center gap-2">
            <span class="text-[var(--text-tertiary)] text-sm">{{ getSessionTime(session) }}</span>
            <button
              class="w-8 h-8 rounded-btn flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--color-danger-light)] hover:text-red-600 dark:hover:text-red-400 transition-all duration-200"
              @click.stop="$emit('stopSession', session.id)"
            >
              <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 10a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1v-4z" />
              </svg>
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * SessionCard - 会话配置卡片
 *
 * 卡片式设计，pill 环境标签，运行中会话展开列表
 */
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { SessionConfig, SessionInfo } from '@/stores/session'
import Button from '@/components/Button.vue'

const { t } = useI18n()

const props = defineProps<{
  config: SessionConfig
  sessions: SessionInfo[]
}>()

defineEmits<{
  (e: 'start'): void
  (e: 'edit'): void
  (e: 'delete'): void
  (e: 'viewSession', session: SessionInfo): void
  (e: 'stopSession', sessionId: string): void
}>()

const isExpanded = ref(false)

// 筛选出该配置下运行中的会话
const runningSessions = computed(() => {
  return props.sessions.filter(s =>
    s.configId === props.config.id &&
    s.status !== 'stopped'
  )
})

function toggleExpand() {
  isExpanded.value = !isExpanded.value
}

function taskStatusText(status: string): string {
  switch (status) {
    case 'idle': return t('common.status.idle')
    case 'in_progress': return t('common.status.inProgress')
    case 'asking': return t('common.status.asking')
    case 'completed': return t('common.status.completed')
    case 'interrupted': return t('common.status.interrupted')
    default: return status
  }
}

function taskStatusBadgeClass(status: string): string {
  switch (status) {
    case 'idle': return 'bg-[var(--bg-hover)] text-[var(--text-secondary)]'
    case 'in_progress': return 'bg-[var(--color-primary-light)] text-blue-600 dark:text-blue-400'
    case 'asking': return 'bg-[var(--color-warning-light)] text-amber-600 dark:text-amber-400'
    case 'completed': return 'bg-[var(--color-success-light)] text-green-600 dark:text-green-400'
    case 'interrupted': return 'bg-[var(--color-danger-light)] text-red-600 dark:text-red-400'
    default: return 'bg-[var(--bg-hover)] text-[var(--text-secondary)]'
  }
}

function getSessionTime(session: SessionInfo): string {
  const start = session.startedAt || session.createdAt
  if (!start) return '--'

  const startTime = new Date(start).getTime()
  const now = Date.now()
  const diff = Math.floor((now - startTime) / 1000)

  if (diff < 60) return t('common.time.secondsAgo', { n: diff })
  if (diff < 3600) return t('common.time.minutesSecondsAgo', { m: Math.floor(diff / 60), s: diff % 60 })
  const hours = Math.floor(diff / 3600)
  const minutes = Math.floor((diff % 3600) / 60)
  return t('common.time.hoursMinutesAgo', { h: hours, m: minutes })
}
</script>
