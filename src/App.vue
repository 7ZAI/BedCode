<template>
  <div :class="themeClasses.container">
    <DesktopLayout v-if="isDesktop" />
    <MobileLayout v-else />

    <!-- Global Toast Container -->
    <ToastContainer />
  </div>
</template>

<script setup lang="ts">
import { computed, provide, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import DesktopLayout from '@/modules/desktop/components/DesktopLayout.vue'
import MobileLayout from '@/modules/mobile/components/MobileLayout.vue'
import { usePlatform } from '@/modules/shared/composables/usePlatform'
import { useSettingsStore } from '@/modules/shared/stores/settings'
import { useGlobalNotifications } from '@/modules/shared/composables/useGlobalNotifications'
import { useOrientation } from '@/modules/mobile/composables/useOrientation'
import { useEdgeToEdge } from '@/modules/mobile/composables/useEdgeToEdge'
import { ToastContainer } from '@/modules/shared/composables/useToast'
import { useTheme } from '@/modules/shared/composables/useTheme'
import { useFontSize } from '@/modules/shared/composables/useFontSize'
import { useKeyboardShortcuts } from '@/modules/shared/composables/useKeyboardShortcuts'

const router = useRouter()
const { platformInfo } = usePlatform()
const { isLandscape, orientation } = useOrientation()
const { safeArea, keyboardInfo, isReady } = useEdgeToEdge()

const isDesktop = computed(() => platformInfo.value.isDesktop)

// 主题与字体管理
const { themeClasses, setupTheme, cleanupTheme } = useTheme()
const { setupFontSize } = useFontSize()

// 全局通知监听（桌面端）
const { startListening: startGlobalNotifications, stopListening: stopGlobalNotifications } = useGlobalNotifications()

// 桌面端键盘快捷键
useKeyboardShortcuts([
  { key: ',', ctrl: true, handler: () => router.push('/settings') },
  { key: '1', ctrl: true, handler: () => router.push('/sessions') },
  { key: '2', ctrl: true, handler: () => router.push('/devices') },
])

onMounted(() => {
  setupTheme()
  setupFontSize()

  if (isDesktop.value) {
    startGlobalNotifications()
  }
})

onUnmounted(() => {
  cleanupTheme()
  stopGlobalNotifications()
})

// Provide to child components
provide('isDesktop', isDesktop)
provide('platformInfo', platformInfo)
provide('isLandscape', isLandscape)
provide('orientation', orientation)
provide('safeArea', safeArea)
provide('keyboardInfo', keyboardInfo)
provide('safeAreaReady', isReady)
</script>
