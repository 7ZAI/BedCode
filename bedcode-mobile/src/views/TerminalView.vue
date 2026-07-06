<template>
  <div
    class="terminal-view"
    :style="terminalViewStyle"
  >
    <!-- Loading Overlay -->
    <transition name="loading-fade">
      <div v-if="!isTerminalReady" class="loading-overlay">
        <div class="loading-spinner"></div>
        <p class="loading-text">{{ t('mobile.terminal.preparing') }}</p>
      </div>
    </transition>

    <!-- Header - 固定位置，不随键盘移动 -->
    <TerminalHeader
      :session-name="sessionName"
      :is-selection-mode="isSelectionMode"
      :visible-items="visibleToolbarItems"
      :all-items="ALL_TOOLBAR_ITEMS"
      :show-sidebar="showSidebar"
      @back="handleBack"
      @action="handleToolbarAction"
    />

    <!-- 裁剪容器：限制上移区域不突破 Header 底部 -->
    <div class="movable-clip">
      <!-- 可移动区域：终端内容 + 输入栏，键盘弹出时整体上移 -->
      <div ref="movableAreaRef" class="movable-area" :style="movableAreaStyle">
        <!-- Main Content: Terminal + Sidebar overlay -->
        <div class="main-content">
          <div class="terminal-output-area">
            <div
              ref="scrollContainer"
              class="terminal-scroll-container"
              :class="{ 'selection-mode': isSelectionMode }"
            >
              <div
                ref="xtermContainer"
                class="xterm-container"
                :style="xtermContainerStyle"
              ></div>
              <div class="scrollbar-track">
                <div
                  class="scrollbar-thumb"
                  :class="{ visible: scrollbarVisible }"
                  :style="scrollbarThumbStyle"
                ></div>
              </div>
              <transition name="selection-bar">
                <div v-if="isSelectionMode && hasSelection && selectionTouchEnded" class="selection-action-bar" :style="selectionBarStyle">
                  <button class="selection-action-btn" @click="copySelection">
                    {{ t('common.button.copy') }}
                  </button>
                  <button class="selection-action-btn" @click="selectAllText">
                    {{ t('mobile.terminal.selectAll') }}
                  </button>
                  <button class="selection-action-btn cancel" @click="exitSelectionMode">
                    {{ t('common.button.cancel') }}
                  </button>
                </div>
              </transition>
            </div>
          </div>

          <FileSidebar
            class="sidebar-overlay"
            :class="{ 'sidebar-hidden': !showSidebar }"
            :session-id="sessionId"
            @long-press="handleLongPress"
          />

          <div v-if="showSidebar" class="sidebar-backdrop" @click="showSidebar = false"></div>
        </div>

        <!-- Input Bar -->
        <TerminalInputBar
          :disabled="!isSessionActive"
          :is-connected="isConnected"
          :placeholder="inputPlaceholder"
          :is-landscape="isLandscape"
          @submit="handleInputSubmit"
          @execute="handleInputExecute"
          @special-key="handleSpecialKey"
          @shortcuts-panel-toggle="handleShortcutsPanelToggle"
        />
      </div>
    </div>

    <!-- Settings Modal -->
    <TerminalSettingsModal
      :visible="showSettings"
      :font-size="terminalSettings.fontSize"
      :theme="terminalSettings.theme"
      :is-theme-user-set="terminalSettings.isThemeUserSet"
      :quick-bar-count="assistStore.settings.quickBarCount"
      :toolbar-items="assistStore.settings.headerToolbarItems || ['folder']"
      :all-toolbar-items="ALL_TOOLBAR_ITEMS"
      :safe-area-style="settingsModalStyle"
      @confirm="handleSettingsConfirm"
      @cancel="showSettings = false"
    />

    <!-- Clear Confirm Modal -->
    <TerminalConfirmModal
      :visible="showClearConfirm"
      :message="t('mobile.terminal.clearScreen') + '?'"
      :safe-area-style="confirmModalStyle"
      @confirm="clearTerminal"
      @cancel="showClearConfirm = false"
    />
  </div>

  <!-- Task Picker -->
  <TaskPickerModal
    :visible="showTaskPicker"
    :tasks="presetTasks"
    :session-id="sessionId"
    @send="onTaskSend"
    @execute="onTaskExecute"
    @close="showTaskPicker = false"
  />

  <!-- Shortcut Config -->
  <ShortcutConfigModal :visible="showShortcutConfig" @close="showShortcutConfig = false" />
</template>

<script setup lang="ts">
/**
 * 终端视图 - 显示 PTY 输出和输入栏
 * 支持多会话切换和 ANSI 渲染
 */
defineOptions({ name: 'TerminalView' })

import { ref, computed, inject, type Ref, onMounted, onUnmounted, watch, nextTick } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import '@xterm/xterm/css/xterm.css'
import '@/styles/terminal.css'
import { useMobileConnection } from '@/composables/useMobileConnection'
import { isMockSession, useMockTerminal } from '@/composables/useMockTerminal'
import { useTerminalBuffer } from '@/composables/useTerminalBuffer'
import { wsResizeTerminal } from '@/composables/useMobileCommands'
import { httpSendSessionInput } from '@/composables/useHttpApi'
import { useOrientation } from '@/composables/useOrientation'
import { useTheme } from '@/composables/useTheme'
import { useSettingsStore } from '@/stores/settings'
import { useInputAssistantStore } from '@/stores/inputAssistant'
import { useTerminalScroll } from '@/composables/useTerminalScroll'
import TerminalHeader from '@/components/TerminalHeader.vue'
import TerminalSettingsModal from '@/components/TerminalSettingsModal.vue'
import type { ToolbarItemConfig, TerminalSettings } from '@/components/TerminalSettingsModal.vue'
import TerminalConfirmModal from '@/components/TerminalConfirmModal.vue'
import TerminalInputBar from '@/components/TerminalInputBar.vue'
import FileSidebar from '@/components/FileSidebar.vue'
import TaskPickerModal from '@/components/TaskPickerModal.vue'
import ShortcutConfigModal from '@/components/ShortcutConfigModal.vue'
import { useToast } from '@/composables/useToast'
import { writeClipboardText } from '@/utils/clipboard'
import { usePresetTasks, executeTask, sendTask } from '@/composables/usePresetTasks'
import { TERMINAL_THEMES } from '@/config/terminalThemes'
import type { PresetTask } from '@/composables/model'

// ==================== Props & Route ====================

const router = useRouter()
const route = useRoute()
const { t } = useI18n()
const connection = useMobileConnection()
const mockTerminal = useMockTerminal()
const toast = useToast()
const { isLandscape } = useOrientation()
const { isSystemDark } = useTheme()
const { writeBufferHistoryToTerminal, registerRealtimeHandler, unregisterRealtimeHandler, subscribeSession, unsubscribeSession, handleDisconnect, handleSessionStopped } = useTerminalBuffer()
const settingsStore = useSettingsStore()
const assistStore = useInputAssistantStore()
const sessionId = computed(() => route.params.id as string)

// 安全区域从 App.vue inject
const safeArea = inject<Ref<{ top: number; bottom: number }>>('safeArea')!

// ==================== Task Picker ====================

const { tasks: presetTasks } = usePresetTasks()
const showTaskPicker = ref(false)

// ==================== Header Toolbar Config ====================

const ALL_TOOLBAR_ITEMS: ToolbarItemConfig[] = [
  { key: 'task', label: 'mobile.terminal.toolbarTask', icon: 'task' },
  { key: 'shortcut', label: 'mobile.terminal.toolbarShortcut', icon: 'shortcut' },
  { key: 'clear', label: 'mobile.terminal.toolbarClear', icon: 'clear' },
  { key: 'refresh', label: 'mobile.terminal.toolbarRefresh', icon: 'refresh' },
  { key: 'settings', label: 'mobile.terminal.toolbarSettings', icon: 'settings' },
  { key: 'folder', label: 'mobile.terminal.toolbarFolder', icon: 'folder' },
]

const visibleToolbarItems = computed(() => {
  const items = assistStore.settings.headerToolbarItems || ['folder']
  return ALL_TOOLBAR_ITEMS.filter(item => items.includes(item.key))
})

// ==================== State ====================

const xtermContainer = ref<HTMLDivElement | null>(null)
const scrollContainer = ref<HTMLDivElement | null>(null)
const movableAreaRef = ref<HTMLDivElement | null>(null)
const isTerminalReady = ref(false)
const terminalRef = ref<Terminal | null>(null)
const fitAddonRef = ref<FitAddon | null>(null)
const resizeObserverRef = ref<ResizeObserver | null>(null)

const showSettings = ref(false)
const showClearConfirm = ref(false)
const showSidebar = ref(false)
const showShortcutConfig = ref(false)

// 终端主题设置：theme 存储当前生效的主题名，isThemeUserSet 标记是否由用户手动指定
const terminalSettings = ref({
  fontSize: assistStore.settings.terminalFontSize,
  theme: assistStore.settings.terminalTheme
    ?? (settingsStore.settings.ui.theme === 'system'
      ? (isSystemDark.value ? 'dark' : 'light')
      : settingsStore.settings.ui.theme) as string,
  isThemeUserSet: assistStore.settings.isTerminalThemeUserSet,
})

// 弹窗安全区域样式
const settingsModalStyle = computed(() => ({
  paddingTop: `${safeArea.value.top}px`,
  paddingBottom: `${safeArea.value.bottom}px`,
}))

const confirmModalStyle = computed(() => ({
  paddingTop: `${safeArea.value.top}px`,
  paddingBottom: `${safeArea.value.bottom}px`,
}))

// ==================== Terminal Scroll ====================

const {
  currentLine,
  isSelectionMode,
  hasSelection,
  selectionTouchEnded,
  scrollbarVisible,
  scrollbarThumbStyle,
  xtermContainerStyle,
  isUserScrolling,
  cellHeight,
  scrollToBottom,
  fitTerminal,
  setupViewportScroll,
  exitSelectionMode,
  copySelection,
  selectAllText,
  handleShortcutsPanelToggle,
  applySettings: applyScrollSettings,
  dispose: disposeScroll,
  longPressTriggerPos,
  selectionViewportRange,
} = useTerminalScroll(terminalRef, scrollContainer)

// ==================== Computed ====================

const isConnected = computed(() =>
  connection.connectionStatus.value === 'connected' ||
  connection.connectionStatus.value === 'paired'
)

const session = computed(() => {
  if (isMockSession(sessionId.value)) {
    return { id: sessionId.value, name: t('mobile.session.mockName'), status: 'running', is_active: true }
  }
  return connection.activeSessions.value.find(s => s.id === sessionId.value)
})

const sessionName = computed(() => session.value?.name || sessionId.value || t('desktop.terminal.title'))

const isSessionActive = computed(() => isMockSession(sessionId.value) || (session.value?.status || 'stopped') === 'running')

const inputPlaceholder = computed(() => {
  if (isMockSession(sessionId.value)) return t('mobile.session.mockName')
  if (!isConnected.value) return t('mobile.input.disconnected') + '...'
  if (!isSessionActive.value) return t('mobile.connection.connectFailed')
  return t('mobile.input.commandPlaceholder')
})

const safeAreaTop = computed(() => safeArea.value.top || 0)

// ==================== Keyboard Avoidance ====================
//
// 双通道键盘检测，兼容不同 Android WebView 实现：
// - 通道 1 (visualViewport): 部分 WebView 在键盘弹出时 visualViewport.height 缩小，
//   通过 resize/scroll 事件检测，计算 fullLayoutHeight - viewportHeight 得到偏移量
// - 通道 2 (插件 keyboardHeight): 部分 WebView 的 visualViewport 不触发事件，
//   通过 tauri-plugin-edge-to-edge 的 safeAreaChanged 事件获取插件报告的键盘高度
//
// 最终偏移量取两个通道中较大的值，确保在所有设备上都能正确避让

// 通道 1: visualViewport
const fullLayoutHeight = ref(window.innerHeight)
const viewportHeight = ref(window.visualViewport?.height ?? window.innerHeight)

// 通道 2: 插件报告的键盘高度
const pluginKeyboardHeight = ref(0)

// 最终键盘偏移量：取两个通道中的较大值
const keyboardOffset = computed(() => {
  const vvOffset = fullLayoutHeight.value - viewportHeight.value
  const offset = Math.max(vvOffset, pluginKeyboardHeight.value)
  return offset > 10 ? offset : 0
})

// 通道 1 回调：visualViewport resize/scroll
function handleVisualViewportChange() {
  const vv = window.visualViewport
  if (!vv) return
  // 无键盘时更新基准高度
  if (!keyboardOffset.value) {
    fullLayoutHeight.value = window.innerHeight
  }
  viewportHeight.value = vv.height
}

// 通道 2 回调：插件 safeAreaChanged 事件
function handlePluginSafeAreaChange(e: Event) {
  const detail = (e as CustomEvent).detail as {
    keyboardHeight: number
    keyboardVisible: boolean
  }
  pluginKeyboardHeight.value = detail.keyboardVisible ? detail.keyboardHeight : 0
}

// terminal-view 只负责安全区域，不参与键盘避让动画
const terminalViewStyle = computed(() => ({
  paddingTop: `${safeAreaTop.value}px`,
}))

// 可移动区域：终端内容 + 输入栏，键盘弹出时整体上移
// 纯 transform 方案：GPU 合成不触发布局重排，无卡顿
//
// 配合 AndroidManifest adjustNothing：
// 系统不调整 WebView 大小，完全由 JS 控制偏移
// 双通道检测取较大值，兼容不同 WebView 的 visualViewport 行为
const movableAreaStyle = computed(() => {
  if (keyboardOffset.value <= 0) {
    return { transform: 'translateY(0)' }
  }
  // 只用 translateY 上移，不用 maxHeight
  // translateY(-keyboardOffset) 使 movable-area 底部恰好对齐键盘顶部
  // 底部超出 movable-clip 的部分由 overflow:hidden 裁剪
  return {
    transform: `translateY(-${keyboardOffset.value}px)`,
  }
})

/** 选择操作栏定位：避让选区和屏幕边界 */
const selectionBarStyle = computed(() => {
  const BAR_MARGIN = 10
  const EDGE_PADDING = 12

  const container = scrollContainer.value
  if (!container) return {}

  const rect = container.getBoundingClientRect()
  const estimatedBarWidth = 240
  const estimatedBarHeight = 40

  // 选区在容器内的像素范围（通过 viewport 行号 × 行高计算）
  let selTop = 0
  let selBottom = 0
  if (cellHeight.value > 0) {
    const topRow = Math.max(0, selectionViewportRange.topRow)
    const bottomRow = Math.min(terminalRef.value?.rows ?? topRow, selectionViewportRange.bottomRow + 1)
    selTop = topRow * cellHeight.value
    selBottom = bottomRow * cellHeight.value
  }

  // 水平：以长按位置为中心，限制不超出容器
  const relX = longPressTriggerPos.x - rect.left
  let left = relX - estimatedBarWidth / 2
  left = Math.max(EDGE_PADDING, Math.min(left, rect.width - estimatedBarWidth - EDGE_PADDING))

  // 垂直：优先选区上方，空间不足则选区下方，都不行则就近边缘
  let top: number
  const aboveTop = selTop - estimatedBarHeight - BAR_MARGIN
  const belowTop = selBottom + BAR_MARGIN

  if (aboveTop >= EDGE_PADDING) {
    top = aboveTop
  } else if (belowTop + estimatedBarHeight <= rect.height - EDGE_PADDING) {
    top = belowTop
  } else if (selTop < rect.height / 2) {
    // 选区偏上，操作栏放底部
    top = rect.height - estimatedBarHeight - EDGE_PADDING
  } else {
    // 选区偏下，操作栏放顶部
    top = EDGE_PADDING
  }

  return {
    top: `${top}px`,
    left: `${left}px`,
  }
})

// ==================== Watchers ====================

// 键盘偏移变化时的处理
// 动画期间临时启用 will-change 保证流畅，动画结束后移除避免 xterm 重影
watch(keyboardOffset, (newVal, oldVal) => {
  if (movableAreaRef.value) {
    movableAreaRef.value.style.willChange = 'transform'
  }

  setTimeout(() => {
    if (movableAreaRef.value) {
      movableAreaRef.value.style.willChange = 'auto'
    }
  }, 300)
})

watch(() => settingsStore.settings.ui.theme, (uiTheme) => {
  if (terminalSettings.value.isThemeUserSet) return
  const resolved = uiTheme === 'system'
    ? (isSystemDark.value ? 'dark' : 'light')
    : uiTheme
  if (terminalSettings.value.theme !== resolved) {
    terminalSettings.value.theme = resolved as string
    applyTerminalTheme()
  }
})

watch(isSystemDark, () => {
  if (terminalSettings.value.isThemeUserSet) return
  if (settingsStore.settings.ui.theme !== 'system') return
  terminalSettings.value.theme = isSystemDark.value ? 'dark' : 'light'
  applyTerminalTheme()
})

// ==================== Terminal Setup ====================

async function initTerminal() {
  if (!xtermContainer.value) return

  const theme = TERMINAL_THEMES[terminalSettings.value.theme]
  const term = new Terminal({
    theme: theme,
    fontFamily: '"Courier New", Courier, "Lucida Console", monospace',
    fontSize: terminalSettings.value.fontSize,
    lineHeight: 1.2,
    cursorBlink: true,
    cursorStyle: 'block',
    allowProposedApi: true,
    scrollback: 5000,
    convertEol: true,
    // 移动端禁用内置输入，避免弹出输入法
    disableStdin: true,
    // 移动端滚动灵敏度
    scrollSensitivity: 0.8,
  })

  terminalRef.value = term
  term.open(xtermContainer.value)

  const addon = new FitAddon()
  fitAddonRef.value = addon
  term.loadAddon(addon)
  term.loadAddon(new WebLinksAddon())

  // WebGL renderer — 后台加载
  try {
    const { WebglAddon } = await import('@xterm/addon-webgl')
    const webglAddon = new WebglAddon()
    term.loadAddon(webglAddon)
    webglAddon.onContextLoss(() => {})
  } catch {
    // WebGL 不可用时回退到 canvas 渲染器
  }

  // 从 buffer 写入历史数据
  writeBufferHistoryToTerminal(sessionId.value, term)

  // 注册实时 handler
  registerRealtimeHandler(sessionId.value, term)

  setTimeout(() => {
    fitTerminal(fitAddonRef.value)
    setupViewportScroll()
  }, 100)

  const observer = new ResizeObserver(() => {
    requestAnimationFrame(() => fitTerminal(fitAddonRef.value))
  })
  resizeObserverRef.value = observer
  observer.observe(xtermContainer.value)

  window.addEventListener('resize', handleWindowResize)

  term.onResize(({ cols, rows }) => {
    if (!isMockSession(sessionId.value) && isConnected.value && isSessionActive.value && sessionId.value) {
      wsResizeTerminal(sessionId.value, cols, rows).catch((e: Error) => {
        console.warn('[TerminalView] Resize failed:', e)
      })
    }
  })
}

function handleWindowResize() {
  setTimeout(() => fitTerminal(fitAddonRef.value), 100)
}

function disposeTerminal() {
  if (resizeObserverRef.value) {
    resizeObserverRef.value.disconnect()
    resizeObserverRef.value = null
  }
  window.removeEventListener('resize', handleWindowResize)

  if (sessionId.value) {
    unregisterRealtimeHandler(sessionId.value)
  }

  disposeScroll()

  if (terminalRef.value) {
    terminalRef.value.dispose()
    terminalRef.value = null
    fitAddonRef.value = null
  }
  isTerminalReady.value = false
}

function applyTerminalTheme() {
  if (!terminalRef.value) return
  const theme = TERMINAL_THEMES[terminalSettings.value.theme]
  terminalRef.value.options.theme = theme
  fitTerminal(fitAddonRef.value)
}

// ==================== Input Handlers ====================

function handleInputSubmit(text: string) {
  if (!terminalRef.value) return
  if (isMockSession(sessionId.value)) return
  if (isConnected.value && isSessionActive.value) {
    httpSendSessionInput(sessionId.value, text).then(result => {
      if (result.code !== 0) {
        console.error('[TerminalView] Send input failed:', result.message)
        toast.error(t('mobile.connection.connectFailed'))
      }
    })
  }
}

async function handleInputExecute(text: string) {
  if (!terminalRef.value) return
  if (isMockSession(sessionId.value)) return
  if (isConnected.value && isSessionActive.value) {
    const result = await httpSendSessionInput(sessionId.value, text, 'enter')
    if (result.code !== 0) {
      console.error('[TerminalView] Send input failed:', result.message)
      toast.error(t('mobile.connection.connectFailed'))
    }
  }
}

function handleSpecialKey(key: string) {
  if (isMockSession(sessionId.value)) return
  if (isConnected.value && isSessionActive.value) {
    httpSendSessionInput(sessionId.value, '', key).then(result => {
      if (result.code !== 0) {
        console.error('[TerminalView] Send special key failed:', result.message)
      }
    })
  }
}

// ==================== Toolbar Actions ====================

function handleToolbarAction(key: string) {
  switch (key) {
    case 'task': showTaskPicker.value = true; break
    case 'shortcut': showShortcutConfig.value = true; break
    case 'clear': showClearConfirm.value = true; break
    case 'refresh': refreshTerminal(); break
    case 'settings': showSettings.value = true; break
    case 'folder': showSidebar.value = !showSidebar.value; break
  }
}

// ==================== Settings ====================

function handleSettingsConfirm(settings: TerminalSettings) {
  terminalSettings.value.fontSize = settings.fontSize
  terminalSettings.value.theme = settings.theme
  terminalSettings.value.isThemeUserSet = settings.isThemeUserSet

  assistStore.saveSettings({
    quickBarCount: settings.quickBarCount,
    headerToolbarItems: settings.toolbarItems,
    terminalFontSize: terminalSettings.value.fontSize,
    terminalTheme: terminalSettings.value.isThemeUserSet ? terminalSettings.value.theme : null,
    isTerminalThemeUserSet: terminalSettings.value.isThemeUserSet,
  })

  applyTerminalTheme()
  applyScrollSettings(settings.theme, settings.fontSize, fitAddonRef.value)
  showSettings.value = false
}

// ==================== Clear Terminal ====================

function clearTerminal() {
  if (!terminalRef.value) return
  terminalRef.value.clear()
  currentLine.value = 0
  isUserScrolling.value = false
  showClearConfirm.value = false
}

// ==================== Refresh Terminal ====================

function refreshTerminal() {
  if (!fitAddonRef.value || !terminalRef.value) return

  fitAddonRef.value.fit()
  if (isConnected.value && isSessionActive.value) {
    wsResizeTerminal(sessionId.value, terminalRef.value.cols, terminalRef.value.rows).then(() => {
      toast.success(t('mobile.terminal.refreshed'))
    }).catch((e: Error) => {
      console.warn('[TerminalView] Refresh resize failed:', e)
      toast.error(t('mobile.terminal.refreshFailed'))
    })
  } else {
    toast.success(t('mobile.terminal.refreshed'))
  }
}

// ==================== Misc Handlers ====================

async function handleLongPress(name: string, path: string) {
  try {
    await writeClipboardText(path)
    toast.success(t('mobile.file.copied', { path }))
  } catch {
    toast.error(t('mobile.file.copyFailed'))
  }
}

function handleBack() {
  router.back()
}

async function onTaskSend(task: PresetTask) {
  if (!isConnected.value || !isSessionActive.value) {
    toast.error(t('mobile.connection.connectFailed'))
    return
  }
  try {
    await sendTask(task, sessionId.value)
  } catch {
    toast.error(t('mobile.toolbox.sendFailed'))
  }
}

async function onTaskExecute(task: PresetTask) {
  if (!isConnected.value || !isSessionActive.value) {
    toast.error(t('mobile.connection.connectFailed'))
    return
  }
  try {
    await executeTask(task, sessionId.value)
  } catch {
    toast.error(t('mobile.toolbox.sendFailed'))
  }
}

// ==================== Lifecycle ====================

onMounted(async () => {
  // 监听 visualViewport 变化，获取键盘弹出/收起的实际偏移
  if (window.visualViewport) {
    window.visualViewport.addEventListener('resize', handleVisualViewportChange)
    window.visualViewport.addEventListener('scroll', handleVisualViewportChange)
  }

  // 通道 2: 监听插件 safeAreaChanged 事件
  window.addEventListener('safeAreaChanged', handlePluginSafeAreaChange as EventListener)

  await nextTick()
  initTerminal()

  if (isMockSession(sessionId.value)) {
    if (terminalRef.value) {
      mockTerminal.startOutput(terminalRef.value)
    }
  } else if (isSessionActive.value && isConnected.value) {
    await subscribeSession(sessionId.value)
  }

  isTerminalReady.value = true
})

onUnmounted(async () => {
  // 移除 visualViewport 事件监听
  if (window.visualViewport) {
    window.visualViewport.removeEventListener('resize', handleVisualViewportChange)
    window.visualViewport.removeEventListener('scroll', handleVisualViewportChange)
  }
  window.removeEventListener('safeAreaChanged', handlePluginSafeAreaChange as EventListener)

  if (isMockSession(sessionId.value)) {
    mockTerminal.stopOutput()
  }
  disposeTerminal()

  if (!isSessionActive.value) {
    await unsubscribeSession(sessionId.value)
  }
})

watch(isSessionActive, async (active, prevActive) => {
  if (!sessionId.value || isMockSession(sessionId.value)) return
  if (active && !prevActive) {
    await subscribeSession(sessionId.value)
  } else if (!active && prevActive) {
    await handleSessionStopped(sessionId.value)
  }
})

watch(isConnected, async (connected) => {
  if (!sessionId.value || isMockSession(sessionId.value)) return
  if (!connected) {
    handleDisconnect()
  } else if (connected && isSessionActive.value) {
    await subscribeSession(sessionId.value)
  }
})
</script>
