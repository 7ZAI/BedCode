<script setup lang="ts">
/**
 * LogPanel — 插件日志面板（桌面端样式）
 */
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { clearLogs, logs } from '../registry'

const open = defineModel<boolean>('logOpen', { default: false })
const filter = ref<'all' | 'warn' | 'error'>('all')

const { t } = useI18n()

const visibleLogs = computed(() => {
  if (filter.value === 'all') return logs.value
  return logs.value.filter((l) => l.level === filter.value || l.level === 'error')
})

const levelColor: Record<string, string> = {
  debug: 'text-[var(--text-tertiary)]',
  info: 'text-[var(--text-secondary)]',
  warn: 'text-amber-500',
  error: 'text-red-500',
}
</script>

<template>
  <Teleport to="body">
    <Transition name="log-panel">
      <div
        v-if="open"
        class="fixed right-3 bottom-3 z-50 w-[420px] max-w-[calc(100vw-24px)] h-[300px] rounded-card border border-[var(--border)] bg-card shadow-card-hover flex flex-col overflow-hidden"
      >
        <div class="flex items-center gap-2 px-3 h-9 flex-shrink-0 border-b border-[var(--border)]">
          <span class="text-xs font-semibold">{{ t('devshell.logs.title') }}</span>
          <div class="flex items-center gap-1 ml-2">
            <button
              v-for="f in (['all', 'warn', 'error'] as const)"
              :key="f"
              class="px-2 py-0.5 rounded-tag text-[11px] transition-colors duration-200"
              :class="filter === f ? 'bg-[var(--color-primary)]/10 text-[var(--color-primary)]' : 'text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]'"
              @click="filter = f"
            >
              {{ f }}
            </button>
          </div>
          <span class="flex-1" />
          <button class="text-[11px] text-[var(--text-tertiary)] hover:text-[var(--text-secondary)] transition-colors duration-200" @click="clearLogs()">
            {{ t('devshell.logs.clear') }}
          </button>
          <button class="w-6 h-6 flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors duration-200" @click="open = false">
            ×
          </button>
        </div>
        <div class="flex-1 min-h-0 overflow-y-auto p-2 font-mono text-[11px] leading-relaxed">
          <p v-if="visibleLogs.length === 0" class="text-[var(--text-tertiary)] px-2">{{ t('devshell.logs.empty') }}</p>
          <p v-for="log in visibleLogs" :key="log.id" class="px-2 py-0.5 break-all">
            <span class="text-[var(--text-tertiary)]">{{ log.ts }}</span>
            <span class="text-[var(--color-primary)]"> [{{ log.pluginId }}]</span>
            <span :class="levelColor[log.level]"> {{ log.message }}</span>
          </p>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.log-panel-enter-active,
.log-panel-leave-active {
  transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
}
.log-panel-enter-from,
.log-panel-leave-to {
  opacity: 0;
  transform: translateY(8px) scale(0.98);
}
</style>
