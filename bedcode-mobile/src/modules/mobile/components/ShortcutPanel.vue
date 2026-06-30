<template>
  <Teleport to="body">
    <transition name="fade">
      <div
        v-if="visible"
        class="fixed inset-0 z-[100] flex items-end justify-center mobile-ui"
        @click.self="emit('close')"
      >
        <div class="absolute inset-0 bg-[var(--mobile-overlay)]" @click="emit('close')"></div>

        <!-- 快捷键面板 -->
        <div
          class="relative bg-[var(--mobile-bg-card)] border-t border-[var(--mobile-border)] rounded-t-2xl w-full max-w-md mx-4 mb-4 shadow-xl"
          :style="{ paddingBottom: `${safeAreaBottom}px` }"
        >
          <!-- 拖动条 -->
          <div class="flex justify-center pt-3 pb-1">
            <div class="w-10 h-1 bg-[var(--mobile-accent-muted)] rounded-full"></div>
          </div>

          <!-- Header -->
          <div class="flex items-center justify-between px-4 py-2">
            <span class="font-medium text-[var(--mobile-text-primary)]">{{ t('mobile.shortcut.title') }}</span>
            <button
              class="p-1.5 rounded-lg hover:bg-[var(--mobile-accent-muted)] transition-colors"
              @click="emit('close')"
            >
              <svg class="w-5 h-5 text-[var(--mobile-text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <!-- 高频快捷键 -->
          <div v-if="topShortcuts.length > 0" class="px-4 py-2 border-b border-[var(--mobile-border)]">
            <div class="flex flex-wrap gap-2">
              <button
                v-for="key in topShortcuts"
                :key="key"
                class="px-3 py-1.5 bg-[var(--mobile-accent-muted)] border border-[var(--mobile-accent)] text-[var(--mobile-accent)] rounded-lg text-sm font-medium hover:opacity-80 transition-colors"
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
                class="flex flex-col items-center justify-center p-2.5 bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border)] rounded-lg hover:border-[var(--mobile-accent)] transition-colors"
                @click="handleShortcutClick(key.code)"
              >
                <span class="text-base mb-0.5 text-[var(--mobile-accent)]">{{ key.icon }}</span>
                <span class="text-[10px] text-[var(--mobile-text-muted)]">{{ key.label }}</span>
              </button>
            </div>
          </div>
        </div>
      </div>
    </transition>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, inject } from 'vue'
import type { Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useInputAssistantStore } from '@/modules/shared/stores/inputAssistant'

const { t } = useI18n()

const props = defineProps<{
  visible: boolean
}>()

const emit = defineEmits<{
  close: []
  select: [key: string]
}>()

const safeArea = inject<Ref<{ bottom: number; navigationBar: number }>>('safeArea')
const safeAreaBottom = computed(() => safeArea?.value?.navigationBar || safeArea?.value?.bottom || 16)

const store = useInputAssistantStore()

const topShortcuts = computed(() => store.topShortcuts)

const allShortcuts = [
  { label: 'Tab', code: 'tab', icon: '⇥' },
  { label: 'Enter', code: 'enter', icon: '↵' },
  { label: 'Esc', code: 'escape', icon: '⎋' },
  { label: 'Del', code: 'backspace', icon: '⌫' },
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