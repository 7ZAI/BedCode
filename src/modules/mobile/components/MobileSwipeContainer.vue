<template>
  <div class="swipe-container" ref="containerRef" @touchstart="handleTouchStart" @touchmove="handleTouchMove" @touchend="handleTouchEnd" @touchcancel="handleTouchEnd">
    <div
      class="swipe-track"
      ref="trackRef"
      :class="{ dragging: isDragging }"
      :style="trackStyle"
    >
      <div class="swipe-page">
        <DevicesView />
      </div>
      <div class="swipe-page">
        <SessionsView />
      </div>
      <div class="swipe-page">
        <ToolboxView />
      </div>
      <div class="swipe-page">
        <SettingsView />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, onActivated, watch, provide } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import DevicesView from '@/modules/mobile/views/DevicesView.vue'
import SessionsView from '@/modules/mobile/views/SessionsView.vue'
import ToolboxView from '@/modules/mobile/views/ToolboxView.vue'
import SettingsView from '@/modules/mobile/views/SettingsView.vue'

// 定义组件名称，用于 keep-alive 缓存
defineOptions({
  name: 'MobileSwipeContainer'
})

const route = useRoute()
const router = useRouter()
const containerRef = ref<HTMLElement | null>(null)
const trackRef = ref<HTMLElement | null>(null)

// 页面配置
const pages = [
  { name: 'mobile-devices', component: DevicesView },
  { name: 'mobile-sessions', component: SessionsView },
  { name: 'mobile-toolbox', component: ToolboxView },
  { name: 'mobile-settings', component: SettingsView }
]

// 状态
const currentPage = ref(0)
const translateX = ref(0)
const isDragging = ref(false)
const isAnimating = ref(false)

// 触摸状态
let startX = 0
let startY = 0
let startTime = 0
let lastX = 0
let direction: 'horizontal' | 'vertical' | null = null

// 参数配置
const CONFIG = {
  directionThreshold: 20,
  swipeThreshold: 80,
  velocityThreshold: 0.3,
  maxOvershoot: 50,
  animationDuration: 300
}

// 计算轨道样式（拖动时无过渡，松手后 CSS 动画平滑滑动）
const trackStyle = computed(() => ({
  transform: `translate3d(${translateX.value}px, 0, 0)`,
  transition: isDragging.value
    ? 'none'
    : `transform ${CONFIG.animationDuration}ms cubic-bezier(0.4, 0, 0.2, 1)`
}))

// 初始化页面
function initPage() {
  const queryPage = route.query.page
  if (queryPage) {
    const page = parseInt(queryPage as string, 10)
    if (!isNaN(page) && page >= 0 && page <= 3) {
      currentPage.value = page
      translateX.value = -page * window.innerWidth
      return
    }
  }

  const name = route.name as string
  const pageIndex = pages.findIndex(p => p.name === name)
  if (pageIndex !== -1) {
    currentPage.value = pageIndex
    translateX.value = -pageIndex * window.innerWidth
  }
}

// 同步路由（只更新 query，不切换到独立页面路由以免容器被卸载）
function syncRoute(page: number) {
  router.replace({ name: 'mobile-home', query: { page: page.toString() } })
}

// 切换到指定页面
function goToPage(page: number, animate = true) {
  if (page < 0 || page > 3 || page === currentPage.value) return

  isAnimating.value = animate
  currentPage.value = page
  translateX.value = -page * window.innerWidth

  syncRoute(page)

  setTimeout(() => {
    isAnimating.value = false
  }, animate ? CONFIG.animationDuration : 0)
}

/** 重置触摸状态，确保从终端返回后滑动功能正常 */
function resetTouchState() {
  isDragging.value = false
  isAnimating.value = false
  direction = null
  // 修正 translateX 与当前页面同步（窗口大小可能在停用期间变化）
  translateX.value = -currentPage.value * window.innerWidth
}

// 触摸事件处理
function handleTouchStart(e: TouchEvent) {
  if (isAnimating.value) return

  startX = e.touches[0].clientX
  startY = e.touches[0].clientY
  startTime = Date.now()
  lastX = startX
  direction = null
  isDragging.value = true
}

function handleTouchMove(e: TouchEvent) {
  if (!isDragging.value || direction === 'vertical') return

  const deltaX = e.touches[0].clientX - startX
  const deltaY = e.touches[0].clientY - startY

  // 首次移动确定方向
  if (!direction) {
    if (Math.abs(deltaX) > CONFIG.directionThreshold || Math.abs(deltaY) > CONFIG.directionThreshold) {
      direction = Math.abs(deltaX) > Math.abs(deltaY) ? 'horizontal' : 'vertical'

      if (direction === 'vertical') {
        // 垂直滑动时不阻止默认行为，让子元素（如终端）可以正常滚动
        isDragging.value = false
        direction = null
        return
      }
    } else {
      return
    }
  }

  // 水平滑动
  if (direction === 'horizontal') {
    e.preventDefault()

    const containerWidth = window.innerWidth
    const baseTranslate = -currentPage.value * containerWidth
    let newTranslate = baseTranslate + deltaX

    // 边界弹性处理
    if (currentPage.value === 0 && deltaX > 0) {
      newTranslate = baseTranslate + deltaX * 0.3
    } else if (currentPage.value === 3 && deltaX < 0) {
      newTranslate = baseTranslate + deltaX * 0.3
    }

    translateX.value = newTranslate
    lastX = e.touches[0].clientX
  }
}

function handleTouchEnd(e: TouchEvent) {
  if (!isDragging.value || direction !== 'horizontal') {
    isDragging.value = false
    direction = null
    return
  }

  const endX = e.changedTouches[0].clientX
  const deltaX = endX - startX
  const deltaTime = Date.now() - startTime
  const velocity = Math.abs(deltaX) / deltaTime

  const containerWidth = window.innerWidth
  const shouldSwipe = Math.abs(deltaX) > CONFIG.swipeThreshold || velocity > CONFIG.velocityThreshold

  if (shouldSwipe) {
    if (deltaX < 0 && currentPage.value < 3) {
      goToPage(currentPage.value + 1)
    } else if (deltaX > 0 && currentPage.value > 0) {
      goToPage(currentPage.value - 1)
    } else {
      translateX.value = -currentPage.value * containerWidth
    }
  } else {
    translateX.value = -currentPage.value * containerWidth
  }

  isDragging.value = false
  direction = null
}

// 路由监听
watch(() => route.query.page, (queryPage) => {
  if (queryPage) {
    const page = parseInt(queryPage as string, 10)
    if (!isNaN(page) && page >= 0 && page <= 3 && page !== currentPage.value) {
      goToPage(page, false)
    }
  }
})

// 窗口大小变化
function handleResize() {
  translateX.value = -currentPage.value * window.innerWidth
}

onMounted(() => {
  initPage()
  window.addEventListener('resize', handleResize)
})

onUnmounted(() => {
  window.removeEventListener('resize', handleResize)
})

// keep-alive 激活时重置触摸状态，确保从终端返回后滑动功能正常
// 停用期间窗口大小可能变化（键盘弹出/收起、旋转等），translateX 需要重新同步
// 触摸状态也可能残留（如导航离开时触摸序列未完成），需要清除
onActivated(() => {
  resetTouchState()
})

// 暴露给导航组件使用
defineExpose({
  goToPage,
  currentPage
})

// 提供给子组件的上下文
provide('swipeContainer', {
  goToPage,
  currentPage
})
</script>

<style scoped>
.swipe-container {
  position: relative;
  width: 100%;
  height: 100%;
  overflow: hidden;
  /* 允许浏览器垂直滚动，但水平手势由 JS 处理
   * 确保浏览器不会消费水平滑动用于 overscroll 效果 */
  touch-action: pan-y;
}

.swipe-track {
  display: flex;
  width: 100%;
  height: 100%;
  will-change: transform;
}

.swipe-page {
  flex: 0 0 100%;
  width: 100%;
  height: 100%;
  overflow-y: auto;
  overflow-x: hidden;
  -webkit-overflow-scrolling: touch;
  /* 允许垂直滚动，但禁止水平方向的默认手势（避免与 swipe 冲突） */
  touch-action: pan-y;
  /* 禁止 overscroll 效果（橡皮筋/发光），防止浏览器劫持水平滑动手势
   * 不加此属性时，在子页面滚动到边界后水平滑动会被浏览器吞掉 */
  overscroll-behavior: none;
}
</style>