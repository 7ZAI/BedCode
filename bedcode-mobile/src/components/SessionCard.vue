<template>
  <div
    class="group-row cursor-pointer transition-colors"
    :class="[session.status === 'stopped' ? 'opacity-60' : '']"
    @click="$emit('click')"
  >
    <span class="icon-chip" :class="statusChipClass">
      <component :is="statusIcon" class="w-5 h-5" />
    </span>

    <div class="flex-1 min-w-0">
      <div class="group-row-title truncate">{{ session.name }}</div>
      <div class="group-row-sub mt-0.5 flex items-center gap-2">
        <span v-if="sessionType">{{ sessionType }}</span>
        <span class="font-mono" style="color: var(--mobile-row-sub)">{{ elapsed }}</span>
        <span v-if="taskStatusLabel" style="color: var(--mobile-chip-amber)">{{ taskStatusLabel }}</span>
      </div>
    </div>

    <span class="status-badge" :class="statusBadgeClass">
      <span v-if="session.status === 'running'" class="status-dot dot-emerald"></span>
      {{ statusLabel }}
    </span>

    <button
      v-if="session.status !== 'stopped' && subscribeAvailable"
      class="ml-1 w-11 h-11 rounded-lg flex items-center justify-center active:opacity-80 transition-colors flex-shrink-0"
      :style="subscriptionBtnStyle"
      @click.stop="$emit('toggle-subscribe')"
      :title="subscriptionPaused ? t('mobile.sessionCard.resumeSubscription') : t('mobile.sessionCard.pauseSubscription')"
    >
      <!-- 眼睛：订阅中；暂停时叠加斜线（桌面端可接管尺寸） -->
      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
        <path v-if="subscriptionPaused" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4l16 16" />
      </svg>
    </button>
    <button
      v-if="session.status !== 'stopped'"
      class="ml-1 w-11 h-11 rounded-lg flex items-center justify-center active:opacity-80 transition-colors flex-shrink-0"
      style="background: color-mix(in srgb, var(--mobile-chip-red) 16%, transparent); color: var(--mobile-chip-red); border: 1px solid color-mix(in srgb, var(--mobile-chip-red) 35%, transparent)"
      @click.stop="$emit('stop')"
      :title="t('mobile.sessionCard.stopSession')"
    >
      <svg class="w-2.5 h-2.5" fill="currentColor" viewBox="0 0 24 24">
        <rect x="6" y="6" width="12" height="12" rx="2" />
      </svg>
    </button>
    <button
      v-else
      class="ml-1 w-11 h-11 rounded-lg flex items-center justify-center active:opacity-80 transition-colors flex-shrink-0"
      style="background: var(--mobile-chip-zinc-bg); color: var(--mobile-chip-zinc)"
      @click.stop="$emit('delete')"
      :title="t('mobile.sessionCard.deleteSession')"
    >
      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
      </svg>
    </button>
  </div>
</template>

<script setup lang="ts">
import { computed, h } from 'vue'
import { useI18n } from 'vue-i18n'
import type { RemoteSession } from '@/composables/useMobileCommands'
import { useRunTime } from '@/composables/useRunTime'
import { useTerminalBufferStore } from '@/stores/terminalBuffer'

const { t } = useI18n()
const terminalBuffer = useTerminalBufferStore()

const props = defineProps<{
  session: RemoteSession
  /** 是否显示订阅开关（mock 会话无真实订阅，隐藏） */
  subscribeAvailable?: boolean
}>()

defineEmits<{
  click: []
  stop: []
  delete: []
  'toggle-subscribe': []
}>()

/** 订阅是否被手动暂停（会话卡片操作）——尺寸控制权已让给桌面端 */
const subscriptionPaused = computed(() => {
  const buffer = terminalBuffer.getBuffer(props.session.id)
  return buffer?.manuallyPaused ?? false
})

/** 订阅开关配色：暂停中 amber 高亮（有状态），订阅中中性色；
 * 全部走既有 chip token（--mobile-chip-amber* / --mobile-chip-zinc*），
 * 不另造百分比避免主题下偏色 */
const subscriptionBtnStyle = computed(() => {
  if (subscriptionPaused.value) {
    return {
      background: 'var(--mobile-chip-amber-bg)',
      color: 'var(--mobile-chip-amber)',
    }
  }
  return {
    background: 'var(--mobile-chip-zinc-bg)',
    color: 'var(--mobile-chip-zinc)',
  }
})

const isRunning = computed(() => {
  return props.session.status === 'running' || props.session.status === 'waiting_input'
})

const { runTime: elapsed } = useRunTime(
  () => props.session.startedAt || props.session.createdAt,
  isRunning
)

const statusChipClass = computed(() => {
  switch (props.session.status) {
    case 'running': return 'chip-emerald'
    case 'waiting_input': return 'chip-amber'
    default: return 'chip-zinc'
  }
})

const statusIcon = computed(() => {
  switch (props.session.status) {
    case 'running':
      return () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
        h('path', { 'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'stroke-width': '2', d: 'M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z' }),
      ])
    case 'waiting_input':
      return () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
        h('path', { 'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'stroke-width': '2', d: 'M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z' }),
      ])
    default:
      return () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
        h('path', { 'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'stroke-width': '2', d: 'M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z' }),
      ])
  }
})

const statusBadgeClass = computed(() => {
  switch (props.session.status) {
    case 'running': return 'badge-emerald'
    case 'waiting_input': return 'badge-amber'
    default: return 'badge-zinc'
  }
})

const statusLabel = computed(() => {
  switch (props.session.status) {
    case 'running': return t('mobile.sessionCard.running')
    case 'waiting_input': return t('mobile.sessionCard.waitingInput')
    default: return t('mobile.sessionCard.stopped')
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
</script>
