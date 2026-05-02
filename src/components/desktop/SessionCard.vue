<template>
  <div
    class="bg-dark-800 rounded-lg border border-dark-700 p-4 hover:border-dark-600 transition-colors cursor-pointer"
    @click="$emit('edit')"
  >
    <!-- Header -->
    <div class="flex items-start justify-between mb-4">
      <div>
        <h3 class="font-medium text-white">{{ config.name }}</h3>
        <p class="text-dark-400 text-sm mt-1">{{ config.workingDir }}</p>
      </div>
      <span
        :class="[
          'px-2 py-1 rounded text-xs font-medium',
          config.environment === 'wsl2'
            ? 'bg-purple-900/50 text-purple-300 border border-purple-700'
            : 'bg-blue-900/50 text-blue-300 border border-blue-700'
        ]"
      >
        {{ config.environment === 'wsl2' ? 'WSL2' : 'Windows' }}
      </span>
    </div>

    <!-- Info -->
    <div class="space-y-2 text-sm">
      <div class="flex items-center gap-2 text-dark-400">
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
        <span class="font-mono">{{ config.command }}</span>
      </div>

      <div v-if="config.wslDistro" class="flex items-center gap-2 text-dark-400">
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
        </svg>
        <span>{{ config.wslDistro }}</span>
      </div>

      <div v-if="config.tmuxSession" class="flex items-center gap-2 text-dark-400">
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 5a1 1 0 011-1h14a1 1 0 011 1v2a1 1 0 01-1 1H5a1 1 0 01-1-1V5zM4 13a1 1 0 011-1h6a1 1 0 011 1v6a1 1 0 01-1 1H5a1 1 0 01-1-1v-6zM16 13a1 1 0 011-1h2a1 1 0 011 1v6a1 1 0 01-1 1h-2a1 1 0 01-1-1v-6z" />
        </svg>
        <span>Tmux: {{ config.tmuxSession }}</span>
      </div>
    </div>

    <!-- Actions -->
    <div class="mt-4 pt-4 border-t border-dark-700 flex gap-2">
      <Button variant="primary" size="sm" class="flex-1" @click.stop="$emit('start')">
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
</template>

<script setup lang="ts">
import type { SessionConfig } from '@/stores/session'
import Button from '@/components/common/Button.vue'

defineEmits<{
  (e: 'start'): void
  (e: 'edit'): void
  (e: 'delete'): void
}>()

defineProps<{
  config: SessionConfig
}>()
</script>
