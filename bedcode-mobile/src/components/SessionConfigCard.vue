<template>
  <div
    class="bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl overflow-hidden transition-colors duration-300 hover:border-[var(--mobile-border-hover)]"
  >
    <div class="p-4">
      <div class="flex items-center gap-3">
        <span
          class="config-icon"
          :class="config.environment === 'wsl2' ? 'chip-violet' : 'chip-cyan'"
        >
          <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
          </svg>
        </span>
        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-2 min-w-0">
            <span class="text-base font-medium text-[var(--mobile-text-primary)] truncate flex-1 min-w-0">{{ config.name }}</span>
          </div>
        </div>
        <button
          class="flex-shrink-0 h-8 px-3.5 rounded-lg text-xs font-semibold active:opacity-80 transition-colors"
          style="background: color-mix(in srgb, var(--mobile-accent) 10%, transparent); color: var(--mobile-accent)"
          :class="{ 'opacity-50': isStarting }"
          :disabled="isStarting"
          @click.stop="$emit('start', config)"
        >
          <div
            v-if="isStarting"
            class="w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin"
          />
          <template v-else>{{ t('mobile.sessionConfig.start') }}</template>
        </button>
      </div>
    </div>

    <!-- 配置详情固定显示（工程目录/启动命令）：常驻可见，不再折叠；
         运行中的会话统一展示在会话页「运行中的会话」区域（SessionCard） -->
    <div class="border-t border-[var(--mobile-border)]">
      <button
        class="w-full px-4 py-3 flex items-center gap-3 transition-colors active:opacity-80"
        @click.stop="$emit('navigateToFiles', config)"
      >
        <span class="config-icon-sm chip-amber">
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
          </svg>
        </span>
        <div class="min-w-0 flex-1 text-left">
          <span class="text-xs text-[var(--mobile-text-muted)]">{{ t('mobile.sessionConfig.projectDir') }}</span>
          <p class="text-xs font-mono text-[var(--mobile-text-muted)] truncate mt-0.5">{{ config.working_dir }}</p>
        </div>
      </button>

      <!-- 启动命令 -->
      <div
        v-if="config.command"
        class="px-4 py-3 flex items-center gap-3"
        style="border-top: 1px solid var(--mobile-border)"
      >
        <span class="config-icon-sm chip-violet">
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
          </svg>
        </span>
        <div class="min-w-0 flex-1">
          <span class="text-xs text-[var(--mobile-text-muted)]">{{ t('mobile.sessionConfig.command') }}</span>
          <p class="text-xs font-mono text-[var(--mobile-text-muted)] truncate mt-0.5">{{ config.command }}</p>
        </div>
      </div>
    </div>
  </div>
</template>

<script lang="ts">
export interface SessionConfigSummary {
  id: string
  name: string
  environment: string
  wsl_distro?: string
  working_dir: string
  command: string
}
</script>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const props = defineProps<{
  config: SessionConfigSummary
  isStarting: boolean
}>()

defineEmits<{
  start: [config: SessionConfigSummary]
  navigateToFiles: [config: SessionConfigSummary]
}>()
</script>

