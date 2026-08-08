<template>
  <div class="group-row">
    <!-- 左侧执行图标（点击直接执行） -->
    <span
      class="icon-chip chip-cyan flex-shrink-0 cursor-pointer active:opacity-80 transition-opacity"
      @click.stop="handleExecute"
    >
      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z" />
      </svg>
    </span>

    <!-- 中间内容（点击触发详情/预览） -->
    <div class="flex-1 min-w-0 cursor-pointer" @click="$emit('tap')">
      <div class="group-row-title truncate">{{ task.content }}</div>
      <div class="group-row-sub mt-0.5">{{ formattedDate }}</div>
    </div>

    <!-- 右侧操作按钮组（直接显示，大小自适应） -->
    <div class="flex items-center gap-1.5 flex-shrink-0 ml-2" @click.stop>
      <button
        class="action-btn action-cyan"
        :title="t('mobile.presetTask.execute')"
        @click="handleExecute"
      >
        <svg class="action-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z" />
        </svg>
      </button>
      <button
        class="action-btn action-zinc"
        :title="t('mobile.presetTask.edit')"
        @click="handleEdit"
      >
        <svg class="action-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
        </svg>
      </button>
      <button
        class="action-btn action-red"
        :title="t('mobile.presetTask.delete')"
        @click="handleDelete"
      >
        <svg class="action-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
        </svg>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { PresetTask } from '@/composables/model'

const { t } = useI18n()

const props = defineProps<{
  task: PresetTask
}>()

const emit = defineEmits<{
  tap: []
  execute: []
  edit: [task: PresetTask]
  delete: [id: string]
}>()

const formattedDate = computed(() => {
  const d = new Date(props.task.createdAt)
  return `${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
})

function handleExecute() {
  emit('execute')
}

function handleEdit() {
  emit('edit', props.task)
}

function handleDelete() {
  emit('delete', props.task.id)
}
</script>

<style scoped>
/* 操作按钮：自适应大小，最小 44px 触摸目标 */
.action-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: clamp(0.375rem, 0.4375rem + (100vw - 360px) / 800 * 0.0625rem, 0.5rem);
  border-radius: 0.625rem;
  transition: all 0.2s ease;
  cursor: pointer;
  border: none;
  background: transparent;
  min-width: 2.25rem;
  min-height: 2.25rem;
}

.action-btn:active {
  opacity: 0.7;
  transform: scale(0.95);
}

.action-icon {
  width: clamp(1.125rem, 1.25rem + (100vw - 360px) / 800 * 0.125rem, 1.375rem);
  height: clamp(1.125rem, 1.25rem + (100vw - 360px) / 800 * 0.125rem, 1.375rem);
  flex-shrink: 0;
}

.action-cyan {
  color: var(--mobile-chip-cyan);
  background: var(--mobile-chip-cyan-bg);
}

.action-zinc {
  color: var(--mobile-chip-zinc);
  background: var(--mobile-chip-zinc-bg);
}

.action-red {
  color: var(--mobile-chip-red);
  background: var(--mobile-chip-red-bg);
}
</style>
