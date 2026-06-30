<template>
  <div
    class="bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl p-3.5 shadow-[var(--mobile-card-shadow)] active:scale-[0.98] transition-all duration-150"
    @click="$emit('tap')"
  >
    <!-- Row 1: Title + Type badge -->
    <div class="flex items-start justify-between gap-2 mb-1.5">
      <h4 class="text-sm font-semibold text-[var(--mobile-text-primary)] line-clamp-1 flex-1">{{ task.title }}</h4>
      <span
        class="shrink-0 text-[10px] font-medium px-2 py-0.5 rounded-full"
        :class="typeBadgeClass"
      >
        {{ task.type === 'once' ? t('mobile.presetTask.once') : t('mobile.presetTask.template') }}
      </span>
    </div>

    <!-- Row 2: Content preview -->
    <p class="text-xs text-[var(--mobile-text-muted)] line-clamp-2 mb-2 leading-relaxed">{{ task.content }}</p>

    <!-- Row 3: Status + Date + Action menu -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-1.5">
        <!-- Status indicator -->
        <template v-if="task.type === 'once' && task.status">
          <span
            class="inline-block w-1.5 h-1.5 rounded-full"
            :class="statusDotClass"
          ></span>
          <span class="text-[10px]" :class="statusTextClass">{{ statusLabel }}</span>
        </template>
        <template v-else>
          <span class="text-[10px] text-[var(--mobile-text-muted)]">{{ t('mobile.presetTask.reusable') }}</span>
        </template>
      </div>

      <div class="flex items-center gap-2">
        <span class="text-[10px] text-[var(--mobile-text-disabled)]">{{ formattedDate }}</span>
        <!-- Action menu trigger -->
        <button
          class="p-1 text-[var(--mobile-text-muted)] hover:text-[var(--mobile-text-primary)] active:bg-[var(--mobile-bg-secondary)] rounded transition-colors"
          @click.stop="showMenu = !showMenu"
        >
          <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
            <path d="M6 10a2 2 0 11-4 0 2 2 0 014 0zM12 10a2 2 0 11-4 0 2 2 0 014 0zM16 12a2 2 0 100-4 2 2 0 000 4z" />
          </svg>
        </button>
      </div>
    </div>

    <!-- Dropdown menu -->
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="showMenu" class="fixed inset-0 z-50 mobile-ui" @click="showMenu = false">
          <div class="absolute inset-0 bg-[var(--mobile-overlay-light)]"></div>
          <div
            class="absolute right-4 bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl shadow-xl overflow-hidden min-w-[140px]"
            :style="{ top: menuTop + 'px' }"
            @click.stop
          >
            <button
              class="w-full px-4 py-3 text-left text-sm text-[var(--mobile-text-primary)] hover:bg-[var(--mobile-bg-secondary)] flex items-center gap-2 transition-colors"
              @click="handleExecute"
            >
              <svg class="w-4 h-4 text-[var(--mobile-accent)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z" />
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              {{ t('mobile.presetTask.execute') }}
            </button>
            <button
              class="w-full px-4 py-3 text-left text-sm text-[var(--mobile-text-primary)] hover:bg-[var(--mobile-bg-secondary)] flex items-center gap-2 transition-colors"
              @click="handleEdit"
            >
              <svg class="w-4 h-4 text-[var(--mobile-text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
              </svg>
              {{ t('mobile.presetTask.edit') }}
            </button>
            <button
              class="w-full px-4 py-3 text-left text-sm text-[var(--mobile-error)] hover:bg-[var(--mobile-bg-secondary)] flex items-center gap-2 transition-colors"
              @click="handleDelete"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
              </svg>
              {{ t('mobile.presetTask.delete') }}
            </button>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
/**
 * PresetTaskCard - 预设任务卡片
 *
 * 展示任务标题、内容预览、类型标签、状态指示器和操作菜单
 */

import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { PresetTask } from '@/modules/mobile/composables/model'

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

const showMenu = ref(false)
const menuTop = ref(100)

// 类型标签样式
const typeBadgeClass = computed(() => {
  return props.task.type === 'once'
    ? 'bg-amber-500/15 text-amber-400 border border-amber-500/20'
    : 'bg-cyan-500/15 text-cyan-400 border border-cyan-500/20'
})

// 状态点样式
const statusDotClass = computed(() => {
  const status = props.task.status
  if (status === 'pending') return 'bg-gray-500'
  if (status === 'running') return 'bg-blue-400 animate-pulse'
  if (status === 'completed') return 'bg-emerald-400'
  if (status === 'failed') return 'bg-red-400'
  return 'bg-gray-500'
})

// 状态文字样式
const statusTextClass = computed(() => {
  const status = props.task.status
  if (status === 'pending') return 'text-gray-400'
  if (status === 'running') return 'text-blue-400'
  if (status === 'completed') return 'text-emerald-400'
  if (status === 'failed') return 'text-red-400'
  return 'text-gray-400'
})

// 状态文字
const statusLabel = computed(() => {
  const status = props.task.status
  if (status === 'pending') return t('mobile.presetTask.pending')
  if (status === 'running') return t('mobile.presetTask.running')
  if (status === 'completed') return t('mobile.presetTask.completed')
  if (status === 'failed') return t('mobile.presetTask.failed')
  return ''
})

// 格式化日期 (MM-DD)
const formattedDate = computed(() => {
  const d = new Date(props.task.createdAt)
  return `${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
})

function handleExecute() {
  showMenu.value = false
  emit('execute')
}

function handleEdit() {
  showMenu.value = false
  emit('edit', props.task)
}

function handleDelete() {
  showMenu.value = false
  emit('delete', props.task.id)
}
</script>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.15s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
