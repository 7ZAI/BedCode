<template>
  <div class="h-screen flex flex-col bg-slate-100 dark:bg-dark-900">
    <!-- Header with title and window controls -->
    <header class="bg-white dark:bg-dark-800 border-b border-slate-200 dark:border-dark-700 px-4 py-2 flex items-center justify-between h-10 shrink-0" data-tauri-drag-region>
      <div class="flex items-center gap-2 text-sm text-slate-600 dark:text-dark-300">
        <span class="font-medium">{{ sessionName }}</span>
      </div>
      <div class="flex items-center gap-1">
        <button @click="minimizeWindow" class="p-1.5 hover:bg-slate-100 dark:bg-dark-700 rounded transition-colors" :title="t('desktop.terminal.minimize')">
          <svg class="w-4 h-4 text-slate-600 dark:text-dark-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 12H4" />
          </svg>
        </button>
        <button @click="toggleMaximize" class="p-1.5 hover:bg-slate-100 dark:bg-dark-700 rounded transition-colors" :title="t('desktop.terminal.maximize')">
          <svg class="w-4 h-4 text-slate-600 dark:text-dark-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path v-if="!isMaximized" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h4" />
            <path v-else stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 9V4.5M9 9H4.5M9 9L3.75 3.75M9 15v4.5M9 15H4.5M9 15l-5 5m5-5l5-5m-5 5v-4.5m0 4.5h4.5" />
          </svg>
        </button>
        <button @click="closeWindow" class="p-1.5 hover:bg-red-600 rounded transition-colors" :title="t('desktop.terminal.close')">
          <svg class="w-4 h-4 text-slate-600 dark:text-dark-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
    </header>

    <!-- Loading State -->
    <div v-if="isLoading" class="flex-1 flex items-center justify-center">
      <div class="text-center">
        <svg class="animate-spin h-8 w-8 text-primary-500 mx-auto mb-3" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        <p class="text-slate-500 dark:text-dark-400 text-sm">{{ t('desktop.terminal.loadingSession') }}</p>
      </div>
    </div>

    <!-- Terminal Preview Component -->
    <TerminalPreview v-else :session="session" :show-input="true" />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'
import { getCurrentWindow, PhysicalPosition } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import TerminalPreview from '@/modules/desktop/components/TerminalPreview.vue'
import type { SessionInfo } from '@/modules/shared/composables/useTauri'

const { t } = useI18n()
const appWindow = getCurrentWindow()

const SNAP_THRESHOLD = 15  // 贴靠阈值（像素）

const route = useRoute()
const sessionId = ref(route.params.id as string)
const sessionName = ref('')
const session = ref<SessionInfo | null>(null)
const isMaximized = ref(false)
const isLoading = ref(true)
const isSnapped = ref(false)  // 是否已贴靠
const snapDirection = ref<'left' | 'right' | null>(null)  // 贴靠方向

// 记录主窗口上一次的位置
let lastMainWindowPos = { x: 0, y: 0, width: 0, height: 0 }
// 记录本窗口上一次的位置
let lastTerminalWindowPos = { x: 0, y: 0 }

let unlistenMainMoved: UnlistenFn | null = null
let unlistenMainResized: UnlistenFn | null = null
let unlistenSnapped: UnlistenFn | null = null

async function loadSessionInfo() {
  isLoading.value = true
  try {
    const result = await invoke<SessionInfo>('get_session', { sessionId: sessionId.value })
    session.value = result
    sessionName.value = result.name

    // 加载完成后初始化位置
    await initWindowPosition()
  } catch (e) {
    console.error('[TerminalWindow] Failed to load session info:', e)
    sessionName.value = t('desktop.terminal.defaultName')
  } finally {
    isLoading.value = false
  }
}

/**
 * 初始化窗口位置和贴靠检测
 */
async function initWindowPosition() {
  const win = appWindow

  // 获取本窗口当前位置
  const pos = await win.outerPosition()
  lastTerminalWindowPos = { x: pos.x, y: pos.y }

  // 监听主窗口移动
  unlistenMainMoved = await listen<{ x: number; y: number; width: number; height: number }>(
    'main-window-moved',
    handleMainWindowMoved
  )

  // 监听主窗口大小变化
  unlistenMainResized = await listen<{ width: number; height: number }>(
    'main-window-resized',
    handleMainWindowResized
  )

  // 监听贴靠状态变化（从主窗口发出）
  unlistenSnapped = await listen<{ sessionId: string; direction: 'left' | 'right' }>(
    'terminal-window-snapped',
    (event) => {
      if (event.payload.sessionId === sessionId.value) {
        isSnapped.value = true
        snapDirection.value = event.payload.direction
      }
    }
  )
}

/**
 * 处理主窗口移动 - 贴靠时同步移动
 */
async function handleMainWindowMoved(event: { payload: { x: number; y: number; width: number; height: number } }) {
  const mainPos = event.payload

  // 更新本窗口记录的位置
  lastMainWindowPos = mainPos

  if (!isSnapped.value) {
    // 未贴靠时，检测是否需要贴靠
    await checkAndSnap(mainPos)
    return
  }

  // 已贴靠：跟随主窗口移动
  const win = appWindow
  const terminalPos = await win.outerPosition()
  const terminalSize = await win.outerSize()

  let newX = terminalPos.x

  if (snapDirection.value === 'right') {
    // 贴靠右侧
    newX = mainPos.x + mainPos.width
  } else if (snapDirection.value === 'left') {
    // 贴靠左侧
    newX = mainPos.x - terminalSize.width
  }

  // 计算移动差值
  const dx = newX - terminalPos.x

  // 仅当有实际移动时才更新
  if (dx !== 0) {
    await win.setPosition(new PhysicalPosition(newX, terminalPos.y))
  }

  lastTerminalWindowPos = { x: newX, y: terminalPos.y }
}

/**
 * 处理主窗口大小变化 - 调整贴靠位置
 */
async function handleMainWindowResized(event: { payload: { width: number; height: number } }) {
  if (!isSnapped.value) return

  const mainSize = event.payload
  const win = appWindow
  const terminalPos = await win.outerPosition()
  const terminalSize = await win.outerSize()

  let newX = terminalPos.x

  if (snapDirection.value === 'right') {
    newX = mainSize.width + lastMainWindowPos.width - terminalSize.width + lastMainWindowPos.x
  } else if (snapDirection.value === 'left') {
    newX = lastMainWindowPos.x - terminalSize.width
  }

  if (newX !== terminalPos.x) {
    await win.setPosition(new PhysicalPosition(newX, terminalPos.y))
  }
}

/**
 * 检测并执行贴靠
 */
async function checkAndSnap(mainPos: { x: number; y: number; width: number; height: number }) {
  const win = appWindow
  const terminalPos = await win.outerPosition()
  const terminalSize = await win.outerSize()

  // 检测右侧贴靠
  const rightDistance = Math.abs((mainPos.x + mainPos.width) - terminalPos.x)
  if (rightDistance < SNAP_THRESHOLD) {
    isSnapped.value = true
    snapDirection.value = 'right'
    await win.setPosition(new PhysicalPosition(mainPos.x + mainPos.width, terminalPos.y))
    // 通知主窗口
    return
  }

  // 检测左侧贴靠
  const leftDistance = Math.abs(mainPos.x - (terminalPos.x + terminalSize.width))
  if (leftDistance < SNAP_THRESHOLD) {
    isSnapped.value = true
    snapDirection.value = 'left'
    await win.setPosition(new PhysicalPosition(mainPos.x - terminalSize.width, terminalPos.y))
    return
  }

  // 未贴靠
  isSnapped.value = false
  snapDirection.value = null
}

async function minimizeWindow() {
  const win = appWindow
  await win.minimize()
}

async function toggleMaximize() {
  const win = appWindow
  const maximized = await win.isMaximized()
  if (maximized) {
    await win.unmaximize()
    isMaximized.value = false
  } else {
    await win.maximize()
    isMaximized.value = true
  }
}

async function closeWindow() {
  try {
    await appWindow.close()
  } catch (e) {
    console.error('[TerminalWindowView] Close error:', e)
  }
}

onMounted(() => {
  loadSessionInfo()
})

onUnmounted(() => {
  if (unlistenMainMoved) unlistenMainMoved()
  if (unlistenMainResized) unlistenMainResized()
  if (unlistenSnapped) unlistenSnapped()
})
</script>

<style scoped>
:deep(.xterm) {
  height: 100%;
  padding: 8px;
}

:deep(.xterm-viewport) {
  border-radius: 0;
  overflow-y: auto !important;
  overflow-x: hidden;
}

:deep(.xterm-viewport)::-webkit-scrollbar {
  width: 6px;
}

:deep(.xterm-viewport)::-webkit-scrollbar-track {
  background: transparent;
  margin: 8px 2px;
  border-radius: 3px;
}

:deep(.xterm-viewport)::-webkit-scrollbar-thumb {
  background: rgba(128, 128, 128, 0.25);
  border-radius: 3px;
  transition: background 0.2s ease;
}

:deep(.xterm-viewport)::-webkit-scrollbar-thumb:hover {
  background: rgba(128, 128, 128, 0.5);
}

:deep(.xterm-viewport:hover)::-webkit-scrollbar-thumb {
  background: rgba(128, 128, 128, 0.35);
}

.dark :deep(.xterm-viewport)::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.12);
  border-radius: 3px;
}

.dark :deep(.xterm-viewport)::-webkit-scrollbar-thumb:hover {
  background: rgba(255, 255, 255, 0.3);
}

.dark :deep(.xterm-viewport:hover)::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.2);
}
</style>