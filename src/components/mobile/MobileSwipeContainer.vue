<template>
  <div
    ref="containerRef"
    class="swipe-container"
  >
    <!-- 连接页面 -->
    <div class="swipe-page">
      <DevicesView />
    </div>

    <!-- 会话页面 -->
    <div class="swipe-page">
      <SessionsView />
    </div>

    <!-- 快捷页面 -->
    <div class="swipe-page">
      <QuickActionsView />
    </div>

    <!-- 设置页面 -->
    <div class="swipe-page">
      <SettingsView />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import DevicesView from '@/views/mobile/DevicesView.vue'
import SessionsView from '@/views/mobile/SessionsView.vue'
import QuickActionsView from '@/views/mobile/QuickActionsView.vue'
import SettingsView from '@/views/mobile/SettingsView.vue'

const route = useRoute()
const router = useRouter()
const containerRef = ref<HTMLElement | null>(null)
const currentPage = ref(0)

// 路由名称到页面索引的映射
const pageRoutes: Record<string, number> = {
  'mobile-devices': 0,
  'mobile-sessions': 1,
  'mobile-quick-actions': 2,
  'mobile-settings': 3
}

// 初始化页面
function initPage() {
  // 优先检查查询参数
  const queryPage = route.query.page
  if (queryPage) {
    const page = parseInt(queryPage as string, 10)
    if (!isNaN(page) && page >= 0 && page <= 3) {
      currentPage.value = page
      scrollToPage(page, false)
      return
    }
  }

  // 其次检查路由名称
  const name = route.name as string
  if (name && pageRoutes[name] !== undefined) {
    currentPage.value = pageRoutes[name]
    scrollToPage(currentPage.value, false)
  }
}

// 监听路由变化，同步页面（同时监听路由名称和查询参数）
watch([() => route.name, () => route.query], ([name, query]) => {
  // 优先检查查询参数
  if (query.page) {
    const page = parseInt(query.page as string, 10)
    if (!isNaN(page) && page >= 0 && page <= 3 && page !== currentPage.value) {
      currentPage.value = page
      scrollToPage(page, false)
      return
    }
  }

  // 其次检查路由名称
  if (name && pageRoutes[name as string] !== undefined) {
    const targetPage = pageRoutes[name as string]
    if (targetPage !== currentPage.value) {
      currentPage.value = targetPage
      scrollToPage(targetPage, false)
    }
  }
}, { immediate: true })

onMounted(() => {
  initPage()
})

onUnmounted(() => {
  // Cleanup if needed
})

// 触摸滑动相关变量 - 已禁用滑动切换功能
// let startX = 0
// let startY = 0
// let isDragging = false
// const threshold = 50

// function handleTouchStart(e: TouchEvent) {
//   startX = e.touches[0].clientX
//   startY = e.touches[0].clientY
//   isDragging = true
// }

// function handleTouchMove(e: TouchEvent) {
//   if (!isDragging) return

//   const deltaX = e.touches[0].clientX - startX
//   const deltaY = e.touches[0].clientY - startY

//   // 忽略垂直滑动
//   if (Math.abs(deltaY) > Math.abs(deltaX)) return

//   // 阻止默认滚动行为
//   if (containerRef.value) {
//     containerRef.value.style.overflow = 'hidden'
//   }
// }

// function handleTouchEnd(e: TouchEvent) {
//   if (!isDragging) return

//   const endX = e.changedTouches[0].clientX
//   const deltaX = endX - startX

//   // 根据滑动方向和距离决定是否切换页面
//   if (Math.abs(deltaX) > threshold) {
//     if (deltaX < 0) {
//       // 向左滑 -> 下一页
//       if (currentPage.value < 3) {
//         currentPage.value++
//         scrollToPage(currentPage.value)
//         syncRoute(currentPage.value)
//       }
//     } else {
//       // 向右滑 -> 上一页
//       if (currentPage.value > 0) {
//         currentPage.value--
//         scrollToPage(currentPage.value)
//         syncRoute(currentPage.value)
//       }
//     }
//   }

//   isDragging = false

//   // 恢复滚动
//   if (containerRef.value) {
//     containerRef.value.style.overflow = ''
//   }
// }

function scrollToPage(page: number, smooth = true) {
  if (!containerRef.value) return

  const containerWidth = window.innerWidth
  containerRef.value.scrollTo({
    left: page * containerWidth,
    behavior: smooth ? 'smooth' : 'auto'
  })
}
</script>

<style scoped>
.swipe-container {
  display: flex;
  width: 100%;
  height: 100%;
  overflow-x: hidden;
  overflow-y: auto;
  scroll-snap-type: x mandatory;
  scroll-behavior: smooth;
  -webkit-overflow-scrolling: touch;
}

.swipe-page {
  flex: 0 0 100%;
  width: 100%;
  height: 100%;
  overflow-y: auto;
  scroll-snap-align: start;
}
</style>