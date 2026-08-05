<template>
  <div class="group-row cursor-pointer" @click="$emit('tap')">
    <span class="icon-chip chip-cyan">
      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z" />
      </svg>
    </span>

    <div class="flex-1 min-w-0">
      <div class="group-row-title truncate">{{ task.content }}</div>
      <div ref="rowRef" class="relative flex items-center justify-between mt-1">
        <span ref="dateRef" class="group-row-sub flex-shrink-0">{{ formattedDate }}</span>

        <div v-if="showInlineActions" class="flex items-center gap-1">
          <button
            class="p-1.5 rounded-lg transition-colors active:opacity-80"
            style="color: var(--mobile-chip-cyan); background: var(--mobile-chip-cyan-bg)"
            :title="t('mobile.presetTask.execute')"
            @click.stop="handleExecute"
          >
            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z" />
            </svg>
          </button>
          <button
            class="p-1.5 rounded-lg transition-colors active:opacity-80"
            style="color: var(--mobile-chip-zinc); background: var(--mobile-chip-zinc-bg)"
            :title="t('mobile.presetTask.edit')"
            @click.stop="handleEdit"
          >
            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
            </svg>
          </button>
          <button
            class="p-1.5 rounded-lg transition-colors active:opacity-80"
            style="color: var(--mobile-chip-red); background: var(--mobile-chip-red-bg)"
            :title="t('mobile.presetTask.delete')"
            @click.stop="handleDelete"
          >
            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
            </svg>
          </button>
        </div>

        <button
          v-else
          ref="menuTriggerRef"
          class="p-1 rounded-lg transition-colors active:opacity-80"
          style="color: var(--mobile-text-muted)"
          @click.stop="toggleMenu"
        >
          <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
            <path d="M6 10a2 2 0 11-4 0 2 2 0 014 0zM12 10a2 2 0 11-4 0 2 2 0 014 0zM16 12a2 2 0 100-4 2 2 0 000 4z" />
          </svg>
        </button>

        <div ref="measureRef" class="measure-ghost flex items-center gap-1" aria-hidden="true">
          <span class="p-1.5"><span class="block w-3.5 h-3.5"></span></span>
          <span class="p-1.5"><span class="block w-3.5 h-3.5"></span></span>
          <span class="p-1.5"><span class="block w-3.5 h-3.5"></span></span>
        </div>
      </div>
    </div>

    <Teleport to="body">
      <Transition name="fade">
        <div v-if="showMenu" class="fixed inset-0 z-50 mobile-ui" @click="showMenu = false">
          <div
            class="absolute rounded-xl shadow-xl overflow-hidden min-w-[140px] group-card"
            :style="menuStyle"
            @click.stop
          >
            <button
              class="w-full px-4 py-3 text-left text-sm flex items-center gap-2 transition-colors active:opacity-80"
              style="color: var(--mobile-row-title)"
              @click="handleExecute"
            >
              <svg class="w-4 h-4" style="color: var(--mobile-chip-cyan)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z" />
              </svg>
              {{ t('mobile.presetTask.execute') }}
            </button>
            <button
              class="w-full px-4 py-3 text-left text-sm flex items-center gap-2 transition-colors active:opacity-80"
              style="border-top: 1px solid var(--mobile-group-divider); color: var(--mobile-row-title)"
              @click="handleEdit"
            >
              <svg class="w-4 h-4" style="color: var(--mobile-chip-zinc)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
              </svg>
              {{ t('mobile.presetTask.edit') }}
            </button>
            <button
              class="w-full px-4 py-3 text-left text-sm flex items-center gap-2 transition-colors active:opacity-80"
              style="border-top: 1px solid var(--mobile-group-divider); color: var(--mobile-chip-red)"
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

const rowRef = ref<HTMLElement | null>(null)
const dateRef = ref<HTMLElement | null>(null)
const measureRef = ref<HTMLElement | null>(null)
const showInlineActions = ref(true)
let resizeObserver: ResizeObserver | null = null

const MIN_SPACING = 12

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

const showMenu = ref(false)
const menuTriggerRef = ref<HTMLElement | null>(null)
const menuPosition = ref({ top: 0, left: 0, openUp: false })

const MENU_HEIGHT = 144

const menuStyle = computed(() => {
  const { top, left, openUp } = menuPosition.value
  return {
    top: openUp ? 'auto' : `${top}px`,
    bottom: openUp ? `${window.innerHeight - top}px` : 'auto',
    left: `${left}px`,
  }
})

function computeMenuPosition() {
  const el = menuTriggerRef.value
  if (!el) return

  const rect = el.getBoundingClientRect()
  const left = Math.max(8, rect.right - 140)
  const spaceAbove = rect.top
  const spaceBelow = window.innerHeight - rect.bottom

  const openUp = spaceAbove >= MENU_HEIGHT || spaceAbove > spaceBelow
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
  await nextTick()
  computeMenuPosition()
}

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

.measure-ghost {
  position: absolute;
  top: 0;
  left: -9999px;
  visibility: hidden;
  pointer-events: none;
  white-space: nowrap;
}
</style>
