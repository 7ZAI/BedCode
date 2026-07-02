<template>
  <div :class="themeClasses.container">
    <MobileLayout />

    <!-- Global Toast Container -->
    <ToastContainer />
  </div>
</template>

<script setup lang="ts">
import { provide, onMounted, onUnmounted } from 'vue'
import MobileLayout from '@/components/MobileLayout.vue'
import { usePlatform } from '@/composables/usePlatform'
import { useOrientation } from '@/composables/useOrientation'
import { useEdgeToEdge } from '@/composables/useEdgeToEdge'
import { ToastContainer } from '@/composables/useToast'
import { useTheme } from '@/composables/useTheme'
import { useFontSize } from '@/composables/useFontSize'
import { useMdnsAdvertiser } from '@/composables/useMdnsAdvertiser'

const { platformInfo } = usePlatform()
const { isLandscape, orientation } = useOrientation()
const { safeArea, keyboardInfo, isReady } = useEdgeToEdge()

// 主题与字体管理
const { themeClasses, setupTheme, cleanupTheme } = useTheme()
const { setupFontSize } = useFontSize()
const { startAdvertise, stopAdvertise } = useMdnsAdvertiser()

onMounted(async () => {
  setupTheme()
  setupFontSize()
  // 自动广播 mDNS 服务，允许桌面端发现移动端
  try {
    const deviceName = `BedCode-Mobile-${Math.random().toString(36).slice(2, 6)}`
    await startAdvertise(0, deviceName)
  } catch (e) {
    console.warn('[App] mDNS advertise failed:', e)
  }
})

onUnmounted(async () => {
  cleanupTheme()
  await stopAdvertise()
})

// Provide to child components
provide('platformInfo', platformInfo)
provide('isLandscape', isLandscape)
provide('orientation', orientation)
provide('safeArea', safeArea)
provide('keyboardInfo', keyboardInfo)
provide('safeAreaReady', isReady)
</script>
