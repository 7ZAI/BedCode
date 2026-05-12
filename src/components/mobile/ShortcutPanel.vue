<template>
  <Teleport to="body">
    <transition name="slide">
      <div
        v-if="visible"
        class="fixed right-0 top-0 h-full w-[280px] bg-white dark:bg-dark-800 shadow-xl z-[1001] flex flex-col"
        :style="{ paddingTop: 'env(safe-area-inset-top, 0px)', paddingBottom: 'env(safe-area-inset-bottom, 0px)' }"
      >
        <!-- Header -->
        <div class="flex items-center justify-between px-4 py-3 border-b border-gray-200 dark:border-dark-700">
          <span class="font-medium text-gray-900 dark:text-dark-100">快捷键</span>
          <button
            class="p-2 rounded-lg hover:bg-gray-100 dark:hover:bg-dark-700"
            @click="emit('close')"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <!-- 高频快捷键 -->
        <div v-if="topShortcuts.length > 0" class="px-4 py-3 border-b border-gray-100 dark:border-dark-700">
          <div class="text-xs text-gray-500 dark:text-dark-400 mb-2">高频使用</div>
          <div class="flex flex-wrap gap-2">
            <button
              v-for="key in topShortcuts"
              :key="key"
              class="px-3 py-2 bg-primary-100 dark:bg-primary-900/30 text-primary-700 dark:text-primary-300 rounded-lg text-sm font-medium"
              @click="handleShortcutClick(key)"
            >
              {{ getShortcutLabel(key) }}
            </button>
          </div>
        </div>

        <!-- 全部快捷键 -->
        <div class="flex-1 overflow-y-auto px-4 py-3">
          <div class="text-xs text-gray-500 dark:text-dark-400 mb-2">全部快捷键</div>
          <div class="grid grid-cols-4 gap-2">
            <button
              v-for="key in allShortcuts"
              :key="key.code"
              class="flex flex-col items-center justify-center p-2 bg-gray-100 dark:bg-dark-700 rounded-lg hover:bg-gray-200 dark:hover:bg-dark-600 transition-colors"
              @click="handleShortcutClick(key.code)"
            >
              <span class="text-lg mb-1">{{ key.icon }}</span>
              <span class="text-[10px] text-gray-600 dark:text-dark-300">{{ key.label }}</span>
            </button>
          </div>
        </div>
      </div>
    </transition>

    <!-- 点击遮罩关闭 -->
    <transition name="fade">
      <div
        v-if="visible"
        class="fixed inset-0 bg-black/30 z-[1000]"
        @click="emit('close')"
      ></div>
    </transition>
  </Teleport>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useInputAssistantStore } from '@/stores/inputAssistant'

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
.slide-enter-active,
.slide-leave-active {
  transition: transform 0.3s ease;
}

.slide-enter-from,
.slide-leave-to {
  transform: translateX(100%);
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
