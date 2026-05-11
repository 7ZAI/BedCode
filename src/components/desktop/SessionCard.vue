<template>
  <div :class="['rounded-lg border overflow-hidden', { 'border-primary-500': hasRunningSessions }, 'bg-white dark:bg-dark-800 border-gray-200 dark:border-dark-700']">
    <!-- Config Header (always visible) -->
    <div
      :class="[
        'flex items-center gap-4 px-4 py-3 cursor-pointer transition-colors',
        hasRunningSessions ? 'bg-gray-50 dark:bg-dark-750' : 'hover:bg-gray-50 dark:hover:bg-dark-750'
      ]"
      @click="$emit('edit')"
    >
      <!-- Left: Environment Badge -->
      <span
        :class="[
          'flex-shrink-0 px-2 py-1 rounded text-xs font-medium',
          config.environment === 'wsl2'
            ? 'bg-purple-100 dark:bg-purple-900/50 text-purple-700 dark:text-purple-300 border border-purple-200 dark:border-purple-700'
            : 'bg-blue-100 dark:bg-blue-900/50 text-blue-700 dark:text-blue-300 border border-blue-200 dark:border-blue-700'
        ]"
      >
        {{ config.environment === 'wsl2' ? 'WSL2' : 'Windows' }}
      </span>

      <!-- Center: Config Info -->
      <div class="flex-1 min-w-0">
        <h3 class="font-medium text-gray-900 dark:text-white truncate">{{ config.name }}</h3>
        <p class="text-gray-500 dark:text-dark-400 text-sm truncate">{{ config.workingDir }}</p>
      </div>

      <!-- Command (always visible on desktop) -->
      <div class="flex items-center gap-2 text-gray-500 dark:text-dark-400 text-sm flex-shrink-0">
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
          class="flex items-center gap-1 mr-2 text-green-600 dark:text-green-400 text-sm"
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
          启动
        </Button>
        <Button variant="ghost" size="sm" @click.stop="$emit('edit')">
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
          </svg>
        </Button>
        <Button variant="ghost" size="sm" @click.stop="$emit('delete')">
          <svg class="w-4 h-4 text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </Button>
      </div>
    </div>

    <!-- Expandable Running Sessions -->
    <div v-if="runningSessions.length > 0" class="border-t border-gray-200 dark:border-dark-700">
      <div
        class="flex items-center gap-2 px-4 py-2 cursor-pointer text-gray-600 dark:text-dark-400 text-sm hover:bg-gray-50 dark:hover:bg-dark-800"
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
        <span>运行中的会话 ({{ runningSessions.length }})</span>
      </div>

      <!-- Running Sessions List -->
      <div v-if="isExpanded" class="bg-gray-50 dark:bg-dark-900">
        <div
          v-for="session in runningSessions"
          :key="session.id"
          class="flex items-center justify-between px-4 py-3 border-b border-gray-200 dark:border-dark-700 last:border-b-0 hover:bg-gray-100 dark:hover:bg-dark-800 cursor-pointer"
          @click="$emit('viewSession', session)"
        >
          <div class="flex items-center gap-3">
            <!-- Status Indicator -->
            <div
              :class="[
                'w-2 h-2 rounded-full',
                session.status === 'running' ? 'bg-green-500' :
                session.status === 'waitingInput' ? 'bg-yellow-500' :
                session.status === 'error' ? 'bg-red-500' : 'bg-gray-400 dark:bg-dark-500'
              ]"
            ></div>
            <span class="text-gray-900 dark:text-white">{{ session.name }}</span>
            <span
              v-if="session.sessionType"
              :class="[
                'text-xs px-2 py-0.5 rounded',
                session.sessionType === 'plugin' ? 'bg-purple-500/20 text-purple-400' : 'bg-blue-500/20 text-blue-400'
              ]"
            >
              {{ session.sessionType === 'plugin' ? 'Plugin' : 'PTY' }}
            </span>
          </div>
          <div class="flex items-center gap-2">
            <span class="text-gray-500 dark:text-dark-400 text-sm">{{ getSessionTime(session) }}</span>
            <Button variant="ghost" size="sm" @click.stop="$emit('stopSession', session.id)">
              <svg class="w-4 h-4 text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 10a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1v-4z" />
              </svg>
            </Button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import type { SessionConfig, SessionInfo } from '@/stores/session'
import Button from '@/components/common/Button.vue'

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

const hasRunningSessions = computed(() => runningSessions.value.length > 0)

function toggleExpand() {
  isExpanded.value = !isExpanded.value
}

function getSessionTime(session: SessionInfo): string {
  const start = session.startedAt || session.createdAt
  if (!start) return '--'

  const startTime = new Date(start).getTime()
  const now = Date.now()
  const diff = Math.floor((now - startTime) / 1000)

  if (diff < 60) return `${diff}秒`
  if (diff < 3600) return `${Math.floor(diff / 60)}分${diff % 60}秒`
  const hours = Math.floor(diff / 3600)
  const minutes = Math.floor((diff % 3600) / 60)
  return `${hours}小时${minutes}分`
}
</script>