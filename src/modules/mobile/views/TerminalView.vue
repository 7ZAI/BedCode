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
      <button class="settings-btn" @click="openSettings" title="设置">
        <svg width="20" height="20" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
        </svg>
      </button>
    </header>

    <!-- Terminal Output Area - 禁止输入焦点 -->
    <div class="terminal-output-area" @click.prevent.stop>
      <div
        ref="xtermContainer"
        class="xterm-container"
        @touchstart="onContainerTouchStart"
        @touchmove="onContainerTouchMove"
        @click.prevent.stop
      ></div>
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
import { ref, computed, onMounted, onUnmounted, watch, nextTick } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
// import { WebglAddon } from '@xterm/addon-webgl'  // 暂时不使用，可能影响滚动
import '@xterm/xterm/css/xterm.css'  // 必须引入 xterm 样式
// FIXME: 真实 WebSocket 命令暂时注释
// import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useMobileConnection } from '@/modules/mobile/composables/useMobileConnection'
// FIXME: 真实 WebSocket 命令暂时注释
// import {
//   wsJoinSession,
//   wsLeaveSession,
//   wsSendInput,
//   wsSubscribeTerminal,
//   wsUnsubscribeTerminal,
//   wsGetTerminalIncremental,
//   wsUpdateTerminalIndex,
//   wsClearTerminalBuffer,
//   wsResizeTerminal,
// } from '@/modules/mobile/composables/useMobileCommands'
import { useOrientation } from '@/modules/mobile/composables/useOrientation'
import { useEdgeToEdge } from '@/modules/mobile/composables/useEdgeToEdge'
import TerminalInputBar from '@/modules/mobile/components/TerminalInputBar.vue'
import { useToast } from '@/modules/shared/composables/useToast'

// ==================== Props & Route ====================

const router = useRouter()
const route = useRoute()
const connection = useMobileConnection()
const toast = useToast()
const { isLandscape } = useOrientation()
const { safeArea, keyboardInfo } = useEdgeToEdge()

const sessionId = computed(() => route.params.id as string)

// ==================== State ====================

const xtermContainer = ref<HTMLDivElement | null>(null)
let terminal: Terminal | null = null
let fitAddon: FitAddon | null = null
let resizeObserver: ResizeObserver | null = null
// FIXME: 真实输出监听暂时注释
// let outputListener: UnlistenFn | null = null
// let outputPollInterval: ReturnType<typeof setInterval> | null = null
// let currentIndex = 0
let mockOutputInterval: ReturnType<typeof setInterval> | null = null

// 设置相关状态
const showSettings = ref(false)
const showClearConfirm = ref(false)
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
  if (!terminal) return

  const theme = TERMINAL_THEMES[terminalSettings.value.theme]

  // 单独设置每个属性，避免覆盖整个 options 对象
  terminal.options.theme = theme
  terminal.options.fontSize = terminalSettings.value.fontSize

  // 重新 fit 终端
  setTimeout(() => fitTerminal(), 50)
}

// ==================== Computed ====================

const isConnected = computed(() =>
  connection.connectionStatus.value === 'connected' ||
  connection.connectionStatus.value === 'paired'
)

const isDebugSession = computed(() => sessionId.value === 'mock-debug-session')

const session = computed(() =>
  connection.activeSessions.value.find(s => s.id === sessionId.value)
)

const sessionName = computed(() => {
  if (isDebugSession.value) return '调试终端 (模拟)'
  return session.value?.name || sessionId.value || '终端'
})

const sessionStatus = computed(() => {
  if (isDebugSession.value) return 'running'
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

// 键盘高度（用于避让）
const keyboardHeight = computed(() => keyboardInfo.value.keyboardHeight || 0)
const isKeyboardVisible = computed(() => keyboardInfo.value.isVisible)

// 安全区域
const safeAreaTop = computed(() => safeArea.value.top || 0)
const safeAreaBottom = computed(() => safeArea.value.bottom || 0)

// 终端视图样式：应用安全区域和键盘避让
const terminalViewStyle = computed(() => ({
  paddingTop: `${safeAreaTop.value}px`,
  paddingBottom: isKeyboardVisible.value ? `${keyboardHeight.value}px` : `${safeAreaBottom.value}px`,
}))

// 监听键盘变化，重新 fit 终端
watch(keyboardHeight, () => {
  setTimeout(() => fitTerminal(), 100)
})

// ==================== Terminal Setup ====================

async function initTerminal() {
  console.log('[TerminalView] initTerminal called, xtermContainer:', xtermContainer.value)

  if (!xtermContainer.value) {
    console.error('[TerminalView] xtermContainer is null!')
    return
  }

  console.log('[TerminalView] Container dimensions:', xtermContainer.value.offsetWidth, 'x', xtermContainer.value.offsetHeight)

  const theme = TERMINAL_THEMES[terminalSettings.value.theme]
  terminal = new Terminal({
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

  terminal.open(xtermContainer.value)
  console.log('[TerminalView] Terminal opened')

  // Load addons
  fitAddon = new FitAddon()
  terminal.loadAddon(fitAddon)
  terminal.loadAddon(new WebLinksAddon())

  // WebGL renderer - 提升渲染性能
  try {
    const { WebglAddon } = await import('@xterm/addon-webgl')
    const webglAddon = new WebglAddon()
    terminal.loadAddon(webglAddon)
    webglAddon.onContextLoss(() => {
      console.warn('[TerminalView] WebGL context lost')
    })
    console.log('[TerminalView] WebGL renderer loaded')
  } catch (e) {
    console.warn('[TerminalView] WebGL not supported, using DOM renderer:', e)
  }

  // Welcome message
  console.log('[TerminalView] Writing welcome message, isDebugSession:', isDebugSession.value)
  if (isDebugSession.value) {
    terminal.write('\x1b[36m[调试终端]\x1b[0m 模拟模式，无真实连接\r\n')
    terminal.write('输入命令将显示模拟输出\r\n')
    terminal.write('='.repeat(50) + '\r\n\r\n')
  } else {
    terminal.write('\x1b[36m[终端]\x1b[0m ' + sessionName.value + '\r\n')
    terminal.write('='.repeat(50) + '\r\n\r\n')
  }

  // Fit terminal - delay to ensure container is rendered
  setTimeout(() => {
    console.log('[TerminalView] Delayed fit, container dimensions:', xtermContainer.value?.offsetWidth, 'x', xtermContainer.value?.offsetHeight)
    fitTerminal()

    // 调试：检查 xterm 内部结构
    const xtermElement = xtermContainer.value?.querySelector('.xterm') as HTMLElement
    const viewport = xtermContainer.value?.querySelector('.xterm-viewport') as HTMLElement
    const screenElement = xtermContainer.value?.querySelector('.xterm-screen') as HTMLElement
    const helper = xtermContainer.value?.querySelector('.xterm-helpers') as HTMLElement

    console.log('[TerminalView] xterm structure:', {
      xterm: !!xtermElement,
      viewport: !!viewport,
      screen: !!screenElement,
      helper: !!helper,
    })

    if (viewport) {
      console.log('[TerminalView] Viewport found:', {
        height: viewport.style.height,
        overflowY: getComputedStyle(viewport).overflowY,
        scrollHeight: viewport.scrollHeight,
        clientHeight: viewport.clientHeight,
      })

      // 关键修复：确保 viewport 支持触摸滚动
      viewport.style.touchAction = 'pan-y'
      viewport.style.overflowY = 'auto'

      // 添加触摸事件监听调试
      viewport.addEventListener('touchstart', (e) => {
        console.log('[TerminalView] Touch start on viewport, touches:', e.touches.length)
      }, { passive: true })

      viewport.addEventListener('touchmove', (e) => {
        console.log('[TerminalView] Touch move on viewport, deltaY:', e.touches[0]?.clientY)
      }, { passive: true })

      // 添加滚轮事件监听调试
      viewport.addEventListener('wheel', (e) => {
        console.log('[TerminalView] Wheel event on viewport:', e.deltaY)
      }, { passive: true })
    } else {
      console.error('[TerminalView] Viewport not found!')
    }

    // 确保 xterm 主元素不阻止触摸
    if (xtermElement) {
      xtermElement.style.touchAction = 'pan-y'
      console.log('[TerminalView] Set touch-action on .xterm')

      // 尝试在 xterm 主元素上监听滚轮
      xtermElement.addEventListener('wheel', (e) => {
        console.log('[TerminalView] Wheel on .xterm:', e.deltaY)
        // 尝试手动触发 xterm 滚动
        if (terminal) {
          const scrollAmount = Math.round(e.deltaY / 20)
          terminal.scrollLines(scrollAmount)
          console.log('[TerminalView] Manually scrolled:', scrollAmount)
        }
      }, { passive: true })
    }

    // 确保屏幕元素不阻止触摸
    if (screenElement) {
      screenElement.style.touchAction = 'pan-y'
      console.log('[TerminalView] Set touch-action on .xterm-screen')
    }
  }, 100)

  // Resize observer
  resizeObserver = new ResizeObserver(() => {
    requestAnimationFrame(fitTerminal)
  })
  resizeObserver.observe(xtermContainer.value)

  // Window resize
  window.addEventListener('resize', handleWindowResize)

  // FIXME: Terminal resize 事件暂时注释
  // terminal.onResize(({ cols, rows }) => {
  //   if (isConnected.value && isSessionActive.value && !isDebugSession.value) {
  //     wsResizeTerminal(sessionId.value, cols, rows).catch((e: Error) => {
  //       console.error('[TerminalView] Resize failed:', e)
  //     })
  //   }
  // })
}

function fitTerminal() {
  if (!fitAddon || !terminal) return
  try {
    fitAddon.fit()
  } catch (e) {
    console.warn('[TerminalView] fit failed:', e)
  }
}

function handleWindowResize() {
  setTimeout(fitTerminal, 100)
}

function disposeTerminal() {
  if (resizeObserver) {
    resizeObserver.disconnect()
    resizeObserver = null
  }
  window.removeEventListener('resize', handleWindowResize)
  if (mockOutputInterval) {
    clearInterval(mockOutputInterval)
    mockOutputInterval = null
  }
  // FIXME: 真实输出监听清理暂时注释
  // if (outputPollInterval) {
  //   clearInterval(outputPollInterval)
  //   outputPollInterval = null
  // }
  // if (outputListener) {
  //   outputListener()
  //   outputListener = null
  // }
  if (terminal) {
    terminal.dispose()
    terminal = null
    fitAddon = null
  }
}

// ==================== Mock Data ====================

const MOCK_COMMANDS = [
  'ls -la', 'cd /usr/local/bin', 'cat package.json', 'git status',
  'npm install', 'docker ps', 'curl -I https://api.example.com',
  'ps aux | grep node', 'mkdir -p src/components',
  'echo "Hello, World!"', 'python3 -m http.server 8080',
  'tail -f /var/log/syslog', 'find . -name "*.ts" -type f',
  'tar -czf archive.tar.gz ./dist', 'ssh user@remote-host',
]

const MOCK_OUTPUTS = [
  'total 128\r\n  drwxr-xr-x  12 user  staff   384 Jun 01 10:23 .\r\n  drwxr-xr-x   5 root   root   160 Jun 01 09:15 ..',
  '{\r\n  "name": "bedcode-app",\r\n  "version": "1.0.0"\r\n}',
  'On branch dev\r\n  modified:   src/components/Terminal.vue\r\n  no changes added',
  'CONTAINER ID   IMAGE          COMMAND\r\n  abc123def456   nginx:latest   "/docker"  2 hours ago',
  'HTTP/1.1 200 OK\r\n  Content-Type: application/json\r\n  {"status":"ok"}',
  'PID   USER   TIME   COMMAND\r\n  1234  root   0:05   node server.js',
  'Server running at http://localhost:8080/\r\n  Serving directory: /home/user/project',
  'Connection established to remote-host\r\n  Welcome to Ubuntu 22.04.4 LTS',
]

const MOCK_LOG_MESSAGES = [
  'Processing request...',
  'Connection established',
  'Data received',
  'Operation completed',
  'Syncing...',
  'Heartbeat received',
  'Task finished',
]

const MOCK_LEVELS = ['INFO', 'WARN', 'DEBUG', 'ERROR']
const MOCK_COLORS = ['\x1b[32m', '\x1b[33m', '\x1b[36m', '\x1b[35m', '\x1b[34m', '\x1b[37m']

function getRandomInt(min: number, max: number) {
  return Math.floor(Math.random() * (max - min + 1)) + min
}

function getRandomItem<T>(arr: T[]): T {
  return arr[getRandomInt(0, arr.length - 1)]
}

function getTimestamp() {
  return new Date().toISOString().slice(11, 19)
}

// Generate mock output lines
function generateMockOutput(): string {
  const lineCount = getRandomInt(5, 10)
  const lines: string[] = []

  for (let i = 0; i < lineCount; i++) {
    const type = getRandomInt(0, 3)

    switch (type) {
      case 0: // Command
        lines.push('$ ' + getRandomItem(MOCK_COMMANDS))
        break
      case 1: // Output
        lines.push(getRandomItem(MOCK_OUTPUTS))
        break
      case 2: { // Colored log
        const color = getRandomItem(MOCK_COLORS)
        const level = getRandomItem(MOCK_LEVELS)
        const msg = getRandomItem(MOCK_LOG_MESSAGES)
        lines.push(color + '[' + getTimestamp() + '] [' + level + '] ' + msg + '\x1b[0m')
        break
      }
      case 3: { // Progress bar
        const progress = getRandomInt(5, 30)
        const remaining = getRandomInt(0, 10)
        lines.push('[' + '='.repeat(progress) + ' '.repeat(remaining) + '] ' + getRandomInt(10, 100) + '%')
        break
      }
    }
  }

  return lines.join('\r\n') + '\r\n'
}

// ==================== Session Subscription ====================

async function subscribeSession() {
  console.log('[TerminalView] subscribeSession called, isDebugSession:', isDebugSession.value, 'isConnected:', isConnected.value)

  // FIXME: 真实会话订阅暂时注释，使用模拟输出
  // if (isDebugSession.value || !isConnected.value) return

  // try {
  //   // Join session
  //   await wsJoinSession(sessionId.value)
  //   console.log('[TerminalView] Joined session:', sessionId.value)

  //   // Subscribe terminal buffer
  //   currentIndex = await wsSubscribeTerminal(sessionId.value)
  //   console.log('[TerminalView] Subscribed terminal, initial index:', currentIndex)

  //   // Start polling for incremental output
  //   startOutputPoll()
  // } catch (e) {
  //   console.error('[TerminalView] Subscribe failed:', e)
  //   toast.error('订阅终端失败')
  // }

  // 使用模拟输出
  startMockOutput()
}

async function unsubscribeSession() {
  // 停止模拟输出
  if (mockOutputInterval) {
    clearInterval(mockOutputInterval)
    mockOutputInterval = null
  }

  // FIXME: 真实会话取消订阅暂时注释
  // if (isDebugSession.value || !isConnected.value) return

  // try {
  //   if (outputPollInterval) {
  //     clearInterval(outputPollInterval)
  //     outputPollInterval = null
  //   }
  //   await wsUnsubscribeTerminal(sessionId.value)
  //   await wsLeaveSession(sessionId.value)
  //   console.log('[TerminalView] Unsubscribed session:', sessionId.value)
  // } catch (e) {
  //   console.error('[TerminalView] Unsubscribe failed:', e)
  // }
}

function startMockOutput() {
  console.log('[TerminalView] startMockOutput called, terminal exists:', !!terminal, 'isSessionActive:', isSessionActive.value)

  // 立即输出一次
  if (terminal) {
    const output = generateMockOutput()
    terminal.write(output)
  }

  // 每 3 秒生成模拟输出
  mockOutputInterval = setInterval(() => {
    if (!terminal) {
      console.log('[TerminalView] mockOutput interval: terminal is null')
      return
    }
    if (!isSessionActive.value) {
      console.log('[TerminalView] mockOutput interval: session not active')
      return
    }

    const output = generateMockOutput()
    terminal.write(output)
  }, 3000)

  console.log('[TerminalView] mockOutputInterval started:', mockOutputInterval)
}

// function startOutputPoll() {
//   // Poll every 100ms for incremental output
//   outputPollInterval = setInterval(async () => {
//     if (!terminal || !isSessionActive.value) return

//     try {
//       const result = await wsGetTerminalIncremental(sessionId.value)
//       if (result && result.events && result.events.length > 0) {
//         // Write all event data to terminal
//         for (const event of result.events) {
//           terminal.write(event.data)
//         }
//         currentIndex = result.current_index
//         await wsUpdateTerminalIndex(sessionId.value, currentIndex)
//       }
//     } catch (e) {
//       console.error('[TerminalView] Get incremental failed:', e)
//     }
//   }, 100)
// }

// ==================== Input Handlers ====================

function handleInputSubmit(text: string) {
  if (!terminal) return

  const term = terminal  // Capture reference for closure

  // 所有输入都显示模拟输出
  term.write('$ ' + text + '\r\n')
  setTimeout(() => {
    const responses = [
      `Command not found: ${text}\r\n`,
      `Executing: ${text}\r\n  Done.\r\n`,
      'Error: Permission denied\r\n',
      `${text}: command executed successfully\r\n`,
    ]
    const randomResponse = responses[Math.floor(Math.random() * responses.length)]
    term.write(randomResponse)
  }, 300)

  // FIXME: 真实输入发送暂时注释
  // if (isConnected.value && isSessionActive.value) {
  //   wsSendInput(sessionId.value, text + '\n').catch(e => {
  //     console.error('[TerminalView] Send input failed:', e)
  //     toast.error('发送命令失败')
  //   })
  // }
}

function handleInputExecute(text: string) {
  if (!terminal) return

  const term = terminal  // Capture reference for closure

  // 所有执行都显示模拟结果
  term.write('$ ' + text + '\r\n')
  setTimeout(() => {
    term.write('\x1b[32m[执行结果]\x1b[0m\r\n')
    term.write('  命令: ' + text + '\r\n')
    term.write('  状态: \x1b[32m成功\x1b[0m\r\n')
    term.write('  耗时: ' + (Math.random() * 2).toFixed(3) + 's\r\n\r\n')
  }, 200)

  // FIXME: 真实输入发送暂时注释
  // if (isConnected.value && isSessionActive.value) {
  //   wsSendInput(sessionId.value, text + '\n').catch(e => {
  //     console.error('[TerminalView] Send input failed:', e)
  //     toast.error('发送命令失败')
  //   })
  // }
}

function handleSpecialKey(key: string) {
  // FIXME: 真实特殊键发送暂时注释
  // if (!isConnected.value || !isSessionActive.value || isDebugSession.value) return
  // wsSendInput(sessionId.value, '', key).catch(e => {
  //   console.error('[TerminalView] Send special key failed:', e)
  // })
}

// ==================== Clear Terminal ====================

function confirmClear() {
  showClearConfirm.value = true
}

async function clearTerminal() {
  if (!terminal) return

  terminal.clear()
  showClearConfirm.value = false

  // FIXME: 真实清屏操作暂时注释
  // if (!isDebugSession.value && isConnected.value) {
  //   try {
  //     await wsClearTerminalBuffer(sessionId.value)
  //     currentIndex = 0
  //     await wsUpdateTerminalIndex(sessionId.value, 0)
  //   } catch (e) {
  //     console.error('[TerminalView] Clear buffer failed:', e)
  //   }
  // }
}

// ==================== Navigation ====================

function handleBack() {
  router.back()
}

// ==================== Touch Debug ====================

let lastTouchY = 0

function onViewTouchStart(e: TouchEvent) {
  console.log('[TerminalView] Touch start on view, target:', (e.target as HTMLElement)?.className)
  lastTouchY = e.touches[0]?.clientY || 0
}

function onViewTouchMove(e: TouchEvent) {
  console.log('[TerminalView] Touch move on view')
  // 手动处理触摸滚动
  const currentY = e.touches[0]?.clientY || 0
  const deltaY = lastTouchY - currentY
  lastTouchY = currentY

  if (terminal && Math.abs(deltaY) > 1) {
    const scrollAmount = Math.round(deltaY / 10)
    terminal.scrollLines(scrollAmount)
  }
}

function onContainerTouchStart(e: TouchEvent) {
  // console.log('[TerminalView] Touch start on container, target:', (e.target as HTMLElement)?.className)
  lastTouchY = e.touches[0]?.clientY || 0
}

function onContainerTouchMove(e: TouchEvent) {
  // console.log('[TerminalView] Touch move on container')
  // 手动处理触摸滚动
  const currentY = e.touches[0]?.clientY || 0
  const deltaY = lastTouchY - currentY
  lastTouchY = currentY

  if (terminal && Math.abs(deltaY) > 1) {
    const scrollAmount = Math.round(deltaY / 10)
    terminal.scrollLines(scrollAmount)
  }
}

// ==================== Lifecycle ====================

onMounted(async () => {
  console.log('[TerminalView] onMounted, sessionId:', sessionId.value, 'isDebugSession:', isDebugSession.value, 'isSessionActive:', isSessionActive.value)

  await nextTick()
  initTerminal()

  // 所有会话都启动模拟输出（调试会话始终活动，真实会话检查状态）
  // 对于调试会话，isSessionActive 应该是 true
  // 但为了确保模拟输出能工作，我们直接启动
  startMockOutput()

  // FIXME: 真实输出监听暂时注释
  // Listen for output events (backup mechanism)
  // outputListener = await listen<{ session_id: string; data: string }>('ws_output', (event) => {
  //   if (event.payload.session_id === sessionId.value && terminal) {
  //     terminal.write(event.payload.data)
  //   }
  // })
})

onUnmounted(async () => {
  await unsubscribeSession()
  disposeTerminal()
})

// Watch session status changes
watch(isSessionActive, async (active, prevActive) => {
  if (active && !prevActive) {
    // Session became active
    await subscribeSession()
    if (terminal) {
      terminal.write('\x1b[32m[会话已启动]\x1b[0m\r\n')
    }
  } else if (!active && prevActive) {
    // Session stopped
    await unsubscribeSession()
    if (terminal) {
      terminal.write('\x1b[33m[会话已停止]\x1b[0m\r\n')
    }
  }
})

// Watch connection status changes
watch(isConnected, async (connected) => {
  if (!connected && terminal) {
    terminal.write('\x1b[31m[连接已断开]\x1b[0m\r\n')
    await unsubscribeSession()
  } else if (connected && isSessionActive.value && !isDebugSession.value && terminal) {
    terminal.write('\x1b[32m[连接已恢复]\x1b[0m\r\n')
    await subscribeSession()
  }
})
</script>

<style scoped>
.terminal-view {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: #0a0a0f;
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
}

/* Header */
.header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.75rem 1rem;
  background: rgba(18, 18, 26, 0.9);
  backdrop-filter: blur(20px);
  border-bottom: 1px solid rgba(0, 212, 255, 0.15);
  flex-shrink: 0;
  position: relative;
  z-index: 10;
}

.back-btn {
  padding: 0.5rem;
  margin-left: -0.5rem;
  color: var(--text-secondary, #9ca3af);
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
  color: #ffffff;
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
  color: var(--text-muted, #6b7280);
}

.clear-btn {
  padding: 0.5rem;
  border-radius: 0.5rem;
  background: #1f2937;
  border: 1px solid var(--border, #374151);
  color: var(--text-secondary, #9ca3af);
  cursor: pointer;
  transition: all 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.clear-btn:hover {
  border-color: rgba(0, 212, 255, 0.3);
}

.settings-btn {
  padding: 0.5rem;
  border-radius: 0.5rem;
  background: #1f2937;
  border: 1px solid var(--border, #374151);
  color: var(--text-secondary, #9ca3af);
  cursor: pointer;
  transition: all 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.settings-btn:hover {
  border-color: rgba(0, 212, 255, 0.3);
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
  background: #1a1a2e;
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
  border-bottom: 1px solid rgba(255, 255, 255, 0.1);
}

.settings-header h2 {
  font-size: 1rem;
  font-weight: 600;
  color: #ffffff;
  margin: 0;
}

.close-btn {
  padding: 0.25rem;
  background: none;
  border: none;
  color: #9ca3af;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}

.close-btn:hover {
  color: #ffffff;
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
  color: #9ca3af;
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
  background: #2d2d44;
  border: 1px solid rgba(255, 255, 255, 0.1);
  color: #ffffff;
  font-size: 1.25rem;
  cursor: pointer;
  transition: all 0.2s ease;
}

.size-btn:hover:not(:disabled) {
  background: #3d3d54;
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
  color: #ffffff;
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
  background: #2d2d44;
  border: 2px solid transparent;
  cursor: pointer;
  transition: all 0.2s ease;
}

.theme-btn:hover {
  background: #3d3d54;
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
  color: #9ca3af;
}

.theme-btn.active .theme-name {
  color: #00d4ff;
  font-weight: 600;
}

.settings-footer {
  display: flex;
  gap: 0.75rem;
  padding: 0.75rem 1rem;
  border-top: 1px solid rgba(255, 255, 255, 0.1);
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
  background: #2d2d44;
  border: 1px solid rgba(255, 255, 255, 0.1);
  color: #9ca3af;
}

.settings-footer-btn.cancel:hover {
  background: #3d3d54;
  color: #ffffff;
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
  background: #1a1a2e;
  border-radius: 1rem;
  padding: 1.5rem;
  width: 100%;
  max-width: 300px;
  text-align: center;
}

.confirm-text {
  font-size: 1rem;
  color: #ffffff;
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
  background: #2d2d44;
  border: 1px solid rgba(255, 255, 255, 0.1);
  color: #9ca3af;
}

.confirm-btn.cancel:hover {
  background: #3d3d54;
  color: #ffffff;
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

/* Terminal Area */
.terminal-output-area {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  position: relative;
  background: #0a0a0f;
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