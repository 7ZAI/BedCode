<template>
  <div
    class="rounded animate-pulse bg-slate-200 dark:bg-dark-700"
    :class="[shapeClass, customClass]"
    :style="customStyle"
  ></div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

interface Props {
  shape?: 'text' | 'circle' | 'rect'
  width?: string | number
  height?: string | number
  rows?: number
}

const props = withDefaults(defineProps<Props>(), {
  shape: 'text',
})

const shapeClass = computed(() => {
  switch (props.shape) {
    case 'text':
      return 'h-4 w-full' // 默认一行文字高度
    case 'circle':
      return 'rounded-full'
    case 'rect':
      return '' // 矩形由 width/height 控制
    default:
      return 'h-4 w-full'
  }
})

const customClass = computed(() => {
  // 如果是 circle 且没有指定尺寸，使用默认尺寸
  if (props.shape === 'circle' && !props.width && !props.height) {
    return 'w-10 h-10'
  }
  return ''
})

const customStyle = computed(() => {
  const style: Record<string, string> = {}

  if (props.width) {
    style.width = typeof props.width === 'number' ? `${props.width}px` : props.width
  }
  if (props.height) {
    style.height = typeof props.height === 'number' ? `${props.height}px` : props.height
  }

  return style
})
</script>