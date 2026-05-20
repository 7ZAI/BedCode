<template>
  <div
    class="group bg-white dark:bg-dark-800 rounded-2xl p-4 transition-all duration-200 hover:shadow-lg active:scale-[0.98] cursor-pointer border border-gray-100 dark:border-dark-700"
    :class="[
      session.status === 'stopped' ? 'opacity-60' : '',
      isHovered ? 'shadow-md border-primary-200 dark:border-primary-800' : ''
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
          <p class="font-semibold text-gray-900 dark:text-dark-100 truncate text-base">
            {{ session.name }}
          </p>
          <span
            :class="[
              'text-xs px-2.5 py-1 rounded-full font-medium shrink-0',
              statusConfig.badgeClass
            ]"
          >
            {{ statusConfig.label }}
          </span>
        </div>

        <div class="flex items-center gap-3 mt-2">
          <!-- Time elapsed -->
          <div class="flex items-center gap-1.5 text-gray-500 dark:text-dark-400">
            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
            <span class="text-xs font-medium">{{ elapsed }}</span>
          </div>

          <!-- Session Type -->
          <div v-if="sessionType" class="flex items-center gap-1.5 text-gray-400 dark:text-dark-500">
            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
            </svg>
            <span class="text-xs">{{ sessionType }}</span>
          </div>
        </div>
      </div>

      <!-- Action Button -->
      <button
        v-if="session.status !== 'stopped'"
        class="w-9 h-9 rounded-xl bg-red-50 dark:bg-red-900/20 flex items-center justify-center transition-all hover:bg-red-100 dark:hover:bg-red-900/30 active:scale-90"
        :class="[
          session.status === 'running' ? 'text-red-500' : 'text-yellow-500'
        ]"
        @click.stop="$emit('stop')"
        title="停止会话"
      >
        <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
          <rect x="6" y="6" width="12" height="12" rx="2" />
        </svg>
      </button>
      <button
        v-else
        class="w-9 h-9 rounded-xl bg-gray-100 dark:bg-dark-700 flex items-center justify-center transition-all hover:bg-gray-200 dark:hover:bg-dark-600 active:scale-90 text-gray-400 dark:text-dark-400"
        @click.stop="$emit('delete')"
        title="删除会话"
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
import type { RemoteSession } from '@/modules/mobile/composables/useMobileCommands'
import { useRunTime } from '@/modules/shared/composables/useRunTime'

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
        bgClass: 'bg-green-50 dark:bg-green-900/30',
        iconClass: 'text-green-500',
        badgeClass: 'bg-green-100 dark:bg-green-900/50 text-green-700 dark:text-green-400',
        label: '运行中',
        icon: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
          h('path', { 'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'stroke-width': '2', d: 'M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z' }),
          h('path', { 'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'stroke-width': '2', d: 'M21 12a9 9 0 11-18 0 9 9 0 0118 0z' })
        ])
      }
    case 'waiting_input':
      return {
        bgClass: 'bg-yellow-50 dark:bg-yellow-900/30',
        iconClass: 'text-yellow-500',
        badgeClass: 'bg-yellow-100 dark:bg-yellow-900/50 text-yellow-700 dark:text-yellow-400',
        label: '等待输入',
        icon: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
          h('path', { 'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'stroke-width': '2', d: 'M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z' })
        ])
      }
    default:
      return {
        bgClass: 'bg-gray-100 dark:bg-dark-700',
        iconClass: 'text-gray-400 dark:text-dark-400',
        badgeClass: 'bg-gray-100 dark:bg-dark-700 text-gray-500 dark:text-dark-400',
        label: '已停止',
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
</script>