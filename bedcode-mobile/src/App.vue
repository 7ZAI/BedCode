<template>
  <div :class="themeClasses.container">
    <MobileLayout />

    <!-- Global Toast Container -->
    <ToastContainer />
  </div>
</template>

<script setup lang="ts">
import { provide, onMounted, onUnmounted } from 'vue'
import MobileLayout from '@/modules/mobile/components/MobileLayout.vue'
import { usePlatform } from '@/modules/shared/composables/usePlatform'
import { useOrientation } from '@/modules/mobile/composables/useOrientation'
import { useEdgeToEdge } from '@/modules/mobile/composables/useEdgeToEdge'
import { ToastContainer } from '@/modules/shared/composables/useToast'
import { useTheme } from '@/modules/shared/composables/useTheme'
import { useFontSize } from '@/modules/shared/composables/useFontSize'

const { platformInfo } = usePlatform()
const { isLandscape, orientation } = useOrientation()
const { safeArea, keyboardInfo, isReady } = useEdgeToEdge()

// 主题与字体管理
const { themeClasses, setupTheme, cleanupTheme } = useTheme()
const { setupFontSize } = useFontSize()

onMounted(() => {
  setupTheme()
  setupFontSize()
})

onUnmounted(() => {
  cleanupTheme()
})

// Provide to child components
provide('platformInfo', platformInfo)
provide('isLandscape', isLandscape)
provide('orientation', orientation)
provide('safeArea', safeArea)
provide('keyboardInfo', keyboardInfo)
provide('safeAreaReady', isReady)
</script>
