<template>
  <div :class="themeClasses.container">
    <MobileLayout />

    <!-- Global Toast Container（vue-sonner，richColors 区分等级） -->
    <Toaster
      :theme="toasterTheme"
      position="top-center"
      rich-colors
      :mobile-offset="{ top: safeArea.top, bottom: safeArea.bottom }"
    />

    <!-- Plugin Dialog Host -->
    <PluginDialogHost />

    <!-- 文件系统授权弹窗（插件目录授权，全局挂载） -->
    <FsAuthDialog />
  </div>
</template>

<script setup lang="ts">
import { provide, computed, onMounted, onUnmounted } from 'vue'
import { Toaster } from 'vue-sonner'
import MobileLayout from '@/components/MobileLayout.vue'
import { usePlatform } from '@/composables/usePlatform'
import { useOrientation } from '@/composables/useOrientation'
import { useEdgeToEdge } from '@/composables/useEdgeToEdge'
import PluginDialogHost from '@/plugin/components/PluginDialogHost.vue'
import FsAuthDialog from '@/components/FsAuthDialog.vue'
import { useTheme } from '@/composables/useTheme'
import { useFontSize } from '@/composables/useFontSize'
import { useSettingsStore } from '@/stores/settings'
// mDNS 广播暂时禁用：移动端目前不需要被发现，避免扫描到自身
// import { useMdnsAdvertiser } from '@/composables/useMdnsAdvertiser'

const { platformInfo } = usePlatform()
const { isLandscape, orientation } = useOrientation()
const { safeArea, keyboardInfo, isReady } = useEdgeToEdge()

// 主题与字体管理
const { themeClasses, setupTheme, cleanupTheme } = useTheme()
const { setupFontSize } = useFontSize()

// Toaster 主题跟随应用设置（'system' 时由 sonner 自身监听系统偏好）
const settingsStore = useSettingsStore()
const toasterTheme = computed(() => settingsStore.settings.ui.theme as 'light' | 'dark' | 'system')
// const { startAdvertise, stopAdvertise } = useMdnsAdvertiser()

onMounted(async () => {
  setupTheme()
  setupFontSize()
  // mDNS 广播暂时禁用
  // try {
  //   const deviceName = `BedCode-Mobile-${Math.random().toString(36).slice(2, 6)}`
  //   await startAdvertise(0, deviceName)
  // } catch (e) {
  //   console.warn('[App] mDNS advertise failed:', e)
  // }
})

onUnmounted(async () => {
  cleanupTheme()
  // await stopAdvertise()
})

// Provide to child components
provide('platformInfo', platformInfo)
provide('isLandscape', isLandscape)
provide('orientation', orientation)
provide('safeArea', safeArea)
provide('keyboardInfo', keyboardInfo)
provide('safeAreaReady', isReady)
</script>
