<template>
  <Teleport to="body">
    <Transition name="center-modal">
      <div
        v-if="modelValue"
        class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui"
        @click.self="closeOnBackdrop && close()"
      >
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-[var(--mobile-overlay)] backdrop-blur-sm"></div>

        <!-- Modal Content -->
        <div
          class="relative rounded-xl shadow-2xl border bg-[var(--mobile-bg-card)] border-[var(--mobile-border)] modal-panel"
          :class="[sizeClass]"
        >
          <!-- Header -->
          <div v-if="title || $slots.header" class="px-6 py-4 border-b border-[var(--mobile-border)]">
            <slot name="header">
              <h3 class="text-lg font-semibold text-[var(--mobile-text-primary)]">{{ title }}</h3>
            </slot>
          </div>

          <!-- Body -->
          <div class="flex flex-col overflow-hidden" :class="bodyMaxHeightClass">
            <div class="flex-1 overflow-y-auto p-6">
              <slot></slot>
            </div>
          </div>

          <!-- Footer -->
          <div v-if="$slots.footer" class="px-6 py-4 border-t border-[var(--mobile-border)]">
            <slot name="footer"></slot>
          </div>

          <!-- Close Button -->
          <button
            v-if="closable"
            class="absolute top-4 right-4 text-[var(--mobile-text-muted)] hover:text-[var(--mobile-text-primary)] active:opacity-80 transition-colors"
            @click="close()"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 通用居中弹窗（自旧宿主 `src/components/Modal.vue` 原样迁入插件）
 *
 * 迁入原因（2026-10-15：连接页/会话页按旧宿主形态还原）：旧页的清空历史 /
 * 断开 / 停止会话 / 删除会话四处都用它做二次确认，宿主壳不含该组件，
 * 插件侧需自持副本。契约与旧实现逐字一致（size / closable / closeOnBackdrop
 * 与各 size 的 body 最大高度）。
 *
 * Teleport 到 body 后仍带 `mobile-ui`：`--mobile-*` token 的祖先选择器口径在
 * 该 class 上，脱离应用根容器也能取到暗色/亮色 token。
 */
import { computed } from 'vue'

interface Props {
  modelValue: boolean
  title?: string
  size?: 'sm' | 'md' | 'lg' | 'xl' | 'full'
  closable?: boolean
  closeOnBackdrop?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  title: '',
  size: 'md',
  closable: true,
  closeOnBackdrop: true,
})

const emit = defineEmits(['update:modelValue', 'close'])

const sizeClass = computed(() => {
  switch (props.size) {
    case 'sm':
      return 'w-full max-w-sm'
    case 'md':
      return 'w-full max-w-md'
    case 'lg':
      return 'w-full max-w-lg'
    case 'xl':
      return 'w-full max-w-xl'
    case 'full':
      return 'w-full max-w-4xl'
    default:
      return 'w-full max-w-md'
  }
})

const bodyMaxHeightClass = computed(() => {
  switch (props.size) {
    case 'sm':
      return 'max-h-[60vh]'
    case 'md':
      return 'max-h-[70vh]'
    case 'lg':
      return 'max-h-[75vh]'
    case 'xl':
    case 'full':
      return 'max-h-[80vh]'
    default:
      return 'max-h-[70vh]'
  }
})

function close() {
  emit('update:modelValue', false)
  emit('close')
}
</script>
