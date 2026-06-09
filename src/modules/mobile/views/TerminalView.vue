<template>
  <div class="terminal-view">
    <!-- Header -->
    <header class="header">
      <button class="back-btn" @click="handleBack">
        <svg width="20" height="20" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <div class="header-title-area">
        <h1 class="header-title">{{ sessionName }}</h1>
      </div>
      <div class="status-area">
        <div class="status-dot" :class="statusClass"></div>
        <span class="status-text">{{ statusText }}</span>
      </div>
      <button class="clear-btn" @click="clearTerminal" title="清屏">
        <svg width="20" height="20" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
        </svg>
      </button>
    </header>

    <!-- Terminal Output Area -->
    <div class="terminal-output-area">
      <div ref="xtermContainer" class="xterm-container"></div>
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
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch, nextTick } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { WebglAddon } from '@xterm/addon-webgl'
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
import TerminalInputBar from '@/modules/mobile/components/TerminalInputBar.vue'
import { useToast } from '@/modules/shared/composables/useToast'

// ==================== Props & Route ====================

const router = useRouter()
const route = useRoute()
const connection = useMobileConnection()
const toast = useToast()
const { isLandscape } = useOrientation()

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

// ==================== Terminal Theme ====================

const DARK_THEME = {
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
}

// ==================== Terminal Setup ====================

function initTerminal() {
  if (!xtermContainer.value) return

  terminal = new Terminal({
    theme: DARK_THEME,
    fontFamily: '"Courier New", Courier, "Lucida Console", monospace',
    fontSize: 14,
    lineHeight: 1.2,
    cursorBlink: true,
    cursorStyle: 'block',
    allowProposedApi: true,
    scrollback: 5000,
    convertEol: true,
  })

  terminal.open(xtermContainer.value)

  // Load addons
  fitAddon = new FitAddon()
  terminal.loadAddon(fitAddon)
  terminal.loadAddon(new WebLinksAddon())

  // Try WebGL renderer
  try {
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
  if (isDebugSession.value) {
    terminal.write('\x1b[36m[调试终端]\x1b[0m 模拟模式，无真实连接\r\n')
    terminal.write('输入命令将显示模拟输出\r\n')
    terminal.write('='.repeat(50) + '\r\n\r\n')
  } else {
    terminal.write('\x1b[36m[终端]\x1b[0m ' + sessionName.value + '\r\n')
    terminal.write('='.repeat(50) + '\r\n\r\n')
  }

  // Fit terminal
  fitTerminal()

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
  // 每 3 秒生成模拟输出
  mockOutputInterval = setInterval(() => {
    if (!terminal || !isSessionActive.value) return

    const output = generateMockOutput()
    terminal.write(output)
  }, 3000)
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

async function clearTerminal() {
  if (!terminal) return

  terminal.clear()

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

// ==================== Lifecycle ====================

onMounted(async () => {
  await nextTick()
  initTerminal()

  // 所有会话都启动模拟输出
  if (isSessionActive.value) {
    await subscribeSession()
  }

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
  height: 100%;
  background: var(--bg-primary, #0a0a0f);
}

/* Header */
.header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.75rem 1rem;
  background: rgba(18, 18, 26, 0.9);
  backdrop-filter: blur(20px);
  border-bottom: 1px solid var(--accent-border, rgba(0, 212, 255, 0.15));
  flex-shrink: 0;
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

/* Terminal Area */
.terminal-output-area {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  position: relative;
}

.xterm-container {
  height: 100%;
  width: 100%;
}

/* xterm scrollbar styling */
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

:deep(.xterm-viewport) {
  scrollbar-width: thin;
  scrollbar-color: rgba(100, 100, 120, 0.3) transparent;
}
</style>