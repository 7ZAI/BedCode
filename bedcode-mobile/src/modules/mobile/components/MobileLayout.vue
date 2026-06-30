<template>
  <div
    v-if="safeAreaReady"
    class="flex flex-col h-screen mobile-app mobile-ui"
    :style="mobileContainerStyle"
  >
    <!-- Main Content -->
    <main class="flex-1 min-h-0">
      <router-view v-slot="{ Component, route }">
        <!-- 终端页面使用 fullPath 作为 key，确保每个会话有独立的缓存实例 -->
        <!-- 其他 keepAlive 页面使用组件名称缓存 -->
        <keep-alive :include="cachedMobileRoutes" :max="maxCachedTerminals">
          <component :is="Component" :key="getKey(route)" />
        </keep-alive>
      </router-view>
    </main>

    <!-- Bottom Navigation (hide on terminal view) -->
    <MobileNav v-if="!isTerminalRoute" />
  </div>
  <!-- 安全区域初始化前的占位，避免内容在状态栏下闪现 -->
  <div v-else class="h-screen mobile-app mobile-ui bg-[var(--mobile-bg)]" />
</template>

<script setup lang="ts">
import { computed, inject, type Ref } from 'vue'
import { useRoute } from 'vue-router'
import MobileNav from '@/modules/mobile/components/MobileNav.vue'
import { useSettingsStore } from '@/modules/shared/stores/settings'

const route = useRoute()
const settingsStore = useSettingsStore()

const isTerminalRoute = computed(() => {
  return route.name === 'mobile-terminal'
})

// 需要 KeepAlive 缓存的移动端组件名称
// MobileSwipeContainer 包含 4 个子页面（设备、会话、快捷操作、设置），缓存以保持切换后数据
const cachedMobileRoutes = ['TerminalView', 'MobileSwipeContainer']
const maxCachedTerminals = computed(() => settingsStore.settings.ui.max_cached_terminals || 10)

// 为 KeepAlive 生成 key
// 终端页面使用 fullPath（包含会话 ID），确保每个会话有独立实例
// 其他页面使用组件名称
function getKey(route: any): string {
  if (route.name === 'mobile-terminal') {
    return route.fullPath
  }
  return route.name || route.fullPath
}

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
