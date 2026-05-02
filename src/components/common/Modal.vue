<template>
  <Teleport to="body">
    <Transition name="modal">
      <div
        v-if="modelValue"
        class="fixed inset-0 z-50 flex items-center justify-center p-4"
        @click.self="closeOnBackdrop && close()"
      >
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/60 backdrop-blur-sm"></div>

        <!-- Modal Content -->
        <div
          class="relative bg-dark-800 rounded-xl shadow-2xl border border-dark-700"
          :class="[sizeClass]"
        >
          <!-- Header -->
          <div v-if="title || $slots.header" class="px-6 py-4 border-b border-dark-700">
            <slot name="header">
              <h3 class="text-lg font-semibold text-white">{{ title }}</h3>
            </slot>
          </div>

          <!-- Body -->
          <div class="p-6">
            <slot></slot>
          </div>

          <!-- Footer -->
          <div v-if="$slots.footer" class="px-6 py-4 border-t border-dark-700">
            <slot name="footer"></slot>
          </div>

          <!-- Close Button -->
          <button
            v-if="closable"
            @click="close()"
            class="absolute top-4 right-4 text-dark-400 hover:text-white transition-colors"
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
