<template>
  <!-- Mobile variant -->
  <div
    class="flex items-center gap-3 px-4 py-3 bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl active:opacity-80 transition-colors duration-200 cursor-pointer"
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
      <p class="text-[var(--mobile-text-primary)] text-base font-medium truncate">{{ name }}</p>
      <div class="flex items-center gap-2 mt-0.5">
        <p class="text-[var(--mobile-text-muted)] text-xs truncate">{{ workingDir }}</p>
        <span
          v-if="sessionType"
          class="inline-flex items-center h-5 px-1.5 rounded-tag text-xs font-medium bg-[var(--mobile-accent-muted)] text-[var(--mobile-accent)] flex-shrink-0"
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
 * SessionListItem - 移动端会话列表项（卡片式布局，含发光状态点和箭头）
 */
defineProps<{
  name: string
  workingDir: string
  status: 'running' | 'waitingInput' | 'stopped' | 'error'
  sessionType?: string
  duration: string
}>()

defineEmits<{
  click: []
}>()
</script>
