<template>
  <div
    class="bg-[var(--mobile-bg-secondary)] border border-[var(--mobile-border)] rounded-xl overflow-hidden shadow-[var(--mobile-card-shadow)] hover:border-[var(--mobile-border-active)] hover:shadow-[var(--mobile-card-shadow-hover)] transition-all duration-300"
  >
    <!-- 折叠头部 -->
    <div
      class="p-4 flex items-center gap-2"
    >
      <div
        class="flex items-center gap-2.5 min-w-0 flex-1 cursor-pointer active:bg-[var(--mobile-accent)]/5 rounded-lg -ml-1 px-1 py-0.5"
        @click="expanded = !expanded"
      >
        <!-- 运行中指示器 -->
        <div
          v-if="runningCount > 0"
          class="w-2 h-2 rounded-full bg-[var(--mobile-success)] shadow-[0_0_6px_rgba(16,185,129,0.5)] animate-pulse shrink-0"
        />
        <p class="font-medium text-[var(--mobile-text-primary)] truncate">{{ config.name }}</p>
        <span
          :class="[
            'text-xs px-2 py-0.5 rounded-full border shrink-0',
            config.environment === 'wsl2'
              ? 'bg-purple-500/10 border-purple-500/30 text-purple-400'
              : 'bg-cyan-500/10 border-cyan-500/30 text-cyan-400'
          ]"
        >
          {{ config.environment === 'wsl2' ? 'WSL2' : 'Windows' }}
        </span>
      </div>

      <!-- 启动按钮 -->
      <button
        class="px-3 py-1.5 bg-[var(--mobile-accent-secondary)] border border-[var(--mobile-border-active)] text-[var(--mobile-accent)] text-sm font-medium rounded-lg hover:bg-[var(--mobile-accent)]/30 transition-all flex items-center gap-1.5 shrink-0"
        :class="{ 'opacity-50': isStarting }"
        :disabled="isStarting"
        @click.stop="$emit('start', config)"
      >
        <div
          v-if="isStarting"
          class="w-4 h-4 border-2 border-[var(--mobile-accent)] border-t-transparent rounded-full animate-spin"
        />
        <svg v-else class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z" />
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
        {{ t('mobile.sessionConfig.start') }}
      </button>

      <!-- 折叠箭头 -->
      <svg
        class="w-5 h-5 text-[var(--mobile-text-muted)] transition-transform duration-200 shrink-0 cursor-pointer"
        :class="{ 'rotate-180': expanded }"
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
        @click="expanded = !expanded"
      >
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
      </svg>
    </div>

    <!-- 展开内容 -->
    <transition name="slide">
      <div v-if="expanded" class="border-t border-[var(--mobile-border)]">
        <!-- 工程目录 -->
        <button
          class="w-full px-4 py-3 flex items-center gap-3 hover:bg-[var(--mobile-accent)]/5 transition-colors active:bg-[var(--mobile-accent)]/10"
          @click.stop="$emit('navigateToFiles', config)"
        >
          <svg class="w-5 h-5 text-amber-400/80 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
          </svg>
          <div class="min-w-0 flex-1 text-left">
            <span class="text-sm text-[var(--mobile-text-secondary)]">{{ t('mobile.sessionConfig.projectDir') }}</span>
            <p class="text-xs text-[var(--mobile-text-disabled)] truncate mt-0.5">{{ config.working_dir }}</p>
          </div>
        </button>

        <!-- 运行中会话列表 -->
        <template v-if="runningSessions.length > 0">
          <div class="px-4 py-2 border-t border-[var(--mobile-border)] bg-[var(--mobile-bg-primary)]/50">
            <span class="text-xs text-[var(--mobile-success)] font-medium">{{ t('mobile.sessionConfig.runningCount', { count: runningSessions.length }) }}</span>
          </div>
          <div
            v-for="session in runningSessions"
            :key="session.id"
            class="px-4 py-3 flex items-center justify-between hover:bg-[var(--mobile-accent)]/5 transition-colors border-t border-[var(--mobile-border)]"
            @click.stop="$emit('sessionClick', session)"
          >
            <div class="flex items-center gap-3 min-w-0">
              <div class="w-2 h-2 rounded-full bg-[var(--mobile-success)] shrink-0 shadow-[0_0_6px_rgba(16,185,129,0.5)]"></div>
              <span class="text-sm text-[var(--mobile-text-secondary)] truncate">{{ session.name }}</span>
            </div>
            <button
              class="shrink-0 p-1.5 text-[var(--mobile-text-muted)] hover:text-[var(--mobile-error)] transition-colors"
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
/** 会话配置摘要信息 */
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
/**
 * SessionConfigCard - 会话配置折叠卡片
 *
 * 折叠态显示配置名称 + 环境标签
 * 展开态显示：启动会话 / 工程目录 / 运行中会话列表
 */

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
  transition: all 0.2s ease;
}

.slide-enter-from,
.slide-leave-to {
  opacity: 0;
  max-height: 0;
}

.slide-enter-to,
.slide-leave-from {
  max-height: 400px;
}
</style>
