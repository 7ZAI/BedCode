<template>
  <Teleport to="body">
    <Transition name="toast">
      <div
        v-if="visible"
        class="fixed z-[9999] flex items-center gap-3 rounded-lg shadow-lg border toast-root"
        :class="[typeClass, positionClass]"
      >
        <!-- Icon -->
        <div class="flex-shrink-0">
          <svg v-if="type === 'success'" class="toast-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
          </svg>
          <svg v-else-if="type === 'error'" class="toast-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
          <svg v-else-if="type === 'warning'" class="toast-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
          </svg>
          <svg v-else class="toast-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
        </div>

        <!-- Message -->
        <span class="toast-message">{{ message }}</span>

        <!-- Close Button -->
        <button
          v-if="closable"
          @click="close()"
          class="flex-shrink-0 ml-2 hover:opacity-75"
        >
          <svg class="toast-close-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'

interface Props {
  message: string
  type?: 'success' | 'error' | 'warning' | 'info'
  duration?: number
  position?: 'top' | 'bottom'
  closable?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  type: 'info',
  duration: 3000,
  position: 'top',
  closable: true,
})

const emit = defineEmits(['close'])
const visible = ref(false)

const typeClass = computed(() => {
  switch (props.type) {
    case 'success':
      return 'bg-[var(--mobile-success)] text-white'
    case 'error':
      return 'bg-[var(--mobile-error)] text-white'
    case 'warning':
      return 'bg-[var(--mobile-warning)] text-white'
    default:
      return 'bg-[var(--mobile-bg-card)] border-[var(--mobile-border)] text-[var(--mobile-text-primary)]'
  }
})

const positionClass = computed(() => {
  return props.position === 'top' ? 'top-4 left-1/2 -translate-x-1/2' : 'bottom-4 left-1/2 -translate-x-1/2'
})

function close() {
  visible.value = false
  emit('close')
}

let timer: ReturnType<typeof setTimeout> | null = null

watch(visible, (val) => {
  if (val && props.duration > 0) {
    timer = setTimeout(() => {
      close()
    }, props.duration)
  } else if (timer) {
    clearTimeout(timer)
    timer = null
  }
})

onMounted(() => {
  visible.value = true
})
</script>

<style scoped>
.toast-root {
  --toast-icon: clamp(1rem, 1.25rem, 1.5rem);
  --toast-font: clamp(0.75rem, 0.875rem, 1rem);
  --toast-px: clamp(0.75rem, 1rem, 1.25rem);
  --toast-py: clamp(0.5rem, 0.75rem, 1rem);
  padding: var(--toast-py) var(--toast-px);
  max-width: clamp(280px, 80vw, 420px);
}

.toast-icon {
  width: var(--toast-icon);
  height: var(--toast-icon);
}

.toast-message {
  font-size: var(--toast-font);
  font-weight: 500;
}

.toast-close-icon {
  width: calc(var(--toast-icon) * 0.8);
  height: calc(var(--toast-icon) * 0.8);
}

.toast-enter-active,
.toast-leave-active {
  transition: all 0.3s ease;
}

.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(-10px);
}
</style>
