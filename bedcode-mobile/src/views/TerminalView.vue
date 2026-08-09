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
              <transition name="scroll-indicator">
                <button
                  v-if="isUserScrolling && !isSelectionMode"
                  class="scroll-to-bottom-btn"
                  @click="scrollToBottomManual"
                  :title="t('mobile.terminal.scrollToBottom')"
                >
                  <svg class="scroll-to-bottom-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 14l-7 7m0 0l-7-7m7 7V3" />
                  </svg>
                </button>
              </transition>
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
            @settings-input-focus="handleSettingsInputFocus"
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
 * 终端视图（移动端）— xterm.js 渲染内核 + 移动端输入/键盘避让
 *
 * 渲染与滚动架构对齐桌面端 TerminalPreview.vue（VS Code 终端体验）：
 * - 写入管线：同帧输出经 rAF 合并 + DEC 2026 同步输出包裹 + 64KB 拆块
 *   （writeCoalescer），高频输出无撕裂/重影、超大块不卡主线程
 * - 渲染：WebGL addon（context loss 自动回退恢复），xterm 渲染循环自绘
 * - 滚动：onScroll 推导"是否在底部"（位置即状态），回到底部自动跟随输出
 * - 尺寸：ResizeObserver + rAF 节流 fit，cols/rows 实际变化才同步 PTY
 *
 * 移动端特殊处理：
 * - disableStdin：禁用 xterm 原生输入（桌面键盘输入流无法在移动端复现），
 *   输入统一由底部 TerminalInputBar 承担（命令/特殊键/快捷键面板）
 * - 触摸滚动接管：自定义触摸滚动 + 惯性 + 长按选择复制（useTerminalScroll）
 * - 键盘避让：visualViewport + 插件 safeAreaChanged 双通道检测，movable-area
 *   transform 上移（配合 AndroidManifest adjustNothing）
 * - Unicode11 addon：TUI 应用 box-drawing 字符列宽计算正确性
 */
defineOptions({ name: 'TerminalView' })

import { ref, computed, inject, type Ref, onMounted, onUnmounted, watch, nextTick } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import '@xterm/xterm/css/xterm.css'
import '@/styles/terminal.css'
import { useMobileConnection } from '@/composables/useMobileConnection'
import { isMockSession, useMockTerminal } from '@/composables/useMockTerminal'
import { useTerminalBuffer } from '@/composables/useTerminalBuffer'
import { wsResizeTerminal } from '@/composables/useMobileCommands'
import { httpSendSessionInput, httpResizeSession } from '@/composables/useHttpApi'
import { useOrientation } from '@/composables/useOrientation'
import { useTheme } from '@/composables/useTheme'
import { useSettingsStore } from '@/stores/settings'
import { useInputAssistantStore } from '@/stores/inputAssistant'
import { useTerminalScroll } from '@/composables/useTerminalScroll'
import { TERMINAL_SCROLLBACK } from '@/utils/terminalScrollback'
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
const { registerRealtimeHandler, unregisterRealtimeHandler, subscribeSession, unsubscribeSession, handleDisconnect, handleSessionStopped } = useTerminalBuffer()
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
// ResizeObserver rAF 节流句柄：同一帧内多次 fit 只执行一次
let resizeRaf = 0

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
  scrollToBottomManual,
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

// 侧边栏设置面板输入框聚焦时，禁用键盘避让
const settingsInputFocused = ref(false)

// 最终键盘偏移量：取两个通道中的较大值
const keyboardOffset = computed(() => {
  // 侧边栏设置面板输入框聚焦时，禁用键盘避让偏移
  if (settingsInputFocused.value) return 0
  const vvOffset = fullLayoutHeight.value - viewportHeight.value
  const offset = Math.max(vvOffset, pluginKeyboardHeight.value)
  return offset > 10 ? offset : 0
})
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
// 动画期间临时启用 will-change + transition 保证流畅，动画结束后移除避免 xterm 重影
// 持续开启 transition 会导致 movable-area 被 GPU 提升为合成层
// 触摸滚动时 WebGL canvas 在合成层上更新不同步，产生重影
watch(keyboardOffset, (newVal, oldVal) => {
  if (movableAreaRef.value) {
    movableAreaRef.value.style.willChange = 'transform'
    movableAreaRef.value.style.transition = 'transform 0.25s cubic-bezier(0.4, 0, 0.2, 1)'
  }

  setTimeout(() => {
    if (movableAreaRef.value) {
      movableAreaRef.value.style.willChange = 'auto'
      movableAreaRef.value.style.transition = 'none'
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

/**
 * WebGL 渲染器：动态加载（移动端包体积/启动优化），
 * 处理上下文丢失（丢失时回退 DOM 渲染，1s 后尝试重建）
 */
async function initWebGL(term: Terminal): Promise<boolean> {
  try {
    const { WebglAddon } = await import('@xterm/addon-webgl')
    const addon = new WebglAddon()
    addon.onContextLoss(() => {
      console.warn('[TerminalView] WebGL context lost, disposing renderer')
      addon.dispose()
      // 上下文丢失时恢复 DOM 层光标
      term.element?.classList.remove('xterm-hidden-cursor')
      // 延迟 1s 后尝试重新创建 WebGL 渲染器
      setTimeout(() => {
        if (terminalRef.value !== term) return
        try {
          const newAddon = new WebglAddon()
          newAddon.onContextLoss(() => {
            console.warn('[TerminalView] WebGL context lost again')
            newAddon.dispose()
            term.element?.classList.remove('xterm-hidden-cursor')
          })
          term.loadAddon(newAddon)
          term.element?.classList.add('xterm-hidden-cursor')
          console.info('[TerminalView] WebGL context recovered')
        } catch (e) {
          console.warn('[TerminalView] WebGL recovery failed, using canvas fallback:', e)
        }
      }, 1000)
    })
    term.loadAddon(addon)
    return true
  } catch {
    // WebGL 不可用时回退到 canvas 渲染器
    return false
  }
}

async function initTerminal() {
  if (!xtermContainer.value) return

  const term = new Terminal({
    // 字体与尺寸（对齐桌面端，VS Code 终端默认字体栈 + 跨平台回退）
    fontSize: terminalSettings.value.fontSize,
    fontFamily: 'Cascadia Mono, Consolas, Monaco, Courier New, monospace',
    lineHeight: 1,
    // 滚动历史行数（与桌面主机服务端事件队列容量对齐）
    scrollback: TERMINAL_SCROLLBACK,
    // 即时滚动：关闭平滑滚动，避免 WebGL 滚动动画期间合成器缓存旧帧导致重影
    smoothScrollDuration: 0,
    // VS Code 风格块光标：移动端保留光标（标记输入落点与 TUI 光标位置），
    // 仅隐藏 DOM 层光标避免与 WebGL 层双光标（见 xterm-hidden-cursor）
    cursorBlink: true,
    cursorStyle: 'block',
    cursorWidth: 1,
    drawBoldTextInBrightColors: true,
    // 移动端特殊处理：禁用 xterm 原生输入。
    // 桌面端键盘输入流（onData → PTY）无法在移动端复现，输入统一由底部
    // TerminalInputBar 承担，避免软键盘误弹与焦点抢占
    disableStdin: true,
    // 主题
    theme: TERMINAL_THEMES[terminalSettings.value.theme],
    allowProposedApi: true,
  })

  terminalRef.value = term

  // 挂载 addon（对齐桌面端顺序：addon 先于 open）
  const addon = new FitAddon()
  fitAddonRef.value = addon
  term.loadAddon(addon)
  term.loadAddon(new WebLinksAddon())

  // Unicode11 addon（移动端特殊处理）：启用 Unicode 11 字符宽度计算。
  // TUI 应用（opencode 等）大量使用 box-drawing 字符（╔═╗║╚╝）和 emoji，
  // 不加载此 addon 时 xterm 默认字符宽度表为 Unicode 5，
  // 部分新字符的列宽计算错误会导致光标位置漂移、上一个写入的字符部分残留（重影）
  const unicode11 = new Unicode11Addon()
  term.loadAddon(unicode11)
  term.unicode.activeVersion = '11'

  term.open(xtermContainer.value)

  // WebGL 渲染器激活后隐藏 DOM 层光标（保留 WebGL 层光标，避免双光标）
  const webglOk = await initWebGL(term)
  if (webglOk) {
    term.element?.classList.add('xterm-hidden-cursor')
  }

  // 注册实时 handler — 历史回放（订阅后服务端流式送达）与实时推送同通道，
  // 统一经 writeCoalescer 的 rAF 合并管线写入（DEC 2026 包裹 + 64KB 拆块）
  registerRealtimeHandler(sessionId.value, term)

  // 延迟 fit + 触摸滚动接管：等待 xterm 完成首帧布局，
  // 触摸监听挂到 viewport 上，需其在 DOM 中就绪
  setTimeout(() => {
    fitTerminal(fitAddonRef.value)
    setupViewportScroll()
  }, 100)

  // ResizeObserver — rAF 节流，避免快速连续 fit 导致的重复渲染；
  // 仅当 cols/rows 实际变化时同步 PTY（xterm 自身负责重绘）
  resizeObserverRef.value = new ResizeObserver(() => {
    if (resizeRaf) return
    resizeRaf = requestAnimationFrame(() => {
      resizeRaf = 0
      if (!fitAddonRef.value || !terminalRef.value) return
      const cols = terminalRef.value.cols
      const rows = terminalRef.value.rows
      fitAddonRef.value.fit()
      if (terminalRef.value.cols !== cols || terminalRef.value.rows !== rows) {
        syncTerminalSizeToHost()
      }
    })
  })
  resizeObserverRef.value.observe(xtermContainer.value)

  // PTY 尺寸同步：xterm 内部 resize（含 fit 触发）时同步到主机会话
  term.onResize(({ cols, rows }) => {
    if (!isMockSession(sessionId.value) && isConnected.value && isSessionActive.value && sessionId.value) {
      wsResizeTerminal(sessionId.value, cols, rows).catch((e: Error) => {
        console.warn('[TerminalView] Resize failed:', e)
      })
    }
  })
}

function disposeTerminal() {
  if (resizeObserverRef.value) {
    resizeObserverRef.value.disconnect()
    resizeObserverRef.value = null
  }
  if (resizeRaf) {
    cancelAnimationFrame(resizeRaf)
    resizeRaf = 0
  }

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
// 输入统一由 TerminalInputBar 承担（xterm 原生输入已禁用），
// 命令经 HTTP 发送到主机会话，特殊键以转义序列形式发送

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

/** 主动同步当前终端尺寸到主机 PTY（HTTP，带响应确认）
 * 重连/会话激活后 PTY 重建为默认 80x24，容器尺寸未变化时 fit/onResize 都不会触发，
 * 必须显式同步一次，否则输出按错误宽度换行导致格式混乱 */
async function syncTerminalSizeToHost() {
  if (!terminalRef.value || isMockSession(sessionId.value)) return
  if (!isConnected.value || !isSessionActive.value) return
  const { cols, rows } = terminalRef.value
  if (cols <= 0 || rows <= 0) return
  const result = await httpResizeSession(sessionId.value, cols, rows)
  if (result.code !== 0) {
    console.warn('[TerminalView] Sync size to host failed:', result.message)
  }
}

async function refreshTerminal() {
  if (!fitAddonRef.value || !terminalRef.value) return

  fitAddonRef.value.fit()
  // 强制重绘可见区：fit 尺寸不变时不触发重排，WebGL 渲染残留需要手动刷新
  if (terminalRef.value.rows > 0) {
    terminalRef.value.refresh(0, terminalRef.value.rows - 1)
  }

  if (isConnected.value && isSessionActive.value) {
    const result = await httpResizeSession(sessionId.value, terminalRef.value.cols, terminalRef.value.rows)
    if (result.code !== 0) {
      console.warn('[TerminalView] Refresh resize failed:', result.message)
      toast.error(t('mobile.terminal.refreshFailed'))
      return
    }
  }
  toast.success(t('mobile.terminal.refreshed'))
}

// ==================== Misc Handlers ====================

/** 侧边栏设置面板输入框聚焦/失焦时，控制键盘避让 */
function handleSettingsInputFocus(focused: boolean) {
  settingsInputFocused.value = focused
}

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
  await initTerminal()

  if (isMockSession(sessionId.value)) {
    if (terminalRef.value) {
      mockTerminal.startOutput(terminalRef.value)
    }
  } else if (isSessionActive.value && isConnected.value) {
    await subscribeSession(sessionId.value)
    syncTerminalSizeToHost()
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

  // 页面卸载即取消订阅：后台期间的输出由服务端环形保留，
  // 重新进入时以字节游标续传（服务端裁决 incremental/reset）
  if (!isMockSession(sessionId.value)) {
    await unsubscribeSession(sessionId.value)
  }
})

watch(isSessionActive, async (active, prevActive) => {
  if (!sessionId.value || isMockSession(sessionId.value)) return
  if (active && !prevActive) {
    await subscribeSession(sessionId.value)
    // 会话激活（含重连后）时 PTY 可能仍是默认尺寸，主动同步一次
    syncTerminalSizeToHost()
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
    // 重连成功后 PTY 重建为默认 80x24，需主动同步当前尺寸
    syncTerminalSizeToHost()
  }
})
</script>
