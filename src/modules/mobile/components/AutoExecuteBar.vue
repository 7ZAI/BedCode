<script setup lang="ts">
/**
 * AutoExecuteBar - 自动执行状态条
 *
 * 自动模式开启时显示在终端顶部，
 * 展示当前任务名和状态，支持暂停/继续
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { QueuedTask } from '../composables/useAutoExecutor'

const props = defineProps<{
  currentTask: QueuedTask | null
  isPaused: boolean
  mode: 'manual' | 'auto'
}>()

const emit = defineEmits<{
  pause: []
  resume: []
}>()

const { t } = useI18n()

const statusText = computed(() => {
  if (!props.currentTask) return ''
  const map: Record<string, string> = {
    pending: t('mobile.autoExecute.pending'),
    running: t('mobile.autoExecute.running'),
    completed: t('mobile.autoExecute.completed'),
    failed: t('mobile.autoExecute.failed'),
    retrying: t('mobile.autoExecute.retrying'),
  }
  return map[props.currentTask.status] || ''
})

const show = computed(() => props.mode === 'auto' && props.currentTask)
</script>

<template>
  <div v-if="show" class="auto-execute-bar">
    <div class="bar-info">
      <span class="task-name">{{ currentTask!.title }}</span>
      <span class="task-status">{{ statusText }}</span>
    </div>
    <button
      class="bar-action"
      @click="isPaused ? emit('resume') : emit('pause')"
    >
      {{ isPaused ? t('mobile.autoExecute.resume') : t('mobile.autoExecute.pause') }}
    </button>
  </div>
</template>

<style scoped>
.auto-execute-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 12px;
  background: var(--mobile-terminal-header);
  border-bottom: 1px solid var(--mobile-border);
  font-size: 13px;
  color: var(--mobile-text-primary);
}

.bar-info {
  display: flex;
  align-items: center;
  gap: 8px;
  overflow: hidden;
}

.task-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 180px;
}

.task-status {
  flex-shrink: 0;
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 11px;
  background: var(--mobile-accent);
  color: var(--mobile-text-on-accent);
}

.bar-action {
  flex-shrink: 0;
  padding: 2px 10px;
  border: 1px solid var(--mobile-border);
  border-radius: 4px;
  background: transparent;
  color: var(--mobile-text-secondary);
  font-size: 12px;
  cursor: pointer;
}
</style>
