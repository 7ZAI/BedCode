<template>
  <button
    :type="type"
    :disabled="disabled || loading"
    :title="title"
    class="inline-flex items-center justify-center gap-2 font-medium rounded-lg transition-all duration-200 focus:outline-none focus:ring-2 focus:ring-offset-2 dark:focus:ring-offset-dark-900"
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
      return 'bg-primary-600 hover:bg-primary-700 text-white focus:ring-primary-500'
    case 'secondary':
      return 'bg-slate-100 dark:bg-dark-700 hover:bg-slate-200 dark:hover:bg-dark-600 text-slate-800 dark:text-white border border-slate-300 dark:border-dark-600 focus:ring-slate-400'
    case 'danger':
      return 'bg-red-600 hover:bg-red-700 text-white focus:ring-red-500'
    case 'ghost':
      return 'bg-transparent hover:bg-slate-100 dark:hover:bg-dark-700 text-slate-700 dark:text-dark-300 hover:text-slate-900 dark:hover:text-white focus:ring-slate-400'
    default:
      return 'bg-primary-600 hover:bg-primary-700 text-white focus:ring-primary-500'
  }
})

const sizeClass = computed(() => {
  switch (props.size) {
    case 'sm':
      return 'px-3 py-1.5 text-sm'
    case 'md':
      return 'px-4 py-2 text-sm'
    case 'lg':
      return 'px-6 py-3 text-base'
    default:
      return 'px-4 py-2 text-sm'
  }
})
</script>
