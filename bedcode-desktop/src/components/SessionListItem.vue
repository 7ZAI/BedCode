<template>
  <!-- Desktop variant -->
  <div
    v-if="isDesktop"
    class="flex items-center gap-3 px-4 py-3 bg-card rounded-card hover:bg-[var(--bg-hover)] transition-colors duration-200 cursor-pointer"
    @click="$emit('click')"
  >
    <!-- Status Dot -->
    <div
      :class="[
        'w-2 h-2 rounded-full flex-shrink-0',
        status === 'running' ? 'bg-green-500 animate-pulse' :
        status === 'waitingInput' ? 'bg-amber-500' :
        status === 'error' ? 'bg-red-500' : 'bg-[var(--text-tertiary)]'
      ]"
    ></div>

    <!-- Session Name -->
    <div class="flex-1 min-w-0">
      <p class="text-[var(--text-primary)] text-sm font-medium truncate">{{ name }}</p>
      <p class="text-[var(--text-tertiary)] text-xs truncate">{{ workingDir }}</p>
    </div>

    <!-- Type Badge -->
    <span
      v-if="sessionType"
      class="inline-flex items-center h-6 px-2 rounded-tag text-[11px] font-medium bg-[var(--color-primary-light)] text-blue-600 dark:text-blue-400 flex-shrink-0"
    >
      {{ sessionType }}
    </span>

    <!-- Task Status Badge -->
    <span
      v-if="taskStatus"
      :class="[
        'inline-flex items-center h-6 px-2 rounded-tag text-[11px] font-medium flex-shrink-0',
        taskStatusBadgeClass
      ]"
    >
      {{ taskStatusLabel }}
    </span>

    <!-- Duration -->
    <span class="text-[var(--text-tertiary)] text-xs flex-shrink-0 tabular-nums">{{ duration }}</span>

    <!-- Stop Button -->
    <button
      v-if="status === 'running' || status === 'waitingInput'"
      class="w-8 h-8 rounded-btn flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--color-danger-light)] hover:text-red-600 dark:hover:text-red-400 transition-colors duration-200 flex-shrink-0"
      title="停止会话"
      @click.stop="$emit('stop')"
    >
      <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 10a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1v-4z" />
      </svg>
    </button>
  </div>

  <!-- Mobile variant -->
  <div
    v-else
    class="flex items-center gap-3 px-4 py-3 bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl active:opacity-80 transition-all duration-200 cursor-pointer"
    @click="$emit('click')"
  >
    <!-- Status Dot -->
    <div
      :class="[
        'w-2.5 h-2.5 rounded-full flex-shrink-0',
        status === 'running' ? 'bg-[var(--mobile-success)] shadow-[0_0_8px_rgba(16,185,129,0.5)] animate-pulse' :
        status === 'waitingInput' ? 'bg-[var(--mobile-warning)]' :
        status === 'error' ? 'bg-[var(--mobile-error)]' : 'bg-[var(--mobile-text-muted)]'
      ]"
    ></div>

    <!-- Session Info -->
    <div class="flex-1 min-w-0">
      <p class="text-[var(--mobile-text-primary)] text-sm font-medium truncate">{{ name }}</p>
      <div class="flex items-center gap-2 mt-0.5">
        <p class="text-[var(--mobile-text-muted)] text-xs truncate">{{ workingDir }}</p>
        <span
          v-if="sessionType"
          class="inline-flex items-center h-5 px-1.5 rounded-tag text-[10px] font-medium bg-[var(--mobile-accent-muted)] text-[var(--mobile-accent)] flex-shrink-0"
        >
          {{ sessionType }}
        </span>
      </div>
    </div>

    <!-- Duration -->
    <span class="text-[var(--mobile-text-muted)] text-xs flex-shrink-0 tabular-nums">{{ duration }}</span>

    <!-- Arrow -->
    <svg class="w-4 h-4 text-[var(--mobile-text-disabled)] flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
    </svg>
  </div>
</template>

<script setup lang="ts">
/**
 * SessionListItem - 跨平台会话列表项
 *
 * 桌面端：紧凑行式布局，含操作按钮
 * 移动端：卡片式布局，含发光状态点和箭头
 * 通过 usePlatform 切换变体，不使用媒体查询
 */
import { computed } from 'vue'
import { usePlatform } from '@/composables/usePlatform'

const { platformInfo } = usePlatform()

const props = withDefaults(defineProps<{
  name: string
  workingDir: string
  status: 'running' | 'waitingInput' | 'stopped' | 'error'
  sessionType?: string
  taskStatus?: string
  duration: string
}>(), {
  sessionType: '',
  taskStatus: '',
})

defineEmits<{
  click: []
  stop: []
}>()

const isDesktop = computed(() => platformInfo.value?.isDesktop ?? true)

const taskStatusLabel = computed(() => {
  switch (props.taskStatus) {
    case 'idle': return '空闲'
    case 'in_progress': return '进行中'
    case 'asking': return '等待输入'
    case 'completed': return '已完成'
    case 'interrupted': return '已中断'
    default: return props.taskStatus
  }
})

const taskStatusBadgeClass = computed(() => {
  switch (props.taskStatus) {
    case 'idle': return 'bg-[var(--bg-hover)] text-[var(--text-secondary)]'
    case 'in_progress': return 'bg-[var(--color-primary-light)] text-blue-600 dark:text-blue-400'
    case 'asking': return 'bg-[var(--color-warning-light)] text-amber-600 dark:text-amber-400'
    case 'completed': return 'bg-[var(--color-success-light)] text-green-600 dark:text-green-400'
    case 'interrupted': return 'bg-[var(--color-danger-light)] text-red-600 dark:text-red-400'
    default: return 'bg-[var(--bg-hover)] text-[var(--text-secondary)]'
  }
})
</script>
