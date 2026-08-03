<template>
  <div
    class="h-screen relative overflow-hidden flex flex-col bg-slate-100 dark:bg-dark-900"
    :class="isShown ? (revealDone ? 'opacity-100' : 'animate-fade-slide-up') : 'opacity-0'"
    @animationend="onRevealEnd"
  >
    <!-- Header with title, settings, actions, and window controls -->
    <header class="bg-white dark:bg-dark-800 border-b border-slate-200 dark:border-dark-700 px-3 h-10 shrink-0 flex items-center justify-between" data-tauri-drag-region>
      <div class="flex items-center gap-2 text-sm text-slate-600 dark:text-dark-300" data-tauri-drag-region>
        <div
          :class="[
            'w-2 h-2 rounded-full shrink-0',
            statusColor
          ]"
        ></div>
        <span class="font-medium truncate">{{ sessionName }}</span>
      </div>

      <div class="flex items-center gap-1.5" data-tauri-drag-region>
        <!-- Settings Button -->
        <button
          @click.stop="isSettingsOpen = !isSettingsOpen"
          class="p-1.5 hover:bg-slate-100 dark:hover:bg-dark-700 rounded transition-colors"
          :class="{ 'bg-slate-200 dark:bg-dark-600': isSettingsOpen }"
          :title="t('desktop.terminal.settings')"
          @mousedown.stop
        >
          <svg class="w-4 h-4 text-slate-500 dark:text-dark-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
        </button>

        <!-- Clear Button -->
        <button @click="terminalPreviewRef?.clearTerminal()" class="p-1.5 hover:bg-slate-100 dark:hover:bg-dark-700 rounded transition-colors" :title="t('desktop.terminal.clearScreen')">
          <svg class="w-4 h-4 text-slate-500 dark:text-dark-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </button>

        <!-- Refresh Format Button -->
        <button @click="terminalPreviewRef?.refreshTerminal()" class="p-1.5 hover:bg-slate-100 dark:hover:bg-dark-700 rounded transition-colors" :title="t('desktop.terminal.refreshFormat')">
          <svg class="w-4 h-4 text-slate-500 dark:text-dark-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
        </button>

        <!-- Plugin Toolbar Extension -->
        <PluginTerminalToolbar />

        <!-- Plugin TitleBar Extension -->
        <PluginTitleBarItems />

        <!-- Divider -->
        <div class="w-px h-4 bg-slate-200 dark:bg-dark-600 mx-0.5"></div>

        <!-- Window Controls -->
        <button @click="minimizeWindow" class="p-1.5 hover:bg-slate-100 dark:hover:bg-dark-700 rounded transition-colors" :title="t('desktop.terminal.minimize')">
          <svg class="w-4 h-4 text-slate-600 dark:text-dark-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 12H4" />
          </svg>
        </button>
        <button @click="toggleMaximize" class="p-1.5 hover:bg-slate-100 dark:hover:bg-dark-700 rounded transition-colors" :title="t('desktop.terminal.maximize')">
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
    <TerminalPreview v-else ref="terminalPreviewRef" :session="session" :show-input="true" :show-header="false" />

    <!-- 设置面板遮罩：点击关闭 -->
    <transition name="settings-backdrop">
      <div
        v-if="isSettingsOpen"
        class="absolute inset-0 top-10 z-20 bg-black/25"
        @click="isSettingsOpen = false"
      ></div>
    </transition>

    <!-- 设置面板：从右侧滑出 -->
    <transition name="settings-panel">
      <aside
        v-if="isSettingsOpen"
        class="absolute top-10 right-0 bottom-0 z-30 w-64 flex flex-col bg-white dark:bg-dark-800 border-l border-slate-200 dark:border-dark-700 shadow-xl"
      >
        <div class="h-10 shrink-0 px-4 flex items-center justify-between border-b border-slate-200 dark:border-dark-700">
          <span class="text-sm font-medium text-slate-700 dark:text-white">{{ t('desktop.terminal.settings') }}</span>
          <button
            class="p-1 hover:bg-slate-100 dark:hover:bg-dark-700 rounded transition-colors"
            :title="t('desktop.terminal.close')"
            @click="isSettingsOpen = false"
          >
            <svg class="w-3.5 h-3.5 text-slate-500 dark:text-dark-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div class="flex-1 overflow-y-auto p-4 space-y-5">
          <!-- 终端主题 -->
          <div>
            <label class="block text-xs font-medium mb-1.5 text-slate-500 dark:text-dark-400">{{ t('desktop.terminal.theme') }}</label>
            <select
              v-model="settingsTheme"
              class="w-full bg-slate-100 dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1.5 text-sm text-slate-700 dark:text-white shadow-xs dark:shadow-none"
              @click.stop
              @mousedown.stop
            >
              <option v-for="(name, key) in themeOptions" :key="key" :value="key">{{ name }}</option>
            </select>
          </div>

          <!-- 字体大小 -->
          <div>
            <label class="block text-xs font-medium mb-1.5 text-slate-500 dark:text-dark-400">{{ t('desktop.terminal.fontSize') }}</label>
            <select
              v-model="settingsFontSize"
              class="w-full bg-slate-100 dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1.5 text-sm text-slate-700 dark:text-white shadow-xs dark:shadow-none"
              @click.stop
              @mousedown.stop
            >
              <option v-for="size in [12, 14, 16, 18, 20]" :key="size" :value="size">{{ size }}px</option>
            </select>
          </div>
        </div>
      </aside>
    </transition>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'
import { getCurrentWindow, PhysicalPosition } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event'
import TerminalPreview from '@/components/TerminalPreview.vue'
import PluginTerminalToolbar from '@/plugin/components/PluginTerminalToolbar.vue'
import PluginTitleBarItems from '@/plugin/components/PluginTitleBarItems.vue'
import type { SessionInfo } from '@/composables/useTauri'

const { t } = useI18n()
const appWindow = getCurrentWindow()

const SNAP_THRESHOLD = 15  // 贴靠阈值（像素）

const route = useRoute()
const sessionId = ref(route.params.id as string)
const sessionName = ref('')
const session = ref<SessionInfo | null>(null)
const isMaximized = ref(false)
const isLoading = ref(true)
const isShown = ref(false)  // 是否已允许显示（由主窗口在内容就绪后通知）
const revealDone = ref(false)  // 进入动画是否已结束（结束后移除残留 transform）
const isSnapped = ref(false)  // 是否已贴靠
const snapDirection = ref<'left' | 'right' | null>(null)  // 贴靠方向

// TerminalPreview 组件引用，访问暴露的 fontSize/terminalTheme 等
const terminalPreviewRef = ref<InstanceType<typeof TerminalPreview> | null>(null)

// 设置面板是否打开
const isSettingsOpen = ref(false)

// 设置面板绑定的主题/字体大小（读写 TerminalPreview 暴露的 ref，与终端实时同步）
const settingsTheme = computed({
  get: () => terminalPreviewRef.value?.terminalTheme ?? 'dracula',
  set: (value: string) => {
    if (terminalPreviewRef.value) terminalPreviewRef.value.terminalTheme = value
  },
})

const settingsFontSize = computed({
  get: () => terminalPreviewRef.value?.fontSize ?? 12,
  set: (value: number) => {
    if (terminalPreviewRef.value) terminalPreviewRef.value.fontSize = value
  },
})

const themeOptions = computed(() => terminalPreviewRef.value?.themeNames ?? {})

// 会话状态颜色（与 TerminalPreview 中的逻辑一致）
const statusColor = computed(() => {
  if (!session.value) return 'bg-slate-400 dark:bg-dark-500'
  switch (session.value.status) {
    case 'running': return 'bg-green-500'
    case 'waitingInput': return 'bg-yellow-500 animate-pulse'
    case 'error': return 'bg-red-500'
    case 'stopped': return 'bg-slate-400 dark:bg-dark-500'
    case 'starting': return 'bg-blue-500 animate-pulse'
    default: return 'bg-slate-400 dark:bg-dark-500'
  }
})

// 记录主窗口上一次的位置
let lastMainWindowPos = { x: 0, y: 0, width: 0, height: 0 }
// 记录本窗口上一次的位置
let lastTerminalWindowPos = { x: 0, y: 0 }

let unlistenMainMoved: UnlistenFn | null = null
let unlistenMainResized: UnlistenFn | null = null
let unlistenSnapped: UnlistenFn | null = null
let unlistenShow: UnlistenFn | null = null
let unlistenFocus: UnlistenFn | null = null

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
    // 通知主窗口内容已就绪，可显示窗口（避免加载闪屏）
    emit('terminal-ready', { sessionId: sessionId.value }).catch(() => {})
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

function handleKeydown(e: KeyboardEvent) {
  // Esc 关闭设置面板
  if (e.key === 'Escape' && isSettingsOpen.value) {
    isSettingsOpen.value = false
  }
}

/**
 * 窗口进入动画结束后，切换到无 transform 状态（opacity-100）。
 * 动画 fill-mode:both 会让 transform: translateY(0) 永久残留在根节点，
 * 使包裹 WebGL 画布的外层长期处于独立合成层，WebView2 合成器滚动时可能
 * 缓存旧帧导致重影；动画结束后移除 transform 消除该触发点。
 */
function onRevealEnd(e: AnimationEvent) {
  if (e.animationName === 'fade-slide-up' && isShown.value) {
    revealDone.value = true
  }
}

onMounted(async () => {
  // Esc 关闭设置面板
  window.addEventListener('keydown', handleKeydown)
  // 先注册显示事件监听，再加载会话，避免与主窗口的显示通知产生竞态
  unlistenShow = await listen<{ sessionId: string }>('terminal-show', (event) => {
    if (event.payload.sessionId === sessionId.value) {
      isShown.value = true
    }
  })

  // 兜底：窗口获得焦点时也触发显现动画
  unlistenFocus = await appWindow.onFocusChanged(({ payload: focused }) => {
    if (focused) {
      isShown.value = true
    }
  })

  loadSessionInfo()
})

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown)
  if (unlistenMainMoved) unlistenMainMoved()
  if (unlistenMainResized) unlistenMainResized()
  if (unlistenSnapped) unlistenSnapped()
  if (unlistenShow) unlistenShow()
  if (unlistenFocus) unlistenFocus()
})
</script>

<style scoped>
:deep(.xterm) {
  height: 100%;
}

/* 设置面板滑出过渡：will-change 提升为独立合成层，避免动画期间页面抖动 */
.settings-panel-enter-active,
.settings-panel-leave-active {
  transition: transform 0.25s ease;
  will-change: transform;
}

.settings-panel-enter-from,
.settings-panel-leave-to {
  transform: translateX(100%);
}

/* 设置面板遮罩淡入淡出 */
.settings-backdrop-enter-active,
.settings-backdrop-leave-active {
  transition: opacity 0.2s ease;
  will-change: opacity;
}

:deep(.xterm-viewport) {
  border-radius: 0;
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
