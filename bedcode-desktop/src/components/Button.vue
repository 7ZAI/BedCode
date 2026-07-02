<template>
  <button
    :type="type"
    :disabled="disabled || loading"
    :title="title"
    class="inline-flex items-center justify-center gap-2 font-medium rounded-btn transition-all duration-200 focus:outline-none focus:ring-2 focus:ring-offset-2 dark:focus:ring-offset-dark-900"
    :class="[variantClass, sizeClass, { 'opacity-50 cursor-not-allowed': disabled || loading }]"
    @click="$emit('click', $event)"
  >
    <!-- Loading Spinner -->
    <svg
      v-if="loading"
      class="w-4 h-4 animate-spin"
      fill="none"
      viewBox="0 0 24 24"
    >
      <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
      <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
    </svg>

    <!-- Icon Slot -->
    <slot name="icon"></slot>

    <!-- Content -->
    <slot></slot>
  </button>
</template>

<script setup lang="ts">
import { computed } from 'vue'

interface Props {
  variant?: 'primary' | 'secondary' | 'danger' | 'ghost'
  size?: 'sm' | 'md' | 'lg'
  type?: 'button' | 'submit' | 'reset'
  disabled?: boolean
  loading?: boolean
  title?: string
}

const props = withDefaults(defineProps<Props>(), {
  variant: 'primary',
  size: 'md',
  type: 'button',
  disabled: false,
  loading: false,
  title: '',
})

defineEmits(['click'])

const variantClass = computed(() => {
  switch (props.variant) {
    case 'primary':
      return 'bg-brand hover:bg-[var(--color-primary-hover)] text-white focus:ring-brand shadow-xs hover:shadow-sm'
    case 'secondary':
      return 'bg-card hover:bg-[var(--bg-hover)] text-[var(--text-primary)] border border-[var(--border)] focus:ring-[var(--border)]'
    case 'danger':
      return 'bg-[var(--color-danger-light)] hover:bg-red-100 dark:hover:bg-red-900/30 text-red-600 dark:text-red-400 focus:ring-red-500'
    case 'ghost':
      return 'bg-transparent hover:bg-[var(--bg-hover)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] focus:ring-[var(--border)]'
    default:
      return 'bg-brand hover:bg-[var(--color-primary-hover)] text-white focus:ring-brand shadow-xs hover:shadow-sm'
  }
})

const sizeClass = computed(() => {
  switch (props.size) {
    case 'sm':
      return 'h-8 px-3 text-xs'
    case 'md':
      return 'h-10 px-4 text-sm'
    case 'lg':
      return 'h-10 px-6 text-sm'
    default:
      return 'h-10 px-4 text-sm'
  }
})
</script>
