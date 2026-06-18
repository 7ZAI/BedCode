<template>
  <div
    class="terminal-view"
    :style="terminalViewStyle"
    @touchstart="onViewTouchStart"
    @touchmove="onViewTouchMove"
  >
    <!-- Header -->
    <header class="header">
      <button class="back-btn" @click="handleBack">
        <svg width="20" height="20" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <div class="status-area">
        <div class="status-dot" :class="statusClass"></div>
        <span class="status-text">{{ statusText }}</span>
      </div>
      <div class="header-title-area">
        <h1 class="header-title">{{ sessionName }}</h1>
      </div>
      <button class="clear-btn" @click="confirmClear" title="清屏">
        <svg width="20" height="20" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
        </svg>
      </button>
      <button class="refresh-btn" @click="refreshTerminal" title="刷新格式">
        <svg width="20" height="20" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
      </button>
      <button class="settings-btn" @click="openSettings" title="设置">
        <svg width="20" height="20" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
        </svg>
      </button>
      <button class="folder-btn" :class="{ active: showSidebar }" @click="showSidebar = !showSidebar" title="文件">
        <svg width="20" height="20" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
        </svg>
      </button>
    </header>

    <!-- Main Content: Terminal + Sidebar overlay -->
    <div class="main-content">
      <div class="terminal-output-area" @click.prevent.stop>
        <div
          ref="xtermContainer"
          class="xterm-container"
          @touchstart="onContainerTouchStart"
          @touchmove="onContainerTouchMove"
          @click.prevent.stop
        ></div>
      </div>

      <!-- File Sidebar - 覆盖层，不影响终端宽高 -->
      <transition name="sidebar-slide">
        <FileSidebar v-if="showSidebar" class="sidebar-overlay" :session-id="sessionId" />
      </transition>

      <!-- 点击侧边栏外部关闭 -->
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
    />

    <!-- Settings Modal -->
    <div v-if="showSettings" class="settings-modal-overlay" @click.self="cancelSettings">
      <div class="settings-modal" :style="settingsModalStyle">
        <div class="settings-header">
          <h2>终端设置</h2>
          <button class="close-btn" @click.stop="cancelSettings">
            <svg width="24" height="24" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div class="settings-content">
          <!-- Font Size -->
          <div class="settings-section">
            <label class="settings-label">字体大小</label>
            <div class="font-size-control">
              <button class="size-btn" @click.stop="tempFontSize--" :disabled="tempFontSize <= 10">-</button>
              <span class="size-value">{{ tempFontSize }}px</span>
              <button class="size-btn" @click.stop="tempFontSize++" :disabled="tempFontSize >= 24">+</button>
            </div>
          </div>

          <!-- Theme -->
          <div class="settings-section">
            <label class="settings-label">主题</label>
            <div class="theme-grid">
              <button
                v-for="(theme, name) in TERMINAL_THEMES"
                :key="name"
                class="theme-btn"
                :class="{ active: tempTheme === name }"
                @click.stop="tempTheme = name"
              >
                <span class="theme-preview" :style="{ background: theme.background, color: theme.foreground }">Aa</span>
                <span class="theme-name">{{ theme.label }}</span>
              </button>
            </div>
          </div>
        </div>

        <!-- Settings Footer -->
        <div class="settings-footer">
          <button class="settings-footer-btn cancel" @click.stop="cancelSettings">取消</button>
          <button class="settings-footer-btn confirm" @click.stop="confirmSettings">确认</button>
        </div>
      </div>
    </div>

    <!-- Clear Confirm Modal -->
    <div v-if="showClearConfirm" class="confirm-modal-overlay" @click.self="showClearConfirm = false">
      <div class="confirm-modal" :style="confirmModalStyle">
        <p class="confirm-text">确定要清空终端内容吗？</p>
        <div class="confirm-buttons">
          <button class="confirm-btn cancel" @click.stop="showClearConfirm = false">取消</button>
          <button class="confirm-btn confirm" @click.stop="clearTerminal">确定</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, inject, type Ref, onMounted, onUnmounted, onActivated, onDeactivated, watch, nextTick } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import '@xterm/xterm/css/xterm.css'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useMobileConnection } from '@/modules/mobile/composables/useMobileConnection'
import {
  wsJoinSession,
  wsLeaveSession,
  wsSendInput,
  wsResizeTerminal,
} from '@/modules/mobile/composables/useMobileCommands'
import { useOrientation } from '@/modules/mobile/composables/useOrientation'
import TerminalInputBar from '@/modules/mobile/components/TerminalInputBar.vue'
import FileSidebar from '@/modules/mobile/components/FileSidebar.vue'
import { useToast } from '@/modules/shared/composables/useToast'

// ==================== Props & Route ====================

const router = useRouter()
const route = useRoute()
const connection = useMobileConnection()
const toast = useToast()
const { isLandscape } = useOrientation()
// 安全区域从 App.vue inject，不独立初始化 useEdgeToEdge
const safeArea = inject<Ref<{ top: number; bottom: number }>>('safeArea')!
const keyboardInfo = inject<Ref<{ keyboardHeight: number; isVisible: boolean }>>('keyboardInfo')!

const sessionId = computed(() => route.params.id as string)

// ==================== State ====================
// 注意：使用 ref 确保每个组件实例有独立的状态
// 在 <script setup> 中，顶层 let 声明的变量是模块级共享的

const xtermContainer = ref<HTMLDivElement | null>(null)
// 终端实例 - 使用 ref 确保组件隔离
const terminalRef = ref<Terminal | null>(null)
const fitAddonRef = ref<FitAddon | null>(null)
const resizeObserverRef = ref<ResizeObserver | null>(null)
// 终端输出事件监听器 - 使用 ref 确保组件隔离
const outputListenerRef = ref<UnlistenFn | null>(null)
// 输出索引去重 - 使用 ref 确保组件隔离
const lastIndexRef = ref(-1)
// 当前订阅的会话 ID - 用于取消订阅时使用（避免路由变化后 sessionId 变成 undefined）
const subscribedSessionIdRef = ref<string | null>(null)

// 设置相关状态
const showSettings = ref(false)
const showClearConfirm = ref(false)
const showSidebar = ref(false)
const terminalSettings = ref({
  fontSize: 14,
  theme: 'dark',
})

// 临时设置（用于编辑中的状态）
const tempFontSize = ref(14)
const tempTheme = ref('dark')

// 弹窗安全区域样式
const settingsModalStyle = computed(() => ({
  paddingTop: `${safeArea.value.top}px`,
  paddingBottom: `${safeArea.value.bottom}px`,
}))

const confirmModalStyle = computed(() => ({
  paddingTop: `${safeArea.value.top}px`,
  paddingBottom: `${safeArea.value.bottom}px`,
}))

// ==================== Terminal Themes ====================

const TERMINAL_THEMES: Record<string, any> = {
  dark: {
    label: '深色',
    background: '#0a0a0f',
    foreground: '#e0e0e0',
    cursor: '#00d4ff',
    cursorAccent: '#0a0a0f',
    selectionBackground: '#1a3a4a',
    black: '#000000',
    red: '#ff5555',
    green: '#50fa7b',
    yellow: '#f1fa8c',
    blue: '#bd93f9',
    magenta: '#ff79c6',
    cyan: '#8be9fd',
    white: '#bbbbbb',
    brightBlack: '#555555',
    brightRed: '#ff5555',
    brightGreen: '#50fa7b',
    brightYellow: '#f1fa8c',
    brightBlue: '#bd93f9',
    brightMagenta: '#ff79c6',
    brightCyan: '#8be9fd',
    brightWhite: '#ffffff',
  },
  light: {
    label: '浅色',
    background: '#ffffff',
    foreground: '#1a1a1a',
    cursor: '#0066cc',
    cursorAccent: '#ffffff',
    selectionBackground: '#b3d7ff',
    black: '#000000',
    red: '#cc0000',
    green: '#008800',
    yellow: '#996600',
    blue: '#0066cc',
    magenta: '#cc00cc',
    cyan: '#008888',
    white: '#cccccc',
    brightBlack: '#666666',
    brightRed: '#ff0000',
    brightGreen: '#00cc00',
    brightYellow: '#ccaa00',
    brightBlue: '#0088ff',
    brightMagenta: '#ff00ff',
    brightCyan: '#00cccc',
    brightWhite: '#ffffff',
  },
  dracula: {
    label: 'Dracula',
    background: '#282a36',
    foreground: '#f8f8f2',
    cursor: '#f8f8f0',
    cursorAccent: '#282a36',
    selectionBackground: '#44475a',
    black: '#000000',
    red: '#ff5555',
    green: '#50fa7b',
    yellow: '#f1fa8c',
    blue: '#bd93f9',
    magenta: '#ff79c6',
    cyan: '#8be9fd',
    white: '#bfbfbf',
    brightBlack: '#282a36',
    brightRed: '#ff5555',
    brightGreen: '#50fa7b',
    brightYellow: '#f1fa8c',
    brightBlue: '#bd93f9',
    brightMagenta: '#ff79c6',
    brightCyan: '#8be9fd',
    brightWhite: '#f8f8f2',
  },
  monokai: {
    label: 'Monokai',
    background: '#272822',
    foreground: '#f8f8f2',
    cursor: '#f8f8f0',
    cursorAccent: '#272822',
    selectionBackground: '#49483e',
    black: '#000000',
    red: '#f92672',
    green: '#a6e22e',
    yellow: '#f4bf75',
    blue: '#66d9ef',
    magenta: '#ae81ff',
    cyan: '#a1efe4',
    white: '#f8f8f2',
    brightBlack: '#75715e',
    brightRed: '#f92672',
    brightGreen: '#a6e22e',
    brightYellow: '#f4bf75',
    brightBlue: '#66d9ef',
    brightMagenta: '#ae81ff',
    brightCyan: '#a1efe4',
    brightWhite: '#f9f8f5',
  },
  nord: {
    label: 'Nord',
    background: '#2e3440',
    foreground: '#d8dee9',
    cursor: '#d8dee9',
    cursorAccent: '#2e3440',
    selectionBackground: '#434c5e',
    black: '#3b4252',
    red: '#bf616a',
    green: '#a3be8c',
    yellow: '#ebcb8b',
    blue: '#81a1c1',
    magenta: '#b48ead',
    cyan: '#88c0d0',
    white: '#e5e9f0',
    brightBlack: '#4c566a',
    brightRed: '#bf616a',
    brightGreen: '#a3be8c',
    brightYellow: '#ebcb8b',
    brightBlue: '#81a1c1',
    brightMagenta: '#b48ead',
    brightCyan: '#8fbcbb',
    brightWhite: '#eceff4',
  },
}

// ==================== Settings Functions ====================

function openSettings() {
  // 打开设置时，用当前设置初始化临时状态
  tempFontSize.value = terminalSettings.value.fontSize
  tempTheme.value = terminalSettings.value.theme
  showSettings.value = true
}

function cancelSettings() {
  showSettings.value = false
}

function confirmSettings() {
  // 确认时才应用设置
  terminalSettings.value.fontSize = tempFontSize.value
  terminalSettings.value.theme = tempTheme.value
  applySettings()
  showSettings.value = false
}

function applySettings() {
  if (!terminalRef.value) return

  const theme = TERMINAL_THEMES[terminalSettings.value.theme]

  // 单独设置每个属性，避免覆盖整个 options 对象
  terminalRef.value.options.theme = theme
  terminalRef.value.options.fontSize = terminalSettings.value.fontSize

  // 重新 fit 终端
  setTimeout(() => fitTerminal(), 50)
}

// ==================== Computed ====================

const isConnected = computed(() =>
  connection.connectionStatus.value === 'connected' ||
  connection.connectionStatus.value === 'paired'
)

const session = computed(() =>
  connection.activeSessions.value.find(s => s.id === sessionId.value)
)

const sessionName = computed(() => {
  return session.value?.name || sessionId.value || '终端'
})

const sessionStatus = computed(() => {
  return session.value?.status || 'stopped'
})

const isSessionActive = computed(() =>
  sessionStatus.value === 'running'
)

const statusClass = computed(() => {
  if (sessionStatus.value === 'running') return 'status-running'
  if (sessionStatus.value === 'stopped') return 'status-stopped'
  return 'status-unknown'
})

const statusText = computed(() => {
  if (sessionStatus.value === 'running') return '运行中'
  if (sessionStatus.value === 'stopped') return '已停止'
  return '未知'
})

const inputPlaceholder = computed(() => {
  if (!isConnected.value) return '未连接...'
  if (!isSessionActive.value) return '会话已停止'
  return '输入命令...'
})

// 安全区域
const safeAreaTop = computed(() => safeArea.value.top || 0)
const keyboardHeight = computed(() => keyboardInfo.value.keyboardHeight || 0)

// 终端视图样式：顶部安全区 + 键盘避让
// 底部安全区由 TerminalInputBar 的 paddingBottom 承担，这里只处理键盘避让
// Android WebView 不支持 CSS env(safe-area-inset-*)，完全依赖 JS 值
const terminalViewStyle = computed(() => ({
  paddingTop: `${safeAreaTop.value}px`,
  paddingBottom: keyboardHeight.value > 0 ? `${keyboardHeight.value}px` : '0px',
}))

// 监听键盘变化，重新 fit 终端
watch(() => keyboardInfo.value.keyboardHeight, () => {
  setTimeout(() => fitTerminal(), 100)
})

// ==================== Terminal Setup ====================

async function initTerminal() {
  // console.log('[TerminalView] initTerminal called, xtermContainer:', xtermContainer.value)

  if (!xtermContainer.value) {
    // console.error('[TerminalView] xtermContainer is null!')
    return
  }

  // console.log('[TerminalView] Container dimensions:', xtermContainer.value.offsetWidth, 'x', xtermContainer.value.offsetHeight)

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
  })

  terminalRef.value = term
  term.open(xtermContainer.value)

  // Load addons
  const addon = new FitAddon()
  fitAddonRef.value = addon
  term.loadAddon(addon)
  term.loadAddon(new WebLinksAddon())

  // WebGL renderer - 提升渲染性能
  try {
    const { WebglAddon } = await import('@xterm/addon-webgl')
    const webglAddon = new WebglAddon()
    term.loadAddon(webglAddon)
    webglAddon.onContextLoss(() => {
      // console.warn('[TerminalView] WebGL context lost')
    })
    // console.log('[TerminalView] WebGL renderer loaded')
  } catch (e) {
    // console.warn('[TerminalView] WebGL not supported, using DOM renderer:', e)
  }

  // Welcome message
  term.write('\x1b[36m[终端]\x1b[0m ' + sessionName.value + '\r\n')
  term.write('='.repeat(50) + '\r\n\r\n')

  // Fit terminal - delay to ensure container is rendered
  setTimeout(() => {
    // console.log('[TerminalView] Delayed fit, container dimensions:', xtermContainer.value?.offsetWidth, 'x', xtermContainer.value?.offsetHeight)
    fitTerminal()

    // 调试：检查 xterm 内部结构
    const xtermElement = xtermContainer.value?.querySelector('.xterm') as HTMLElement
    const viewport = xtermContainer.value?.querySelector('.xterm-viewport') as HTMLElement
    const screenElement = xtermContainer.value?.querySelector('.xterm-screen') as HTMLElement
    // const helper = xtermContainer.value?.querySelector('.xterm-helpers') as HTMLElement

    // console.log('[TerminalView] xterm structure:', {
    //   xterm: !!xtermElement,
    //   viewport: !!viewport,
    //   screen: !!screenElement,
    //   helper: !!helper,
    // })

    if (viewport) {
      // console.log('[TerminalView] Viewport found:', {
      //   height: viewport.style.height,
      //   overflowY: getComputedStyle(viewport).overflowY,
      //   scrollHeight: viewport.scrollHeight,
      //   clientHeight: viewport.clientHeight,
      // })

      // 关键修复：确保 viewport 支持触摸滚动
      viewport.style.touchAction = 'pan-y'
      viewport.style.overflowY = 'auto'

      // 添加触摸事件监听调试（已注释）
      // viewport.addEventListener('touchstart', (e) => {
      //   console.log('[TerminalView] Touch start on viewport, touches:', e.touches.length)
      // }, { passive: true })

      // viewport.addEventListener('touchmove', (e) => {
      //   console.log('[TerminalView] Touch move on viewport, deltaY:', e.touches[0]?.clientY)
      // }, { passive: true })

      // 添加滚轮事件监听调试（已注释）
      // viewport.addEventListener('wheel', (e) => {
      //   console.log('[TerminalView] Wheel event on viewport:', e.deltaY)
      // }, { passive: true })
    } else {
      // console.error('[TerminalView] Viewport not found!')
    }

    // 确保 xterm 主元素不阻止触摸
    if (xtermElement) {
      xtermElement.style.touchAction = 'pan-y'
      // console.log('[TerminalView] Set touch-action on .xterm')

      // 尝试在 xterm 主元素上监听滚轮（已注释）
      // xtermElement.addEventListener('wheel', (e) => {
      //   console.log('[TerminalView] Wheel on .xterm:', e.deltaY)
      //   // 尝试手动触发 xterm 滚动
      //   if (terminal) {
      //     const scrollAmount = Math.round(e.deltaY / 20)
      //     terminal.scrollLines(scrollAmount)
      //     console.log('[TerminalView] Manually scrolled:', scrollAmount)
      //   }
      // }, { passive: true })
    }

    // 确保屏幕元素不阻止触摸
    if (screenElement) {
      screenElement.style.touchAction = 'pan-y'
      // console.log('[TerminalView] Set touch-action on .xterm-screen')
    }
  }, 100)

  // Resize observer
  const observer = new ResizeObserver(() => {
    requestAnimationFrame(fitTerminal)
  })
  resizeObserverRef.value = observer
  observer.observe(xtermContainer.value)

  // Window resize
  window.addEventListener('resize', handleWindowResize)

  // Terminal resize 事件：通知桌面端调整 PTY 大小
  // 注意：捕获当前 sessionId，避免路由切换后读取错误的会话 ID
  const currentSessionIdForResize = sessionId.value
  term.onResize(({ cols, rows }) => {
    if (isConnected.value && isSessionActive.value) {
      wsResizeTerminal(currentSessionIdForResize, cols, rows).catch((e: Error) => {
        console.warn('[TerminalView] Resize failed:', e)
      })
    }
  })
}

function fitTerminal() {
  if (!fitAddonRef.value || !terminalRef.value) return
  try {
    fitAddonRef.value.fit()
  } catch (e) {
    console.warn('[TerminalView] fit failed:', e)
  }
}

function handleWindowResize() {
  setTimeout(fitTerminal, 100)
}

function disposeTerminal() {
  if (resizeObserverRef.value) {
    resizeObserverRef.value.disconnect()
    resizeObserverRef.value = null
  }
  window.removeEventListener('resize', handleWindowResize)
  // 清理输出监听器
  if (outputListenerRef.value) {
    outputListenerRef.value()
    outputListenerRef.value = null
  }
  if (terminalRef.value) {
    terminalRef.value.dispose()
    terminalRef.value = null
    fitAddonRef.value = null
  }
  lastIndexRef.value = -1
  subscribedSessionIdRef.value = null
}

/// 创建前端事件监听器（不调用后端订阅）
/// 用于 onActivated 时恢复前端监听，后端订阅已保持活跃
async function createFrontendListener() {
  if (outputListenerRef.value) {
    outputListenerRef.value()
    outputListenerRef.value = null
  }

  const currentSessionId = sessionId.value
  if (!currentSessionId) {
    return
  }

  try {
    outputListenerRef.value = await listen<{
      session_id: string
      data: string
      index: number
      is_waiting: boolean
    }>('ws_output', (event) => {
      // 只处理当前会话的输出（前端过滤）
      if (event.payload.session_id !== currentSessionId) {
        return
      }

      // 索引去重：避免重复输出（重连时可能发生）
      if (event.payload.index !== undefined && event.payload.index <= lastIndexRef.value) {
        return
      }
      lastIndexRef.value = event.payload.index

      // 写入终端
      if (terminalRef.value) {
        terminalRef.value.write(event.payload.data)
      }
    })
  } catch (e) {
    console.error('[TerminalView] Failed to create frontend listener:', e)
  }
}

// ==================== Input Handlers ====================

async function subscribeSession() {
  if (!isConnected.value) {
    return
  }

  // 先清理可能存在的旧监听器，避免重复订阅
  if (outputListenerRef.value) {
    outputListenerRef.value()
    outputListenerRef.value = null
  }

  try {
    // 加入会话，开始接收输出（后端订阅）
    await wsJoinSession(sessionId.value)

    // 监听终端输出事件 - 捕获当前 sessionId 确保闭包正确
    const currentSessionId = sessionId.value
    // 保存订阅的会话 ID，用于取消订阅时使用
    subscribedSessionIdRef.value = currentSessionId
    outputListenerRef.value = await listen<{
      session_id: string
      data: string
      index: number
      is_waiting: boolean
    }>('ws_output', (event) => {
      // 只处理当前会话的输出（前端过滤）
      if (event.payload.session_id !== currentSessionId) {
        return
      }

      // 索引去重：避免重复输出（重连时可能发生）
      if (event.payload.index !== undefined && event.payload.index <= lastIndexRef.value) {
        return
      }
      lastIndexRef.value = event.payload.index

      // 写入终端
      if (terminalRef.value) {
        terminalRef.value.write(event.payload.data)
      }
    })
  } catch (e) {
    console.error('[TerminalView] Subscribe failed:', e)
    toast.error('订阅终端失败')
  }
}

/// 清理前端事件监听器（不取消后端订阅）
/// 用于组件停用时清理，保持后端订阅以持续接收输出
function clearFrontendListener() {
  if (outputListenerRef.value) {
    outputListenerRef.value()
    outputListenerRef.value = null
  }
}

/// 取消订阅会话（包括后端订阅）
/// 用于会话停止、删除或组件销毁时
async function unsubscribeSession() {
  // 使用保存的会话 ID（避免路由变化后 sessionId 变成 undefined）
  const sessionToLeave = subscribedSessionIdRef.value

  // 清理输出监听器
  if (outputListenerRef.value) {
    outputListenerRef.value()
    outputListenerRef.value = null
  }

  // 清理保存的会话 ID
  subscribedSessionIdRef.value = null

  // 清理去重索引
  lastIndexRef.value = -1

  if (!isConnected.value || !sessionToLeave) {
    return
  }

  try {
    await wsLeaveSession(sessionToLeave)
  } catch (e) {
    console.error('[TerminalView] Unsubscribe failed:', e)
  }
}

// ==================== Input Handlers ====================

function handleInputSubmit(text: string) {
  if (!terminalRef.value) return

  // 发送输入到桌面端（不带换行，仅输入文本）
  if (isConnected.value && isSessionActive.value) {
    wsSendInput(sessionId.value, text).catch(e => {
      console.error('[TerminalView] Send input failed:', e)
      toast.error('发送命令失败')
    })
  }
}

async function handleInputExecute(text: string) {
  if (!terminalRef.value) return

  // 发送输入到桌面端，然后发送 enter 特殊键执行命令
  if (isConnected.value && isSessionActive.value) {
    try {
      // 先发送文本
      await wsSendInput(sessionId.value, text)
      // 再发送 enter 特殊键
      await wsSendInput(sessionId.value, '', 'enter')
    } catch (e) {
      console.error('[TerminalView] Send input failed:', e)
      toast.error('发送命令失败')
    }
  }
}

function handleSpecialKey(key: string) {
  // 发送特殊键到桌面端
  if (isConnected.value && isSessionActive.value) {
    wsSendInput(sessionId.value, '', key).catch(e => {
      console.error('[TerminalView] Send special key failed:', e)
    })
  }
}

// ==================== Clear Terminal ====================

function confirmClear() {
  showClearConfirm.value = true
}

async function clearTerminal() {
  if (!terminalRef.value) return

  terminalRef.value.clear()
  showClearConfirm.value = false
}

// ==================== Refresh Terminal Format ====================

function refreshTerminal() {
  // 刷新格式：重新 fit 终端尺寸并同步到桌面端，不清除内容
  if (!fitAddonRef.value || !terminalRef.value) return

  // 捕获当前 sessionId，避免路由切换后读取错误的会话 ID
  const currentSessionId = sessionId.value

  fitAddonRef.value.fit()
  if (isConnected.value && isSessionActive.value) {
    wsResizeTerminal(currentSessionId, terminalRef.value.cols, terminalRef.value.rows).catch((e: Error) => {
      console.warn('[TerminalView] Refresh resize failed:', e)
    })
  }
}

// ==================== Navigation ====================

function handleBack() {
  router.back()
}

// ==================== Touch Scroll ====================

// lastTouchY 是触摸滚动用的，保持模块级即可（无状态共享问题）
let lastTouchY = 0

function onViewTouchStart(e: TouchEvent) {
  lastTouchY = e.touches[0]?.clientY || 0
}

function onViewTouchMove(e: TouchEvent) {
  // 手动处理触摸滚动
  const currentY = e.touches[0]?.clientY || 0
  const deltaY = lastTouchY - currentY
  lastTouchY = currentY

  if (terminalRef.value && Math.abs(deltaY) > 1) {
    const scrollAmount = Math.round(deltaY / 10)
    terminalRef.value.scrollLines(scrollAmount)
  }
}

function onContainerTouchStart(e: TouchEvent) {
  lastTouchY = e.touches[0]?.clientY || 0
}

function onContainerTouchMove(e: TouchEvent) {
  // 手动处理触摸滚动
  const currentY = e.touches[0]?.clientY || 0
  const deltaY = lastTouchY - currentY
  lastTouchY = currentY

  if (terminalRef.value && Math.abs(deltaY) > 1) {
    const scrollAmount = Math.round(deltaY / 10)
    terminalRef.value.scrollLines(scrollAmount)
  }
}

// ==================== Lifecycle ====================

onMounted(async () => {
  await nextTick()
  initTerminal()

  // 首次进入时订阅输出（后端订阅 + 前端监听）
  if (isSessionActive.value && isConnected.value) {
    await subscribeSession()
  }
})

onUnmounted(async () => {
  // 组件销毁时完全取消订阅（包括后端）
  // 注意：keep-alive 缓存的组件不会触发 onUnmounted
  await unsubscribeSession()
  disposeTerminal()
})

// keep-alive 生命周期：组件被激活时检查订阅状态
// 多终端同时存活模式下，不需要清理监听器
onActivated(async () => {
  // 如果没有监听器且会话活跃，创建监听器
  // 正常情况下监听器应该已经存在（onMounted 创建的）
  if (isConnected.value && isSessionActive.value && !outputListenerRef.value) {
    await createFrontendListener()
  }
})

// keep-alive 生命周期：组件被停用时不做任何操作
// 保持前端监听器活跃，让所有终端实时接收输出
onDeactivated(() => {
  // 不清理监听器，保持实时接收输出
})

// Watch session status changes
// 注意：只在组件处于活跃状态且路由正确时才响应状态变化
watch(isSessionActive, async (active, prevActive) => {
  // 如果 sessionId 不存在（路由已离开），忽略状态变化
  if (!sessionId.value) {
    return
  }

  if (active && !prevActive) {
    // Session became active - 需要完整订阅（后端 + 前端）
    await subscribeSession()
  } else if (!active && prevActive) {
    // Session stopped - 完全取消订阅
    await unsubscribeSession()
  }
})

// Watch connection status changes
watch(isConnected, async (connected) => {
  // 如果 sessionId 不存在（路由已离开），忽略连接变化
  if (!sessionId.value) {
    return
  }

  if (!connected) {
    // 连接断开 - 清理前端监听器，后端订阅会自动失效
    clearFrontendListener()
    subscribedSessionIdRef.value = null
  } else if (connected && isSessionActive.value) {
    // 连接恢复 - 需要完整订阅（后端 + 前端）
    await subscribeSession()
  }
})
</script>

<style scoped>
.terminal-view {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--mobile-terminal-bg);
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 1;
  /* 禁止页面整体滚动，但允许子元素滚动 */
  overflow: hidden;
  /* 关键：允许子元素的触摸滚动传递 */
  touch-action: pan-y;
  /* 平滑过渡动画 - 避免键盘弹出时闪现 */
  transition: padding-top 0.25s cubic-bezier(0.4, 0, 0.2, 1),
              padding-bottom 0.25s cubic-bezier(0.4, 0, 0.2, 1);
}

/* Header */
.header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.75rem 1rem;
  background: var(--mobile-terminal-header);
  backdrop-filter: blur(20px);
  border-bottom: 1px solid var(--mobile-border);
  flex-shrink: 0;
  position: relative;
  z-index: 10;
}

.back-btn {
  padding: 0.5rem;
  margin-left: -0.5rem;
  color: var(--mobile-text-secondary);
  background: none;
  border: none;
  cursor: pointer;
  transition: color 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.back-btn:hover {
  color: var(--accent, #00d4ff);
}

.header-title-area {
  flex: 1;
  min-width: 0;
}

.header-title {
  font-size: 1rem;
  font-weight: 600;
  color: var(--mobile-text-primary);
  margin: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.status-area {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  font-size: 0.75rem;
}

.status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
}

.status-running {
  background: var(--success, #10b981);
  box-shadow: 0 0 6px rgba(16, 185, 129, 0.5);
}

.status-stopped {
  background: var(--error, #ef4444);
}

.status-unknown {
  background: var(--text-muted, #6b7280);
}

.status-text {
  color: var(--mobile-text-muted);
}

.clear-btn {
  padding: 0.5rem;
  border-radius: 0.5rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-secondary);
  cursor: pointer;
  transition: all 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.clear-btn:hover {
  border-color: rgba(0, 212, 255, 0.3);
}

.refresh-btn {
  padding: 0.5rem;
  border-radius: 0.5rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-secondary);
  cursor: pointer;
  transition: all 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.refresh-btn:hover {
  border-color: rgba(0, 212, 255, 0.3);
}

.settings-btn {
  padding: 0.5rem;
  border-radius: 0.5rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-secondary);
  cursor: pointer;
  transition: all 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.settings-btn:hover {
  border-color: rgba(0, 212, 255, 0.3);
}

/* Main Content Area */
.main-content {
  flex: 1;
  min-height: 0;
  position: relative;
  overflow: hidden;
}

/* Sidebar overlay - 浮动在终端上方，不影响终端宽高 */
.sidebar-overlay {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  z-index: 20;
  box-shadow: -4px 0 16px rgba(0, 0, 0, 0.3);
}

.sidebar-backdrop {
  position: absolute;
  inset: 0;
  z-index: 15;
}

/* Folder Button */
.folder-btn {
  padding: 0.5rem;
  border-radius: 0.5rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-secondary);
  cursor: pointer;
  transition: all 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.folder-btn:hover {
  border-color: rgba(0, 212, 255, 0.3);
}

.folder-btn.active {
  color: var(--mobile-accent);
  border-color: var(--mobile-border-active);
  background: var(--mobile-accent-muted);
}

/* Sidebar Slide Transition */
.sidebar-slide-enter-active,
.sidebar-slide-leave-active {
  transition: transform 0.25s cubic-bezier(0.4, 0, 0.2, 1);
}

.sidebar-slide-enter-from,
.sidebar-slide-leave-to {
  transform: translateX(100%);
}

/* Settings Modal */
.settings-modal-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.7);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  padding: 1rem;
}

.settings-modal {
  background: var(--mobile-bg-secondary);
  border-radius: 1rem;
  width: 100%;
  max-width: 360px;
  max-height: 80vh;
  overflow-y: auto;
}

.settings-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.75rem 1rem;
  border-bottom: 1px solid var(--mobile-border);
}

.settings-header h2 {
  font-size: 1rem;
  font-weight: 600;
  color: var(--mobile-text-primary);
  margin: 0;
}

.close-btn {
  padding: 0.25rem;
  background: none;
  border: none;
  color: var(--mobile-text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}

.close-btn:hover {
  color: var(--mobile-text-primary);
}

.settings-content {
  padding: 1rem;
}

.settings-section {
  margin-bottom: 1.5rem;
}

.settings-section:last-child {
  margin-bottom: 0;
}

.settings-label {
  display: block;
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--mobile-text-muted);
  margin-bottom: 0.75rem;
}

.font-size-control {
  display: flex;
  align-items: center;
  gap: 1rem;
}

.size-btn {
  width: 40px;
  height: 40px;
  border-radius: 0.5rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-primary);
  font-size: 1.25rem;
  cursor: pointer;
  transition: all 0.2s ease;
}

.size-btn:hover:not(:disabled) {
  background: var(--mobile-bg-hover);
}

.size-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.size-value {
  flex: 1;
  text-align: center;
  font-size: 1.125rem;
  font-weight: 500;
  color: var(--mobile-text-primary);
}

.theme-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 0.5rem;
}

.theme-btn {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.375rem;
  padding: 0.75rem 0.5rem;
  border-radius: 0.5rem;
  background: var(--mobile-bg-elevated);
  border: 2px solid transparent;
  cursor: pointer;
  transition: all 0.2s ease;
}

.theme-btn:hover {
  background: var(--mobile-bg-hover);
}

.theme-btn.active {
  border-color: #00d4ff;
  background: rgba(0, 212, 255, 0.15);
  box-shadow: 0 0 12px rgba(0, 212, 255, 0.3);
}

.theme-preview {
  width: 100%;
  padding: 0.5rem;
  border-radius: 0.375rem;
  text-align: center;
  font-size: 0.875rem;
  font-weight: 600;
}

.theme-name {
  font-size: 0.75rem;
  color: var(--mobile-text-muted);
}

.theme-btn.active .theme-name {
  color: #00d4ff;
  font-weight: 600;
}

.settings-footer {
  display: flex;
  gap: 0.75rem;
  padding: 0.75rem 1rem;
  border-top: 1px solid var(--mobile-border);
}

.settings-footer-btn {
  flex: 1;
  padding: 0.75rem;
  border-radius: 0.5rem;
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.settings-footer-btn.cancel {
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-muted);
}

.settings-footer-btn.cancel:hover {
  background: var(--mobile-bg-hover);
  color: var(--mobile-text-primary);
}

.settings-footer-btn.confirm {
  background: #00d4ff;
  border: none;
  color: #0a0a0f;
}

.settings-footer-btn.confirm:hover {
  background: #00b8e6;
}

/* Confirm Modal */
.confirm-modal-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.7);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  padding: 1rem;
}

.confirm-modal {
  background: var(--mobile-bg-secondary);
  border-radius: 1rem;
  padding: 1.5rem;
  width: 100%;
  max-width: 300px;
  text-align: center;
}

.confirm-text {
  font-size: 1rem;
  color: var(--mobile-text-primary);
  margin: 0 0 1.25rem;
}

.confirm-buttons {
  display: flex;
  gap: 0.75rem;
}

.confirm-btn {
  flex: 1;
  padding: 0.75rem;
  border-radius: 0.5rem;
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.confirm-btn.cancel {
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-muted);
}

.confirm-btn.cancel:hover {
  background: var(--mobile-bg-hover);
  color: var(--mobile-text-primary);
}

.confirm-btn.confirm {
  background: #ef4444;
  border: none;
  color: #ffffff;
}

.confirm-btn.confirm:hover {
  background: #dc2626;
}

/* Modal Transition */
.modal-enter-active,
.modal-leave-active {
  transition: opacity 0.2s ease;
}

.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}

.modal-enter-active .settings-modal,
.modal-leave-active .settings-modal,
.modal-enter-active .confirm-modal,
.modal-leave-active .confirm-modal {
  transition: transform 0.2s ease;
}

.modal-enter-from .settings-modal,
.modal-leave-to .settings-modal,
.modal-enter-from .confirm-modal,
.modal-leave-to .confirm-modal {
  transform: scale(0.95);
}

/* Terminal Area - 始终占满 main-content，不被 sidebar 挤压 */
.terminal-output-area {
  position: absolute;
  inset: 0;
  overflow: hidden;
  background: var(--mobile-terminal-bg);
  /* 允许子元素触摸滚动 */
  touch-action: pan-y;
}

.xterm-container {
  height: 100%;
  width: 100%;
  position: relative;   /* 关键：让 xterm viewport 定位正确 */
  overflow: hidden;     /* 防止外部出现多余滚动条 */
  /* 允许子元素触摸滚动 */
  touch-action: pan-y;
}

/* xterm 滚动条样式 - 设置滚动条外观和触摸滚动 */
:deep(.xterm) {
  /* 确保 xterm 主容器不阻止触摸事件 */
  touch-action: pan-y;
  /* 禁止输入焦点 */
  user-select: none;
  -webkit-user-select: none;
}

:deep(.xterm-screen) {
  /* 屏幕元素不阻止触摸 */
  touch-action: pan-y;
  /* 禁止选择文本 */
  user-select: none;
  -webkit-user-select: none;
}

:deep(.xterm-viewport) {
  /* 启用触摸滚动 - 关键修复 */
  overflow-y: auto !important;
  touch-action: pan-y !important;
  -webkit-overflow-scrolling: touch;

  /* Firefox 滚动条样式 */
  scrollbar-width: thin;
  scrollbar-color: rgba(100, 100, 120, 0.3) transparent;
}

/* Webkit 滚动条样式 */
:deep(.xterm-viewport::-webkit-scrollbar) {
  width: 6px;
}

:deep(.xterm-viewport::-webkit-scrollbar-track) {
  background: transparent;
}

:deep(.xterm-viewport::-webkit-scrollbar-thumb) {
  background: rgba(100, 100, 120, 0.3);
  border-radius: 3px;
  transition: background 0.2s ease;
}

:deep(.xterm-viewport::-webkit-scrollbar-thumb:hover) {
  background: rgba(0, 212, 255, 0.4);
}
</style>