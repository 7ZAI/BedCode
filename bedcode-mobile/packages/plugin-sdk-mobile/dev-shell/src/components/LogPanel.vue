<script setup lang="ts">
/**
 * LogPanel — 插件日志面板（context.logger / 生命周期 / 加载错误）
 *
 * 常驻右下角（工作台层，非手机框内），Teleport to body。
 */
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { clearLogs, logs } from '../registry'

const open = defineModel<boolean>('logOpen', { default: false })
const filter = ref<'all' | 'warn' | 'error'>('all')

const { t } = useI18n()

const visibleLogs = computed(() => {
  if (filter.value === 'all') return logs.value
  return logs.value.filter((l) => l.level === filter.value || (filter.value === 'warn' && l.level === 'warn') || l.level === 'error')
})

const levelColor: Record<string, string> = {
  debug: 'text-[var(--mobile-text-muted)]',
  info: 'text-[var(--mobile-text-secondary)]',
  warn: 'text-[var(--mobile-warning)]',
  error: 'text-[var(--mobile-error)]',
}
</script>

<template>
  <Teleport to="body">
    <Transition name="log-panel">
      <div
        v-if="open"
        class="mobile-ui fixed right-3 bottom-3 z-50 w-[380px] max-w-[calc(100vw-24px)] h-[280px] rounded-xl border border-[var(--mobile-border)] bg-[var(--mobile-bg-elevated)]/95 backdrop-blur-xl shadow-2xl flex flex-col overflow-hidden"
      >
        <div class="flex items-center gap-2 px-3 h-9 flex-shrink-0 border-b border-[var(--mobile-border)]">
          <span class="text-xs font-semibold text-[var(--mobile-text-primary)]">{{ t('devshell.logs.title') }}</span>
          <div class="flex items-center gap-1 ml-2">
            <button
              v-for="f in (['all', 'warn', 'error'] as const)"
              :key="f"
              class="px-2 py-0.5 rounded text-[11px] transition-colors duration-200"
              :class="filter === f ? 'bg-[var(--mobile-accent-muted)] text-[var(--mobile-accent)]' : 'text-[var(--mobile-text-muted)] hover:text-[var(--mobile-text-secondary)]'"
              @click="filter = f"
            >
              {{ f }}
            </button>
          </div>
          <span class="flex-1" />
          <button class="text-[11px] text-[var(--mobile-text-muted)] hover:text-[var(--mobile-text-secondary)] transition-colors duration-200" @click="clearLogs()">
            {{ t('devshell.logs.clear') }}
          </button>
          <button class="w-6 h-6 flex items-center justify-center text-[var(--mobile-text-muted)] hover:text-[var(--mobile-text-primary)] transition-colors duration-200" @click="open = false">
            ×
          </button>
        </div>
        <div class="flex-1 min-h-0 overflow-y-auto p-2 font-mono text-[11px] leading-relaxed">
          <p v-if="visibleLogs.length === 0" class="text-[var(--mobile-text-muted)] px-2">{{ t('devshell.logs.empty') }}</p>
          <p v-for="log in visibleLogs" :key="log.id" class="px-2 py-0.5 break-all">
            <span class="text-[var(--mobile-text-muted)]">{{ log.ts }}</span>
            <span class="text-[var(--mobile-accent)]"> [{{ log.pluginId }}]</span>
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
