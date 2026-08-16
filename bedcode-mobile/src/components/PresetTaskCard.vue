<template>
  <div class="group-row">
    <!-- 左侧任务图标（装饰性，执行入口统一在右侧操作组） -->
    <span class="icon-chip chip-cyan flex-shrink-0">
      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4" />
      </svg>
    </span>

    <!-- 中间内容（点击触发详情/预览） -->
    <div class="flex-1 min-w-0 cursor-pointer" @click="$emit('tap')">
      <div class="group-row-title truncate">{{ task.content }}</div>
      <div class="group-row-sub mt-0.5 flex items-center gap-2">
        <span>{{ formattedDate }}</span>
        <!-- 执行状态徽章（未使用/执行中/已完成/已中断） -->
        <span class="preset-status-badge" :style="{ color: statusColor[task.status], borderColor: statusColor[task.status] }">
          {{ statusText }}
        </span>
      </div>
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

/** 执行状态徽章颜色（与 auto-task 面板状态色一致，全部走 token；
    小字号下对比度满足 WCAG AA，unused 用浅灰而非 disabled 深灰） */
const statusColor: Record<string, string> = {
  unused: 'var(--mobile-chip-zinc)',
  executing: 'var(--mobile-accent)',
  completed: 'var(--mobile-success)',
  interrupted: 'var(--mobile-error)',
}

const statusText = computed(() => {
  // 状态 key 已在 zh-CN/en 同步收录（mobile.presetTask.status.*）
  return t(`mobile.presetTask.status.${props.task.status}`)
})

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
/* 卡片可嵌入列表/面板，以自身宽度为容器进行流式缩放 */
:root {
  container-type: inline-size;
}

/* 操作按钮：自适应大小（手机紧凑、平板略大），触控区不小于 44px */
.action-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: clamp(0.3125rem, 0.375rem + (100cqw - 360px) / 800, 0.4375rem);
  border-radius: 0.625rem;
  transition: background-color 0.2s ease, color 0.2s ease;
  cursor: pointer;
  border: none;
  background: transparent;
  min-width: 2.75rem;
  min-height: 2.75rem;
}

.action-btn:active {
  opacity: 0.7;
  transform: scale(0.95);
}

.action-icon {
  width: clamp(1rem, 1.125rem + (100cqw - 360px) / 800, 1.25rem);
  height: clamp(1rem, 1.125rem + (100cqw - 360px) / 800, 1.25rem);
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

/* 执行状态徽章 */
.preset-status-badge {
  font-size: clamp(0.625rem, 0.6875rem + (100cqw - 360px) / 800, 0.75rem);
  line-height: 1.4;
  padding: 0.125rem 0.5rem;
  border-radius: 9999px;
  border: 1px solid;
  flex-shrink: 0;
}
</style>
