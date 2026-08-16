<template>
  <button
    :type="type"
    :disabled="disabled || loading"
    :title="title"
    class="inline-flex items-center justify-center gap-2 font-medium rounded-lg transition-colors duration-200 focus:outline-none focus:ring-2 focus:ring-offset-2"
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
      return 'bg-[var(--mobile-accent)] hover:bg-[color:color-mix(in_srgb,var(--mobile-accent)_80%,transparent)] text-[var(--mobile-text-on-accent)] focus:ring-[var(--mobile-accent)]'
    case 'secondary':
      return 'bg-[var(--mobile-bg-elevated)] hover:bg-[var(--mobile-bg-secondary)] text-[var(--mobile-text-secondary)] border border-[var(--mobile-border)] focus:ring-[var(--mobile-border)] active:opacity-80'
    case 'danger':
      return 'bg-[var(--mobile-error)] hover:bg-[color:color-mix(in_srgb,var(--mobile-error)_80%,transparent)] text-[var(--mobile-text-on-accent)] focus:ring-[var(--mobile-error)]'
    case 'ghost':
      return 'bg-transparent hover:bg-[var(--mobile-accent-muted)] text-[var(--mobile-text-secondary)] hover:text-[var(--mobile-accent)] focus:ring-[var(--mobile-border)] active:opacity-80'
    default:
      return 'bg-[var(--mobile-accent)] hover:bg-[color:color-mix(in_srgb,var(--mobile-accent)_80%,transparent)] text-[var(--mobile-text-on-accent)] focus:ring-[var(--mobile-accent)]'
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
