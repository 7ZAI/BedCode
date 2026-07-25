<template>
  <div :class="themeClasses.container">
    <DesktopLayout />

    <!-- Global Toast Container -->
    <ToastContainer />

    <!-- File System Auth Dialog -->
    <FsAuthDialog />

    <!-- Exit Confirm Dialog -->
    <ExitConfirmModal
      v-model:visible="showExitConfirm"
      :sessions="runningSessions"
    />
  </div>
</template>

<script setup lang="ts">
/**
 * BedCode Desktop - Root Component
 */
import { ref, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import DesktopLayout from '@/components/DesktopLayout.vue'
import FsAuthDialog from '@/components/FsAuthDialog.vue'
import ExitConfirmModal from '@/components/ExitConfirmModal.vue'
import { useGlobalNotifications } from '@/composables/useGlobalNotifications'
import { ToastContainer } from '@/composables/useToast'
import { useTheme } from '@/composables/useTheme'
import { useFontSize } from '@/composables/useFontSize'
import { useKeyboardShortcuts } from '@/composables/useKeyboardShortcuts'

interface RunningSession {
  id: string
  name: string
  status: string
}

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

// 退出确认弹窗状态
const showExitConfirm = ref(false)
const runningSessions = ref<RunningSession[]>([])
let unlistenCloseRequested: UnlistenFn | null = null

onMounted(async () => {
  setupTheme()
  setupFontSize()
  startGlobalNotifications()

  // 监听窗口关闭请求事件（有运行中会话时后端发送）
  unlistenCloseRequested = await listen<RunningSession[]>('window-close-requested', (event) => {
    runningSessions.value = event.payload
    showExitConfirm.value = true
  })
})

onUnmounted(() => {
  cleanupTheme()
  stopGlobalNotifications()
  unlistenCloseRequested?.()
})
</script>
