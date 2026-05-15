<template>
  <Teleport to="body">
    <transition name="fade">
      <div
        v-if="visible"
        class="fixed inset-0 z-[100] flex items-end justify-center"
        @click.self="emit('close')"
      >
        <div class="absolute inset-0 bg-black/30" @click="emit('close')"></div>

        <!-- 快捷键面板 -->
        <div
          class="relative bg-white dark:bg-dark-800 rounded-t-2xl w-full max-w-md mx-4 mb-4 shadow-xl"
          :style="{ paddingBottom: 'env(safe-area-inset-bottom, 16px)' }"
        >
          <!-- 拖动条 -->
          <div class="flex justify-center pt-3 pb-1">
            <div class="w-10 h-1 bg-gray-300 dark:bg-dark-600 rounded-full"></div>
          </div>

          <!-- Header -->
          <div class="flex items-center justify-between px-4 py-2">
            <span class="font-medium text-gray-900 dark:text-dark-100">快捷键</span>
            <button
              class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-dark-700"
              @click="emit('close')"
            >
              <svg class="w-5 h-5 text-gray-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <!-- 高频快捷键 -->
          <div v-if="topShortcuts.length > 0" class="px-4 py-2 border-b border-gray-100 dark:border-dark-700">
            <div class="flex flex-wrap gap-2">
              <button
                v-for="key in topShortcuts"
                :key="key"
                class="px-3 py-1.5 bg-primary-100 dark:bg-primary-900/30 text-primary-700 dark:text-primary-300 rounded-lg text-sm font-medium"
                @click="handleShortcutClick(key)"
              >
                {{ getShortcutLabel(key) }}
              </button>
            </div>
          </div>

          <!-- 全部快捷键 - 网格布局 -->
          <div class="px-4 py-3 max-h-[300px] overflow-y-auto">
            <div class="grid grid-cols-4 gap-2">
              <button
                v-for="key in allShortcuts"
                :key="key.code"
                class="flex flex-col items-center justify-center p-2.5 bg-gray-100 dark:bg-dark-700 rounded-lg hover:bg-gray-200 dark:hover:bg-dark-600 transition-colors"
                @click="handleShortcutClick(key.code)"
              >
                <span class="text-base mb-0.5">{{ key.icon }}</span>
                <span class="text-[10px] text-gray-600 dark:text-dark-300">{{ key.label }}</span>
              </button>
            </div>
          </div>
        </div>
      </div>
    </transition>
  </Teleport>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useInputAssistantStore } from '@/modules/shared/stores/inputAssistant'

const props = defineProps<{
  visible: boolean
}>()

const emit = defineEmits<{
  close: []
  select: [key: string]
}>()

const store = useInputAssistantStore()

const topShortcuts = computed(() => store.topShortcuts)

const allShortcuts = [
  { label: 'Tab', code: 'tab', icon: '⇥' },
  { label: 'Enter', code: 'enter', icon: '↵' },
  { label: 'Esc', code: 'escape', icon: '⎋' },
  { label: 'Del', code: 'delete', icon: '⌫' },
  { label: 'Ctrl+C', code: 'ctrl_c', icon: '⚡' },
  { label: 'Ctrl+Z', code: 'ctrl_z', icon: '↺' },
  { label: 'Ctrl+L', code: 'ctrl_l', icon: '🗑' },
  { label: '↑', code: 'arrow_up', icon: '↑' },
  { label: '↓', code: 'arrow_down', icon: '↓' },
  { label: '←', code: 'arrow_left', icon: '←' },
  { label: '→', code: 'arrow_right', icon: '→' },
]

function getShortcutLabel(key: string): string {
  const found = allShortcuts.find(s => s.code === key)
  return found?.label || key
}

function handleShortcutClick(key: string) {
  store.recordShortcut(key)
  emit('select', key)
  emit('close')
}
</script>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-active > div:last-child,
.fade-leave-active > div:last-child {
  transition: transform 0.3s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.fade-enter-from > div:last-child,
.fade-leave-to > div:last-child {
  transform: translateY(100%);
}
</style>