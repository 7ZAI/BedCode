<template>
  <div
    class="bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl p-3.5 shadow-[var(--mobile-card-shadow)] active:scale-[0.98] transition-all duration-150"
    @click="$emit('tap')"
  >
    <!-- 任务内容：单行显示，超出省略 -->
    <p class="text-[0.9375rem] font-semibold text-[var(--mobile-text-primary)] truncate mb-1.5 leading-relaxed" :title="task.content">{{ task.content }}</p>

    <!-- Row: Date + Actions -->
    <div ref="rowRef" class="relative flex items-center justify-between">
      <span ref="dateRef" class="text-xs text-[var(--mobile-text-disabled)] flex-shrink-0">{{ formattedDate }}</span>

      <!-- 宽度充足：直接显示三个操作按钮 -->
      <div v-if="showInlineActions" class="flex items-center gap-1.5">
        <button
          class="inline-action p-1.5 rounded-lg text-[var(--mobile-accent)] bg-[var(--mobile-accent-muted)] hover:opacity-80 active:scale-95 transition-all"
          :title="t('mobile.presetTask.execute')"
          @click.stop="handleExecute"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
        </button>
        <button
          class="inline-action p-1.5 rounded-lg text-[var(--mobile-text-muted)] bg-[var(--mobile-bg-secondary)] hover:text-[var(--mobile-text-primary)] active:scale-95 transition-all"
          :title="t('mobile.presetTask.edit')"
          @click.stop="handleEdit"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
          </svg>
        </button>
        <button
          class="inline-action p-1.5 rounded-lg text-[var(--mobile-error)] bg-[var(--mobile-error-muted)] hover:opacity-80 active:scale-95 transition-all"
          :title="t('mobile.presetTask.delete')"
          @click.stop="handleDelete"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </button>
      </div>

      <!-- 宽度不足：... 菜单触发按钮 -->
      <button
        v-else
        ref="menuTriggerRef"
        class="p-1 text-[var(--mobile-text-muted)] hover:text-[var(--mobile-text-primary)] active:bg-[var(--mobile-bg-secondary)] rounded transition-colors"
        @click.stop="toggleMenu"
      >
        <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
          <path d="M6 10a2 2 0 11-4 0 2 2 0 014 0zM12 10a2 2 0 11-4 0 2 2 0 014 0zM16 12a2 2 0 100-4 2 2 0 000 4z" />
        </svg>
      </button>

      <!-- 隐藏的测量副本：与内联按钮完全一致，用于计算所需宽度 -->
      <div ref="measureRef" class="measure-ghost flex items-center gap-1.5" aria-hidden="true">
        <span class="inline-action p-1.5"><span class="block w-3.5 h-3.5"></span></span>
        <span class="inline-action p-1.5"><span class="block w-3.5 h-3.5"></span></span>
        <span class="inline-action p-1.5"><span class="block w-3.5 h-3.5"></span></span>
      </div>
    </div>

    <!-- Dropdown menu - Teleport 到 body 避免被裁剪，自动选择上方/下方 -->
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="showMenu" class="fixed inset-0 z-50 mobile-ui" @click="showMenu = false">
          <div
            class="absolute bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl shadow-xl overflow-hidden min-w-[140px]"
            :style="menuStyle"
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
              <svg class="w-4 h-4 text-[var(--mobile-error)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
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
 * 操作区自适应布局：
 * - 行宽足够容纳日期 + 三个内联按钮时，直接显示按钮（执行/编辑/删除）
 * - 宽度不足时降级为 ... 下拉菜单（Teleport 定位，自动上/下展开）
 * 宽度判定通过隐藏测量副本 + ResizeObserver 实现，随容器尺寸实时切换
 */

import { ref, computed, nextTick, onMounted, onBeforeUnmount } from 'vue'
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

// ==================== 自适应操作区 ====================

const rowRef = ref<HTMLElement | null>(null)
const dateRef = ref<HTMLElement | null>(null)
const measureRef = ref<HTMLElement | null>(null)
const showInlineActions = ref(true)
let resizeObserver: ResizeObserver | null = null

/** 日期与操作区之间的间距（与 justify-between 下的最小视觉间距一致） */
const MIN_SPACING = 12

/** 比较可用宽度与内联按钮所需宽度，决定内联显示或降级菜单 */
function updateActionLayout() {
  const row = rowRef.value
  const date = dateRef.value
  const measure = measureRef.value
  if (!row || !date || !measure) return

  const available = row.clientWidth - date.offsetWidth - MIN_SPACING
  const required = measure.offsetWidth
  const inline = required <= available
  if (inline !== showInlineActions.value) {
    showInlineActions.value = inline
  }
}

onMounted(() => {
  updateActionLayout()
  if (window.ResizeObserver && rowRef.value) {
    resizeObserver = new ResizeObserver(() => updateActionLayout())
    resizeObserver.observe(rowRef.value)
  } else {
    window.addEventListener('resize', updateActionLayout)
  }
})

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
  resizeObserver = null
  window.removeEventListener('resize', updateActionLayout)
})

// ==================== 下拉菜单（窄屏降级） ====================

const showMenu = ref(false)
const menuTriggerRef = ref<HTMLElement | null>(null)
const menuPosition = ref({ top: 0, left: 0, openUp: false })

// 菜单预估高度：3 个按钮 × 48px
const MENU_HEIGHT = 144

const menuStyle = computed(() => {
  const { top, left, openUp } = menuPosition.value
  return {
    top: openUp ? 'auto' : `${top}px`,
    bottom: openUp ? `${window.innerHeight - top}px` : 'auto',
    left: `${left}px`,
  }
})

/** 计算菜单定位，自动选择上方或下方 */
function computeMenuPosition() {
  const el = menuTriggerRef.value
  if (!el) return

  const rect = el.getBoundingClientRect()
  // 菜单右对齐触发按钮
  const left = Math.max(8, rect.right - 140)
  const spaceAbove = rect.top
  const spaceBelow = window.innerHeight - rect.bottom

  // 上方空间不足则向下展开
  const openUp = spaceAbove >= MENU_HEIGHT || spaceAbove > spaceBelow
  // 上方展开时定位到按钮顶部，下方展开时定位到按钮底部
  const top = openUp ? rect.top : rect.bottom + 4

  menuPosition.value = { top, left, openUp }
}

async function toggleMenu() {
  if (showMenu.value) {
    showMenu.value = false
    return
  }
  computeMenuPosition()
  showMenu.value = true
  // 打开后重新计算（动画完成后 DOM 可能有变化）
  await nextTick()
  computeMenuPosition()
}

// ==================== 展示与事件 ====================

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

/* 测量副本：不可见、不占布局，仅用于获取内联按钮组的真实宽度 */
.measure-ghost {
  position: absolute;
  top: 0;
  left: -9999px;
  visibility: hidden;
  pointer-events: none;
  white-space: nowrap;
}

.inline-action {
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
</style>
