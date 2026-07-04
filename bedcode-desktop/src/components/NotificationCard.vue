<template>
  <div
    class="bg-card rounded-card shadow-card hover:shadow-card-hover transition-all duration-200 overflow-hidden"
    :class="{ 'border-l-4 border-l-[var(--color-primary)]': !read }"
  >
    <!-- Header: title + time + status badge -->
    <div class="flex items-center gap-3 px-5 py-4">
      <!-- Status Indicator -->
      <div
        :class="[
          'w-2 h-2 rounded-full flex-shrink-0',
          severity === 'error' ? 'bg-red-500 shadow-[0_0_6px_rgba(239,68,68,0.5)]' :
          severity === 'warning' ? 'bg-amber-500' :
          severity === 'success' ? 'bg-green-500' : 'bg-[var(--color-primary)]'
        ]"
      ></div>

      <!-- Title + Time -->
      <div class="flex-1 min-w-0">
        <div class="flex items-center gap-2">
          <h4 class="font-semibold text-[var(--text-primary)] text-sm truncate">{{ title }}</h4>
          <span
            v-if="!read"
            class="inline-flex items-center h-5 px-2 rounded-tag text-[10px] font-medium bg-[var(--color-primary-light)] text-blue-600 dark:text-blue-400 flex-shrink-0"
          >
            NEW
          </span>
        </div>
        <p class="text-[var(--text-tertiary)] text-xs mt-0.5">{{ time }}</p>
      </div>

      <!-- Severity Badge -->
      <span
        :class="[
          'inline-flex items-center h-6 px-2.5 rounded-tag text-[11px] font-medium flex-shrink-0',
          severity === 'error' ? 'bg-[var(--color-danger-light)] text-red-600 dark:text-red-400' :
          severity === 'warning' ? 'bg-[var(--color-warning-light)] text-amber-600 dark:text-amber-400' :
          severity === 'success' ? 'bg-[var(--color-success-light)] text-green-600 dark:text-green-400' :
          'bg-[var(--color-primary-light)] text-blue-600 dark:text-blue-400'
        ]"
      >
        {{ severityLabel }}
      </span>

      <!-- Actions -->
      <div class="flex items-center gap-1 flex-shrink-0" @click.stop>
        <button
          class="w-8 h-8 rounded-btn flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors duration-200"
          title="标记已读"
          @click="$emit('markRead')"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
          </svg>
        </button>
        <button
          class="w-8 h-8 rounded-btn flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--color-danger-light)] hover:text-red-600 dark:hover:text-red-400 transition-colors duration-200"
          title="删除"
          @click="$emit('dismiss')"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
    </div>

    <!-- Expandable Detail -->
    <div v-if="detail" class="border-t border-[var(--border)]">
      <button
        class="flex items-center gap-2 w-full px-5 py-2.5 text-[var(--text-secondary)] text-sm hover:bg-[var(--bg-hover)] transition-colors duration-200"
        @click="isExpanded = !isExpanded"
      >
        <svg
          :class="['w-3.5 h-3.5 transition-transform duration-200', isExpanded ? 'rotate-90' : '']"
          fill="none" stroke="currentColor" viewBox="0 0 24 24"
        >
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
        </svg>
        <span>查看详情</span>
      </button>
      <div v-if="isExpanded" class="px-5 pb-4 text-[var(--text-secondary)] text-sm leading-relaxed">
        {{ detail }}
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * NotificationCard - 通知卡片
 *
 * 桌面端通知项，支持严重级别 badge、已读/未读状态、可展开详情
 */
import { ref, computed } from 'vue'

const props = withDefaults(defineProps<{
  title: string
  time: string
  severity?: 'info' | 'success' | 'warning' | 'error'
  read?: boolean
  detail?: string
}>(), {
  severity: 'info',
  read: false,
})

defineEmits<{
  markRead: []
  dismiss: []
}>()

const isExpanded = ref(false)

const severityLabel = computed(() => {
  switch (props.severity) {
    case 'error': return '错误'
    case 'warning': return '警告'
    case 'success': return '成功'
    default: return '信息'
  }
})
</script>
