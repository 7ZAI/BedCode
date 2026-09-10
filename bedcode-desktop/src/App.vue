<template>
  <!-- 启动画面：z-[100] 覆盖一切，初始化完成后淡出 -->
  <SplashLoading :visible="showSplash" />

  <div :class="themeClasses.container">
    <DesktopLayout />

    <!-- Global Toast Container（vue-sonner，richColors 区分等级） -->
    <Toaster
      :theme="toasterTheme"
      position="top-center"
      rich-colors
      expand
      :visible-toasts="6"
      :toast-options="toastOptions"
    />

    <!-- File System Auth Dialog -->
    <FsAuthDialog />

    <!-- Exit Confirm Dialog -->
    <ExitConfirmModal v-model:visible="showExitConfirm" :sessions="runningSessions" />
  </div>

  <!--
    无边框窗口的边缘 resize 热区：decorations:false 后 Linux 失去 GTK 原生边框拖拽，
    Tauri 用 data-tauri-resize-handle 属性接管边缘 resize，无需自定义 JS。
    4 条边各 6px，fixed 定位贴视口边缘，透明不可见。
    仅在桌面端渲染：移动端窗口不可 resize，无需这些热区。
  -->
  <template v-if="isDesktop">
    <div data-tauri-resize-handle class="resize-handle resize-top"></div>
    <div data-tauri-resize-handle class="resize-handle resize-bottom"></div>
    <div data-tauri-resize-handle class="resize-handle resize-left"></div>
    <div data-tauri-resize-handle class="resize-handle resize-right"></div>
  </template>
</template>

<script setup lang="ts">
/**
 * BedCode Desktop - Root Component
 */
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { Toaster, type ToasterProps } from 'vue-sonner'
import DesktopLayout from '@/components/DesktopLayout.vue'
import SplashLoading from '@/components/SplashLoading.vue'
import FsAuthDialog from '@/components/FsAuthDialog.vue'
import ExitConfirmModal from '@/components/ExitConfirmModal.vue'
import { useGlobalNotifications } from '@/composables/useGlobalNotifications'
import { useTheme } from '@/composables/useTheme'
import { useFontSize } from '@/composables/useFontSize'
import { useKeyboardShortcuts } from '@/composables/useKeyboardShortcuts'
import { useSettingsStore } from '@/stores/settings'
import { usePlatform } from '@/composables/usePlatform'

interface RunningSession {
  id: string
  name: string
  status: string
}

const router = useRouter()
const settingsStore = useSettingsStore()

// 平台信息：isDesktop 用于条件渲染桌面端专属 UI（如窗口边缘 resize 热区）
const { platformInfo } = usePlatform()
const isDesktop = computed(() => platformInfo.value.isDesktop)

// 主题与字体管理
const { themeClasses, setupTheme, cleanupTheme } = useTheme()
const { setupFontSize } = useFontSize()

// Toaster 主题跟随应用设置（'system' 时由 sonner 自身监听系统偏好）
const toasterTheme = computed(() => settingsStore.settings.ui.theme as ToasterProps['theme'])

// Toast 外观：等级配色由 style.css 覆盖 richColors 变量（成功绿/警告黄/错误红/info 主色），
// 此处不覆盖背景边框文字，仅保留圆角阴影与关闭按钮（透明底融入彩色背景）
const toastOptions: ToasterProps['toastOptions'] = {
  classes: {
    toast: '!rounded-[10px] !shadow-lg',
    title: '!text-[calc(13px*var(--ui-scale))] !font-medium',
    description: '!text-[var(--text-secondary)]',
    actionButton: '!bg-[var(--color-primary)]',
    cancelButton: '!bg-[var(--bg-hover)]',
    closeButton:
      '!bg-transparent !border-transparent !text-[var(--text-secondary)] hover:!text-[var(--text-primary)]',
  },
}

// 全局通知监听
const { startListening: startGlobalNotifications, stopListening: stopGlobalNotifications } =
  useGlobalNotifications()

// 键盘快捷键
useKeyboardShortcuts([
  { key: ',', ctrl: true, handler: () => router.push('/settings') },
  { key: '1', ctrl: true, handler: () => router.push('/sessions') },
  { key: '2', ctrl: true, handler: () => router.push('/devices') },
])

// 启动画面：保证最低展示时长避免闪烁，初始化完成后淡出
const showSplash = ref(true)
let splashTimer: ReturnType<typeof setTimeout> | null = null

// 退出确认弹窗状态
const showExitConfirm = ref(false)
const runningSessions = ref<RunningSession[]>([])
let unlistenCloseRequested: UnlistenFn | null = null

onMounted(async () => {
  setupTheme()
  setupFontSize()
  startGlobalNotifications()

  // 首帧渲染完成即开始计时，最低展示 900ms 后淡出启动画面
  splashTimer = setTimeout(() => {
    showSplash.value = false
    splashTimer = null
  }, 900)

  // 监听窗口关闭请求事件（有运行中会话时后端发送）
  unlistenCloseRequested = await listen<RunningSession[]>('window-close-requested', (event) => {
    runningSessions.value = event.payload
    showExitConfirm.value = true
  })
})

onUnmounted(() => {
  if (splashTimer) clearTimeout(splashTimer)
  cleanupTheme()
  stopGlobalNotifications()
  unlistenCloseRequested?.()
})
</script>
