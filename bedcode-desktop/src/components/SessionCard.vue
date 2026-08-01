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

    <!-- Expandable Session Management Area -->
    <div v-if="configSessions.length > 0" class="border-t border-[var(--border)]">
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
        <span>{{ $t('desktop.session.sessions', { count: configSessions.length }) }}</span>
      </div>

      <!-- Session Management List -->
      <div v-if="isExpanded" class="bg-[var(--bg-hover)]/30">
        <SessionItem
          v-for="session in configSessions"
          :key="session.id"
          :session="session"
          flat
          class="border-t border-[var(--border)] first:border-t-0"
          @view="emit('viewSession', session)"
          @stop="emit('stopSession', session)"
          @restart="emit('restartSession', session)"
          @delete="emit('deleteSession', session)"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * SessionCard - 会话配置卡片
 *
 * 卡片式设计，pill 环境标签，折叠区域展示该配置下的会话管理列表
 */
import { ref, computed, watch } from 'vue'
import type { SessionConfig, SessionInfo } from '@/stores/session'
import Button from '@/components/Button.vue'
import SessionItem from '@/components/SessionItem.vue'

const props = defineProps<{
  config: SessionConfig
  sessions: SessionInfo[]
}>()

const emit = defineEmits<{
  (e: 'start'): void
  (e: 'edit'): void
  (e: 'delete'): void
  (e: 'viewSession', session: SessionInfo): void
  (e: 'stopSession', session: SessionInfo): void
  (e: 'restartSession', session: SessionInfo): void
  (e: 'deleteSession', session: SessionInfo): void
}>()

const isExpanded = ref(false)

// 该配置下的所有会话（含已停止，支持重启/删除）
const configSessions = computed(() => {
  return props.sessions.filter(s => (s.configId || s.config_id) === props.config.id)
})

// 运行中的会话数量（用于头部状态提示）
const runningSessions = computed(() => {
  return configSessions.value.filter(s => s.status !== 'stopped')
})

// 记录已见过的会话 id，新会话启动后自动展开折叠区域
const knownSessionIds = new Set<string>()
let isInitialized = false

watch(
  () => configSessions.value,
  (list) => {
    let hasNew = false
    for (const s of list) {
      if (!knownSessionIds.has(s.id)) {
        knownSessionIds.add(s.id)
        hasNew = true
      }
    }
    // 首次挂载只记录已有会话，避免页面加载时全部展开
    if (!isInitialized) {
      isInitialized = true
      return
    }
    if (hasNew) {
      isExpanded.value = true
    }
  },
  { immediate: true }
)

function toggleExpand() {
  isExpanded.value = !isExpanded.value
}
</script>
