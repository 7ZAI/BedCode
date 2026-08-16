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
        <div class="shortcut-panel" :style="{ paddingBottom: `${safeAreaBottom}px` }">
          <!-- 拖动条 -->
          <div class="flex justify-center pt-3 pb-1">
            <div class="w-10 h-1 bg-[var(--mobile-accent-muted)] rounded-full"></div>
          </div>

          <!-- Header -->
          <div class="flex items-center justify-between px-4 py-2">
            <span class="font-medium text-[var(--mobile-text-primary)]">{{ t('mobile.shortcut.title') }}</span>
            <button
              class="p-1.5 rounded-lg hover:bg-[var(--mobile-accent-muted)] active:opacity-70 transition-colors"
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
                class="top-shortcut-btn"
                @click="handleShortcutClick(key)"
              >
                {{ getShortcutLabel(key) }}
              </button>
            </div>
          </div>

          <!-- 全部快捷键 - 网格布局 -->
          <div class="shortcut-grid-area">
            <div class="shortcut-grid">
              <button
                v-for="key in allShortcuts"
                :key="key.code"
                class="shortcut-key-btn"
                @click="handleShortcutClick(key.code)"
              >
                <span class="shortcut-key-icon">{{ key.icon }}</span>
                <span class="shortcut-key-label">{{ key.label }}</span>
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
import { useInputAssistantStore } from '@/stores/inputAssistant'

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
.shortcut-panel {
  --key-h: clamp(2rem, 2.5rem, 3rem);
  --key-font: clamp(0.625rem, 0.75rem, 0.875rem);
  --key-icon-font: clamp(0.875rem, 1rem, 1.25rem);

  position: relative;
  background: var(--mobile-bg-card);
  border-top: 1px solid var(--mobile-border);
  border-radius: 1rem 1rem 0 0;
  width: 100%;
  max-width: clamp(280px, 448px, 520px);
  margin: 0 1rem 1rem;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
}

.top-shortcut-btn {
  padding: 0 clamp(0.5rem, 0.75rem, 1rem);
  height: var(--key-h);
  background: var(--mobile-accent-muted);
  border: 1px solid var(--mobile-accent);
  color: var(--mobile-accent);
  border-radius: 0.5rem;
  font-size: var(--key-font);
  font-weight: 500;
  cursor: pointer;
  transition: opacity 0.15s ease;
}

.top-shortcut-btn:hover {
  opacity: 0.8;
}

.shortcut-grid-area {
  padding: 0.75rem 1rem;
  max-height: clamp(200px, 300px, 400px);
  overflow-y: auto;
}

.shortcut-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: clamp(0.375rem, 0.5rem, 0.75rem);
}

.shortcut-key-btn {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: clamp(0.375rem, 0.625rem, 0.75rem);
  background: var(--mobile-bg-primary);
  border: 1px solid var(--mobile-border);
  border-radius: 0.5rem;
  cursor: pointer;
  transition: border-color 0.15s ease;
  min-height: var(--key-h);
}

.shortcut-key-btn:hover {
  border-color: var(--mobile-accent);
}

.shortcut-key-icon {
  font-size: var(--key-icon-font);
  margin-bottom: 0.125rem;
  color: var(--mobile-accent);
}

.shortcut-key-label {
  font-size: clamp(0.5625rem, 0.625rem, 0.75rem);
  color: var(--mobile-text-muted);
}

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
