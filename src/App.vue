<template>
  <div :class="themeClasses.container">
    <!-- Desktop Layout -->
    <template v-if="isDesktop">
      <!-- 终端窗口路由：不显示任何布局元素 -->
      <template v-if="isTerminalWindow">
        <router-view />
      </template>

      <!-- 普通路由：显示完整布局 -->
      <template v-else>
        <div class="flex flex-col h-screen desktop-ui">
          <!-- Custom Title Bar -->
          <TitleBar />
          <div class="flex flex-1 overflow-hidden">
            <!-- Sidebar -->
            <Sidebar />

            <!-- Main Content -->
            <main class="flex-1 overflow-hidden">
              <router-view />
            </main>
          </div>
        </div>
      </template>
    </template>

    <!-- Mobile Layout -->
    <template v-else>
      <div
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
    </template>

    <!-- Global Toast Container -->
    <ToastContainer />
  </div>
</template>

<script setup lang="ts">
import { computed, provide, watch, onMounted, onUnmounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import TitleBar from '@/modules/desktop/components/TitleBar.vue'
import Sidebar from '@/modules/desktop/components/Sidebar.vue'
import MobileNav from '@/modules/mobile/components/MobileNav.vue'
import { usePlatform } from '@/modules/shared/composables/usePlatform'
import { useSettingsStore } from '@/modules/shared/stores/settings'
import { useGlobalNotifications } from '@/modules/shared/composables/useGlobalNotifications'
import { useOrientation } from '@/modules/mobile/composables/useOrientation'
import { useEdgeToEdge } from '@/modules/mobile/composables/useEdgeToEdge'
import { ToastContainer } from '@/modules/shared/composables/useToast'

const route = useRoute()
const router = useRouter()
const { platformInfo } = usePlatform()
const settingsStore = useSettingsStore()
const { isLandscape, orientation } = useOrientation()

// 移动端 Edge-to-Edge 安全区域
const { safeArea, keyboardInfo, isReady: edgeToEdgeReady } = useEdgeToEdge()

// 主题管理
let systemThemeQuery: MediaQueryList | null = null
const isSystemDark = ref(false)

const systemThemeHandler = (e: MediaQueryListEvent) => {
  document.documentElement.classList.toggle('dark', e.matches)
  isSystemDark.value = e.matches
}

function applyTheme(theme: string) {
  const root = document.documentElement
  let isDark = theme === 'dark'

  // system 主题需要检测系统偏好
  if (theme === 'system') {
    isDark = isSystemDark.value
  }

  if (isDark) {
    root.classList.add('dark')
  } else {
    root.classList.remove('dark')
  }
}

function applyFontSize(size: number) {
  const root = document.documentElement
  // 基础字体大小
  root.style.setProperty('--font-size-base', `${size}px`, 'important')
  root.style.setProperty('--global-font-size', `${size}px`, 'important')

  // 计算其他字体大小（基于基础大小）
  const small = Math.max(10, Math.round(size * 0.85))    // 小号字体
  const medium = size                                     // 中号字体（基础）
  const large = Math.round(size * 1.15)                  // 大号字体
  const xl = Math.round(size * 1.3)                      // 特大号字体
  const xs = Math.max(9, Math.round(size * 0.75))        // 极小号字体

  root.style.setProperty('--font-size-xs', `${xs}px`, 'important')
  root.style.setProperty('--font-size-sm', `${small}px`, 'important')
  root.style.setProperty('--font-size-base', `${medium}px`, 'important')
  root.style.setProperty('--font-size-lg', `${large}px`, 'important')
  root.style.setProperty('--font-size-xl', `${xl}px`, 'important')
}

function setupTheme() {
  const theme = settingsStore.settings.ui.theme

  // 初始化系统主题检测
  if (theme === 'system') {
    isSystemDark.value = window.matchMedia('(prefers-color-scheme: dark)').matches
    systemThemeQuery = window.matchMedia('(prefers-color-scheme: dark)')
    systemThemeQuery.addEventListener('change', systemThemeHandler)
  }

  applyTheme(theme)
}

function setupFontSize() {
  const fontSize = settingsStore.settings.ui.terminal_font_size || 14
  applyFontSize(fontSize)
}

// 初始加载设置后再应用主题
// 注意：设置已经在 main.ts 中预加载，这里直接应用
onMounted(() => {
  // 设置可能已在 main.ts 中加载完成，直接应用主题
  setupTheme()
  setupFontSize()

  // 启动全局通知监听（桌面端）
  if (isDesktop.value) {
    startGlobalNotifications()
  }
})

// 监听主题变化
watch(() => settingsStore.settings.ui.theme, (newTheme) => {
  if (systemThemeQuery) {
    systemThemeQuery.removeEventListener('change', systemThemeHandler)
    systemThemeQuery = null
  }
  applyTheme(newTheme)
  setupTheme()
})

// 监听字体大小变化
watch(() => settingsStore.settings.ui.terminal_font_size, (newSize) => {
  if (newSize) {
    applyFontSize(newSize)
  }
})

onUnmounted(() => {
  if (systemThemeQuery) {
    systemThemeQuery.removeEventListener('change', systemThemeHandler)
  }
  stopGlobalNotifications()
})

// Global keyboard shortcuts
import { useKeyboardShortcuts } from '@/modules/shared/composables/useKeyboardShortcuts'
useKeyboardShortcuts([
  { key: ',', ctrl: true, handler: () => router.push('/settings') },
  { key: '1', ctrl: true, handler: () => router.push('/sessions') },
  { key: '2', ctrl: true, handler: () => router.push('/devices') },
])

// Check if current route is terminal view (hide nav)
const isTerminalRoute = computed(() => {
  return route.name === 'mobile-terminal'
})

// 检测是否为终端窗口路由（隐藏侧边栏和标题栏）
const isTerminalWindow = computed(() => {
  return route.path.startsWith('/terminal-window')
})

// Use platform detection for desktop/mobile layout
const isDesktop = computed(() => platformInfo.value.isDesktop)

// 需要 KeepAlive 缓存的移动端组件名称
// MobileSwipeContainer 包含 4 个子页面（设备、会话、快捷操作、设置），缓存以保持切换后数据
const cachedMobileRoutes = ['TerminalView', 'MobileSwipeContainer']
// 动态获取缓存最大数量
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

// 全局通知监听
const { startListening: startGlobalNotifications, stopListening: stopGlobalNotifications } = useGlobalNotifications()

// 主题对应的类名
const themeClasses = computed(() => {
  const theme = settingsStore.settings.ui.theme
  let isDark = theme === 'dark'

  // system 主题需要检测系统偏好
  if (theme === 'system') {
    isDark = isSystemDark.value
  }

  return {
    container: isDark
      ? 'min-h-screen bg-gray-50 dark:bg-dark-900 text-gray-900 dark:text-dark-100'
      : 'min-h-screen bg-gray-50 text-gray-900'
  }
})

// Provide to child components
provide('isDesktop', isDesktop)
provide('platformInfo', platformInfo)
provide('isLandscape', isLandscape)
provide('orientation', orientation)
provide('safeArea', safeArea)
provide('keyboardInfo', keyboardInfo)

// 移动端容器样式：应用安全区域
const mobileContainerStyle = computed(() => {
  if (!platformInfo.value.isMobile) return {}

  const top = safeArea.value.top || 0
  const bottom = safeArea.value.bottom || 0
  const keyboardHeight = keyboardInfo.value.keyboardHeight || 0

  return {
    paddingTop: `${top}px`,
    paddingBottom: `${bottom + keyboardHeight}px`,
  }
})
</script>
