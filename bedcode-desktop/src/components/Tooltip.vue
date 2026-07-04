<template>
  <div class="relative inline-block" ref="triggerRef" @mouseenter="show" @mouseleave="hide">
    <!-- Trigger Element -->
    <slot></slot>

    <!-- Tooltip Content -->
    <Teleport to="body">
      <Transition name="tooltip">
        <div
          v-if="visible"
          ref="tooltipRef"
          class="fixed z-50 px-3 py-1.5 text-sm rounded-btn shadow-lg border whitespace-nowrap bg-card text-[var(--text-primary)] border-[var(--border)]"
          :style="positionStyle"
        >
          {{ content }}
          <!-- Arrow -->
          <div
            class="absolute w-2 h-2 bg-card border-[var(--border)] rotate-45"
            :class="arrowClass"
          ></div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'

interface Props {
  content: string
  position?: 'top' | 'bottom' | 'left' | 'right'
  delay?: number
}

const props = withDefaults(defineProps<Props>(), {
  position: 'top',
  delay: 200,
})

const triggerRef = ref<HTMLElement | null>(null)
const tooltipRef = ref<HTMLElement | null>(null)
const visible = ref(false)
const positionStyle = ref({})
let showTimer: ReturnType<typeof setTimeout> | null = null
let hideTimer: ReturnType<typeof setTimeout> | null = null

const arrowClass = computed(() => {
  switch (props.position) {
    case 'top':
      return 'bottom-[-4px] left-1/2 -translate-x-1/2 border-b border-r'
    case 'bottom':
      return 'top-[-4px] left-1/2 -translate-x-1/2 border-t border-l'
    case 'left':
      return 'right-[-4px] top-1/2 -translate-y-1/2 border-t border-r'
    case 'right':
      return 'left-[-4px] top-1/2 -translate-y-1/2 border-b border-l'
    default:
      return 'bottom-[-4px] left-1/2 -translate-x-1/2 border-b border-r'
  }
})

function calculatePosition() {
  if (!triggerRef.value || !tooltipRef.value) return

  const triggerRect = triggerRef.value.getBoundingClientRect()
  const tooltipRect = tooltipRef.value.getBoundingClientRect()

  const gap = 8

  let top = 0
  let left = 0

  switch (props.position) {
    case 'top':
      top = triggerRect.top - tooltipRect.height - gap
      left = triggerRect.left + (triggerRect.width - tooltipRect.width) / 2
      break
    case 'bottom':
      top = triggerRect.bottom + gap
      left = triggerRect.left + (triggerRect.width - tooltipRect.width) / 2
      break
    case 'left':
      top = triggerRect.top + (triggerRect.height - tooltipRect.height) / 2
      left = triggerRect.left - tooltipRect.width - gap
      break
    case 'right':
      top = triggerRect.top + (triggerRect.height - tooltipRect.height) / 2
      left = triggerRect.right + gap
      break
  }

  // 边界修正，确保 tooltip 不超出视窗
  const viewportWidth = window.innerWidth
  const viewportHeight = window.innerHeight

  if (left < 0) left = gap
  if (left + tooltipRect.width > viewportWidth) left = viewportWidth - tooltipRect.width - gap
  if (top < 0) top = gap
  if (top + tooltipRect.height > viewportHeight) top = viewportHeight - tooltipRect.height - gap

  positionStyle.value = {
    top: `${top}px`,
    left: `${left}px`,
  }
}

function show() {
  if (hideTimer) {
    clearTimeout(hideTimer)
    hideTimer = null
  }
  showTimer = setTimeout(() => {
    visible.value = true
    // 等待 DOM 更新后计算位置
    requestAnimationFrame(() => {
      calculatePosition()
    })
  }, props.delay)
}

function hide() {
  if (showTimer) {
    clearTimeout(showTimer)
    showTimer = null
  }
  hideTimer = setTimeout(() => {
    visible.value = false
  }, 100)
}

// 监听窗口滚动和大小变化，更新 tooltip 位置
function onScroll() {
  if (visible.value) {
    calculatePosition()
  }
}

onMounted(() => {
  window.addEventListener('scroll', onScroll, true)
  window.addEventListener('resize', onScroll)
})

onUnmounted(() => {
  window.removeEventListener('scroll', onScroll, true)
  window.removeEventListener('resize', onScroll)
  if (showTimer) clearTimeout(showTimer)
  if (hideTimer) clearTimeout(hideTimer)
})
</script>

<style scoped>
.tooltip-enter-active,
.tooltip-leave-active {
  transition: opacity 0.15s ease;
}

.tooltip-enter-from,
.tooltip-leave-to {
  opacity: 0;
}
</style>