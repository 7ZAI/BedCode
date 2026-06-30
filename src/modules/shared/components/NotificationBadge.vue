<template>
  <div class="relative inline-flex">
    <!-- Trigger (包裹的内容) -->
    <slot></slot>

    <!-- Badge -->
    <span
      v-if="showBadge"
      class="absolute flex items-center justify-center font-medium leading-none"
      :class="[badgeClass, positionClass, sizeClass]"
    >
      <!-- Dot 模式 -->
      <span v-if="variant === 'dot'" class="w-full h-full rounded-full"></span>

      <!-- Count 模式 -->
      <span v-else>{{ displayCount }}</span>
    </span>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

interface Props {
  count?: number
  max?: number
  variant?: 'count' | 'dot'
  color?: 'primary' | 'danger' | 'warning' | 'success'
  position?: 'top-right' | 'top-left' | 'bottom-right' | 'bottom-left'
  size?: 'sm' | 'md' | 'lg'
}

const props = withDefaults(defineProps<Props>(), {
  count: 0,
  max: 99,
  variant: 'count',
  color: 'danger',
  position: 'top-right',
  size: 'md',
})

const showBadge = computed(() => {
  if (props.variant === 'dot') return true
  return props.count > 0
})

const displayCount = computed(() => {
  if (props.variant === 'dot') return ''
  if (props.count > props.max) return `${props.max}+`
  return props.count.toString()
})

const badgeClass = computed(() => {
  const base = 'rounded-full'

  if (props.variant === 'dot') {
    // Dot 模式：整个 span 是背景
    switch (props.color) {
      case 'primary':
        return `${base} bg-primary-500`
      case 'danger':
        return `${base} bg-red-500`
      case 'warning':
        return `${base} bg-yellow-500`
      case 'success':
        return `${base} bg-green-500`
      default:
        return `${base} bg-red-500`
    }
  } else {
    // Count 模式：有背景和文字
    switch (props.color) {
      case 'primary':
        return `${base} bg-primary-500 text-white`
      case 'danger':
        return `${base} bg-red-500 text-white`
      case 'warning':
        return `${base} bg-yellow-500 text-dark-900`
      case 'success':
        return `${base} bg-green-500 text-white`
      default:
        return `${base} bg-red-500 text-white`
    }
  }
})

const positionClass = computed(() => {
  switch (props.position) {
    case 'top-right':
      return '-top-1 -right-1'
    case 'top-left':
      return '-top-1 -left-1'
    case 'bottom-right':
      return '-bottom-1 -right-1'
    case 'bottom-left':
      return '-bottom-1 -left-1'
    default:
      return '-top-1 -right-1'
  }
})

const sizeClass = computed(() => {
  if (props.variant === 'dot') {
    switch (props.size) {
      case 'sm':
        return 'w-2 h-2'
      case 'md':
        return 'w-2.5 h-2.5'
      case 'lg':
        return 'w-3 h-3'
      default:
        return 'w-2.5 h-2.5'
    }
  } else {
    switch (props.size) {
      case 'sm':
        return 'min-w-4 h-4 px-1 text-xs'
      case 'md':
        return 'min-w-5 h-5 px-1.5 text-xs'
      case 'lg':
        return 'min-w-6 h-6 px-2 text-sm'
      default:
        return 'min-w-5 h-5 px-1.5 text-xs'
    }
  }
})
</script>