<template>
  <Teleport to="body">
    <Transition name="toast">
      <div
        v-if="visible"
        class="fixed z-50 flex items-center gap-3 px-4 py-3 rounded-lg shadow-lg border"
        :class="[typeClass, positionClass]"
      >
        <!-- Icon -->
        <div class="flex-shrink-0">
          <svg v-if="type === 'success'" class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
          </svg>
          <svg v-else-if="type === 'error'" class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
          <svg v-else-if="type === 'warning'" class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
          </svg>
          <svg v-else class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
        </div>

        <!-- Message -->
        <span class="text-sm font-medium">{{ message }}</span>

        <!-- Close Button -->
        <button
          v-if="closable"
          @click="close()"
          class="flex-shrink-0 ml-2 hover:opacity-75"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
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
      return 'bg-green-500 border-green-600 text-white'
    case 'error':
      return 'bg-red-500 border-red-600 text-white'
    case 'warning':
      return 'bg-yellow-500 border-yellow-600 text-white'
    default:
      return 'bg-card border-[var(--border)] text-[var(--text-primary)]'
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
