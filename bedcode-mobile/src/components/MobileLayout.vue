<template>
  <div
    v-if="safeAreaReady"
    class="flex flex-col h-[100dvh] overflow-hidden mobile-app mobile-ui bg-[var(--mobile-bg-primary)]"
    :style="mobileContainerStyle"
  >
    <!-- Main Content -->
    <main class="flex-1 min-h-0">
      <router-view v-slot="{ Component, route }">
        <!-- TerminalView 不使用 keep-alive：buffer store 持有数据，组件正常销毁/重建 -->
        <!-- MobileSwipeContainer 保持 keep-alive 缓存 -->
        <keep-alive v-if="route.name !== 'mobile-terminal'" :include="['MobileSwipeContainer']">
          <component :is="Component" />
        </keep-alive>
        <component v-else :is="Component" :key="route.fullPath" />
      </router-view>
    </main>

    <!-- Bottom Navigation (hide on terminal view) -->
    <MobileNav v-if="!isTerminalRoute" />
  </div>
  <!-- 安全区域初始化前的占位，避免内容在状态栏下闪现 -->
  <div v-else class="h-[100dvh] mobile-app mobile-ui bg-[var(--mobile-bg-primary)]" />
</template>

<script setup lang="ts">
import { computed, inject, type Ref } from 'vue'
import { useRoute } from 'vue-router'
import MobileNav from '@/components/MobileNav.vue'

const route = useRoute()

const isTerminalRoute = computed(() => {
  return route.name === 'mobile-terminal'
})

// 从 App.vue inject 的安全区域信息
const safeArea = inject<Ref<{ top: number; bottom: number }>>('safeArea')!
const safeAreaReady = inject<Ref<boolean>>('safeAreaReady')!
const platformInfo = inject<Ref<{ isMobile: boolean }>>('platformInfo')!

// 移动端容器样式：顶部安全区由容器 padding 处理
// 底部安全区由各底部元素（MobileNav、TerminalInputBar）的 paddingBottom 承担
// 注意：Android WebView 不支持 CSS env(safe-area-inset-*)，完全依赖 JS 值
// 通过 safeAreaReady 守卫确保 safeArea 初始化后才渲染内容
const mobileContainerStyle = computed(() => {
  if (!platformInfo.value.isMobile) return {}

  const top = safeArea.value.top || 0

  return {
    paddingTop: `${top}px`,
  }
})
</script>
