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
        <div
          class="flex-1 min-w-0 cursor-pointer"
          @click="expanded = !expanded"
        >
          <div class="flex items-center gap-2 min-w-0">
            <span class="text-base font-medium text-[var(--mobile-text-primary)] truncate flex-1 min-w-0">{{ config.name }}</span>
          </div>
          <button
            v-if="runningCount > 0"
            class="mt-2 inline-flex items-center gap-1.5 text-xs font-medium"
            style="color: var(--mobile-chip-emerald)"
            @click.stop="$emit('sessionClick', runningSessions[0])"
          >
            <span class="status-dot dot-emerald"></span>
            {{ t('mobile.sessionConfig.runningCount', { count: runningCount }) }} · {{ t('mobile.sessionConfig.viewSession') }}
          </button>
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
        <svg
          class="w-4 h-4 flex-shrink-0 transition-transform duration-200 cursor-pointer"
          style="color: var(--mobile-row-sub)"
          :class="{ 'rotate-180': expanded }"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
          @click="expanded = !expanded"
        >
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
        </svg>
      </div>
    </div>

    <transition name="slide">
      <div v-if="expanded" class="border-t border-[var(--mobile-border)]">
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

        <!-- 启动命令（仅展开时显示） -->
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

        <template v-if="runningSessions.length > 0">
          <div
            v-for="session in runningSessions"
            :key="session.id"
            class="px-4 py-3 flex items-center justify-between transition-colors active:opacity-80"
            style="border-top: 1px solid var(--mobile-border)"
            @click.stop="$emit('sessionClick', session)"
          >
            <div class="flex items-center gap-3 min-w-0 cursor-pointer">
              <span class="status-dot dot-emerald"></span>
              <span class="text-xs text-[var(--mobile-text-muted)] truncate">{{ session.name }}</span>
            </div>
            <button
              class="shrink-0 p-1.5 rounded-lg transition-colors active:opacity-80"
              style="color: var(--mobile-text-muted)"
              @click.stop="$emit('stopSession', session)"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </template>
      </div>
    </transition>
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
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const props = defineProps<{
  config: SessionConfigSummary
  activeSessions: any[]
  isStarting: boolean
}>()

defineEmits<{
  start: [config: SessionConfigSummary]
  navigateToFiles: [config: SessionConfigSummary]
  sessionClick: [session: any]
  stopSession: [session: any]
}>()

const expanded = ref(false)

const runningSessions = computed(() =>
  props.activeSessions.filter(
    s => (s.config_id === props.config.id || s.configId === props.config.id)
      && (s.status === 'running' || s.status === 'waiting_input')
  )
)

const runningCount = computed(() => runningSessions.value.length)
</script>

<style scoped>
.slide-enter-active,
.slide-leave-active {
  transition: transform 0.2s ease, opacity 0.2s ease;
  transform-origin: top;
}

.slide-enter-from,
.slide-leave-to {
  transform: scaleY(0);
  opacity: 0;
}

.slide-enter-to,
.slide-leave-from {
  transform: scaleY(1);
  opacity: 1;
}
</style>
