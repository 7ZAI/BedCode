<template>
  <div
    class="group bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl p-4 shadow-[var(--mobile-card-shadow)] transition-all duration-300 hover:border-cyan-500/30 hover:shadow-[var(--mobile-card-shadow-hover)] active:scale-[0.98] cursor-pointer"
    :class="[
      session.status === 'stopped' ? 'opacity-60' : '',
      isHovered ? 'shadow-[0_0_15px_rgba(34,211,238,0.08)] border-cyan-500/20' : ''
    ]"
    @click="$emit('click')"
    @mouseenter="isHovered = true"
    @mouseleave="isHovered = false"
  >
    <div class="flex items-start gap-3">
      <!-- Status Icon -->
      <div
        :class="[
          'w-11 h-11 rounded-xl flex items-center justify-center shrink-0 transition-colors',
          statusConfig.bgClass
        ]"
      >
        <component :is="statusConfig.icon" class="w-5 h-5" :class="statusConfig.iconClass" />
      </div>

      <!-- Content -->
      <div class="flex-1 min-w-0">
        <div class="flex items-center justify-between gap-2">
          <p class="font-semibold text-[var(--mobile-text-primary)] truncate text-base">
            {{ session.name }}
          </p>
          <span
            :class="[
              'text-xs px-2.5 py-1 rounded-full font-medium shrink-0 border',
              statusConfig.badgeClass
            ]"
          >
            {{ statusConfig.label }}
          </span>
        </div>

        <div class="flex items-center gap-3 mt-2">
          <!-- Time elapsed -->
          <div class="flex items-center gap-1.5 text-[var(--mobile-text-muted)]">
            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
            <span class="text-xs font-medium">{{ elapsed }}</span>
          </div>

          <!-- Session Type -->
          <div v-if="sessionType" class="flex items-center gap-1.5 text-[var(--mobile-text-muted)]">
            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
            </svg>
            <span class="text-xs">{{ sessionType }}</span>
          </div>

          <!-- Task Status (Plugin sessions) -->
          <div v-if="taskStatusLabel" class="flex items-center gap-1.5">
            <span
              :class="[
                'text-xs px-2 py-0.5 rounded-full font-medium',
                taskStatusBadgeClass
              ]"
            >
              {{ taskStatusLabel }}
            </span>
          </div>
        </div>
      </div>

      <!-- Action Button -->
      <button
        v-if="session.status !== 'stopped'"
        class="w-9 h-9 rounded-xl bg-red-500/10 border border-red-500/20 flex items-center justify-center transition-all hover:bg-red-500/20 active:scale-90"
        :class="[
          session.status === 'running' ? 'text-red-400' : 'text-yellow-400'
        ]"
        @click.stop="$emit('stop')"
        :title="t('mobile.sessionCard.stopSession')"
      >
        <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
          <rect x="6" y="6" width="12" height="12" rx="2" />
        </svg>
      </button>
      <button
        v-else-if="session.status === 'stopped'"
        class="w-9 h-9 rounded-xl bg-[var(--mobile-bg-elevated)] border border-[var(--mobile-border)] flex items-center justify-center transition-all hover:bg-[var(--mobile-accent-muted)] hover:border-[var(--mobile-accent)] active:scale-90 text-[var(--mobile-text-muted)]"
        @click.stop="$emit('delete')"
        :title="t('mobile.sessionCard.deleteSession')"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
        </svg>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, h } from 'vue'
import { useI18n } from 'vue-i18n'
import type { RemoteSession } from '@/modules/mobile/composables/useMobileCommands'
import { useRunTime } from '@/modules/shared/composables/useRunTime'

const { t } = useI18n()

const props = defineProps<{
  session: RemoteSession
}>()

defineEmits<{
  click: []
  stop: []
  delete: []
}>()

const isHovered = ref(false)

// 判断是否在运行
const isRunning = computed(() => {
  return props.session.status === 'running' || props.session.status === 'waiting_input'
})

// 使用 useRunTime composable 实现每秒更新
const { runTime: elapsed } = useRunTime(
  () => props.session.startedAt || props.session.createdAt,
  isRunning
)

const statusConfig = computed(() => {
  switch (props.session.status) {
    case 'running':
      return {
        bgClass: 'bg-[var(--mobile-success-muted)] border border-[var(--mobile-success-muted)]',
        iconClass: 'text-[var(--mobile-success)]',
        badgeClass: 'bg-[var(--mobile-success-muted)] border border-[var(--mobile-success-muted)] text-[var(--mobile-success)]',
        label: t('mobile.sessionCard.running'),
        icon: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
          h('path', { 'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'stroke-width': '2', d: 'M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z' }),
          h('path', { 'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'stroke-width': '2', d: 'M21 12a9 9 0 11-18 0 9 9 0 0118 0z' })
        ])
      }
    case 'waiting_input':
      return {
        bgClass: 'bg-[var(--mobile-warning-muted)] border border-[var(--mobile-warning-muted)]',
        iconClass: 'text-[var(--mobile-warning)]',
        badgeClass: 'bg-[var(--mobile-warning-muted)] border border-[var(--mobile-warning-muted)] text-[var(--mobile-warning)]',
        label: t('mobile.sessionCard.waitingInput'),
        icon: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
          h('path', { 'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'stroke-width': '2', d: 'M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z' })
        ])
      }
    default:
      return {
        bgClass: 'bg-[var(--mobile-bg-elevated)] border border-[var(--mobile-border)]',
        iconClass: 'text-[var(--mobile-text-muted)]',
        badgeClass: 'bg-[var(--mobile-bg-elevated)] border border-[var(--mobile-border)] text-[var(--mobile-text-muted)]',
        label: t('mobile.sessionCard.stopped'),
        icon: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
          h('path', { 'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'stroke-width': '2', d: 'M18.364 18.364A9 9 0 005.636 5.636m12.728 12.728A9 9 0 015.636 5.636m12.728 12.728L5.636 5.636' })
        ])
      }
  }
})

const sessionType = computed(() => {
  const type = props.session.sessionType
  return type === 'plugin' ? 'Plugin' : type === 'pty' ? 'PTY' : null
})

const taskStatusLabel = computed(() => {
  const status = props.session.taskStatus
  if (!status) return null
  switch (status) {
    case 'idle': return t('mobile.sessionCard.taskIdle')
    case 'in_progress': return t('mobile.sessionCard.taskInProgress')
    case 'asking': return t('mobile.sessionCard.taskAsking')
    case 'completed': return t('mobile.sessionCard.taskCompleted')
    case 'interrupted': return t('mobile.sessionCard.taskInterrupted')
    default: return status
  }
})

const taskStatusBadgeClass = computed(() => {
  const status = props.session.taskStatus
  if (!status) return ''
  switch (status) {
    case 'idle': return 'bg-gray-500/20 text-gray-400'
    case 'in_progress': return 'bg-blue-500/20 text-blue-400'
    case 'asking': return 'bg-yellow-500/20 text-yellow-400'
    case 'completed': return 'bg-green-500/20 text-green-400'
    case 'interrupted': return 'bg-red-500/20 text-red-400'
    default: return 'bg-gray-500/20 text-gray-400'
  }
})
</script>