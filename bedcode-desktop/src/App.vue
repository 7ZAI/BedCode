<template>
  <div :class="themeClasses.container">
    <DesktopLayout />

    <!-- Global Toast Container -->
    <ToastContainer />
  </div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import DesktopLayout from '@/components/DesktopLayout.vue'
import { useGlobalNotifications } from '@/composables/useGlobalNotifications'
import { ToastContainer } from '@/composables/useToast'
import { useTheme } from '@/composables/useTheme'
import { useFontSize } from '@/composables/useFontSize'
import { useKeyboardShortcuts } from '@/composables/useKeyboardShortcuts'

const router = useRouter()

// 主题与字体管理
const { themeClasses, setupTheme, cleanupTheme } = useTheme()
const { setupFontSize } = useFontSize()

// 全局通知监听
const { startListening: startGlobalNotifications, stopListening: stopGlobalNotifications } = useGlobalNotifications()

// 键盘快捷键
useKeyboardShortcuts([
  { key: ',', ctrl: true, handler: () => router.push('/settings') },
  { key: '1', ctrl: true, handler: () => router.push('/sessions') },
  { key: '2', ctrl: true, handler: () => router.push('/devices') },
])

onMounted(() => {
  setupTheme()
  setupFontSize()
  startGlobalNotifications()
})

onUnmounted(() => {
  cleanupTheme()
  stopGlobalNotifications()
})
</script>
