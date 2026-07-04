<template>
  <div class="inline-flex items-center justify-center" :class="sizeClass">
    <!-- Circle Spinner (默认) -->
    <svg
      v-if="variant === 'circle'"
      class="animate-spin"
      :class="colorClass"
      fill="none"
      viewBox="0 0 24 24"
    >
      <circle
        class="opacity-25"
        cx="12"
        cy="12"
        r="10"
        stroke="currentColor"
        stroke-width="4"
      ></circle>
      <path
        class="opacity-75"
        fill="currentColor"
        d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
      ></path>
    </svg>

    <!-- Dots Spinner -->
    <div v-else-if="variant === 'dots'" class="flex gap-1">
      <span
        v-for="i in 3"
        :key="i"
        class="rounded-full animate-bounce"
        :class="[colorClass, dotSizeClass]"
        :style="{ animationDelay: `${(i - 1) * 150}ms` }"
      ></span>
    </div>

    <!-- Pulse Spinner -->
    <div v-else-if="variant === 'pulse'" class="relative">
      <span
        class="absolute inset-0 rounded-full animate-ping"
        :class="[colorClass, pulseSizeClass]"
      ></span>
      <span
        class="relative rounded-full"
        :class="[colorClass, pulseSizeClass, 'opacity-75']"
      ></span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

interface Props {
  size?: 'sm' | 'md' | 'lg' | 'xl'
  color?: 'primary' | 'white' | 'dark' | 'danger' | 'success' | 'warning'
  variant?: 'circle' | 'dots' | 'pulse'
}

const props = withDefaults(defineProps<Props>(), {
  size: 'md',
  color: 'primary',
  variant: 'circle',
})

const sizeClass = computed(() => {
  switch (props.size) {
    case 'sm':
      return 'w-4 h-4'
    case 'md':
      return 'w-5 h-5'
    case 'lg':
      return 'w-6 h-6'
    case 'xl':
      return 'w-8 h-8'
    default:
      return 'w-5 h-5'
  }
})

const colorClass = computed(() => {
  switch (props.color) {
    case 'primary':
      return 'text-primary-500'
    case 'white':
      return 'text-white'
    case 'dark':
      return 'text-gray-400 dark:text-dark-400'
    case 'danger':
      return 'text-red-500'
    case 'success':
      return 'text-green-500'
    case 'warning':
      return 'text-yellow-500'
    default:
      return 'text-primary-500'
  }
})

const dotSizeClass = computed(() => {
  switch (props.size) {
    case 'sm':
      return 'w-1.5 h-1.5'
    case 'md':
      return 'w-2 h-2'
    case 'lg':
      return 'w-2.5 h-2.5'
    case 'xl':
      return 'w-3 h-3'
    default:
      return 'w-2 h-2'
  }
})

const pulseSizeClass = computed(() => {
  switch (props.size) {
    case 'sm':
      return 'w-3 h-3'
    case 'md':
      return 'w-4 h-4'
    case 'lg':
      return 'w-5 h-5'
    case 'xl':
      return 'w-6 h-6'
    default:
      return 'w-4 h-4'
  }
})
</script>