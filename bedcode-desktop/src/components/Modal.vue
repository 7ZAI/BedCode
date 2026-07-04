<template>
  <Teleport to="body">
    <Transition name="modal">
      <div
        v-if="modelValue"
        class="fixed inset-0 z-50 flex items-center justify-center p-4"
        @click.self="closeOnBackdrop && close()"
      >
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/50 backdrop-blur-sm"></div>

        <!-- Modal Content -->
        <div
          class="relative rounded-card shadow-2xl border bg-card border-[var(--border)]"
          :class="[sizeClass]"
        >
          <!-- Header -->
          <div v-if="title || $slots.header" class="px-6 py-4 border-b border-[var(--border)]">
            <slot name="header">
              <h3 class="text-lg font-semibold text-[var(--text-primary)]">{{ title }}</h3>
            </slot>
          </div>

          <!-- Body -->
          <div
            class="flex flex-col overflow-hidden"
            :class="bodyMaxHeightClass"
          >
            <div class="flex-1 overflow-y-auto p-6">
              <slot></slot>
            </div>
          </div>

          <!-- Footer -->
          <div v-if="$slots.footer" class="px-6 py-4 border-t border-[var(--border)]">
            <slot name="footer"></slot>
          </div>

          <!-- Close Button -->
          <button
            v-if="closable"
            @click="close()"
            class="absolute top-4 right-4 text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors"
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
import { computed } from 'vue'

interface Props {
  modelValue: boolean
  title?: string
  size?: 'sm' | 'md' | 'lg' | 'xl' | 'full'
  closable?: boolean
  closeOnBackdrop?: boolean
}

const props = withDefaults(defineProps<Props>(), {
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
  // 根据弹窗大小设置不同的最大高度
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

<style scoped>
.modal-enter-active,
.modal-leave-active {
  transition: all 0.2s ease;
}

.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}

.modal-enter-from > div:last-child,
.modal-leave-to > div:last-child {
  transform: scale(0.95);
}
</style>
