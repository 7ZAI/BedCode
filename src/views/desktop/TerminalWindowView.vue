<template>
  <div class="h-screen flex flex-col bg-dark-900">
    <!-- Header -->
    <header class="bg-dark-800 border-b border-dark-700 px-4 py-3 flex items-center justify-between h-12 shrink-0" data-tauri-drag-region>
      <div class="flex items-center gap-3">
        <div :class="['w-2 h-2 rounded-full', statusColor]"></div>
        <h3 class="font-medium text-white">{{ sessionName }}</h3>
        <span class="text-xs text-dark-400">({{ sessionId }})</span>
      </div>
      <div class="flex items-center gap-2">
        <!-- Window Controls -->
        <button @click="minimizeWindow" class="p-1 hover:bg-dark-700 rounded">
          <svg class="w-4 h-4 text-dark-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 12H4" />
          </svg>
        </button>
        <button @click="toggleMaximize" class="p-1 hover:bg-dark-700 rounded">
          <svg class="w-4 h-4 text-dark-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path v-if="!isMaximized" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h4" />
            <path v-else stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 9V4.5M9 9H4.5M9 9L3.75 3.75M9 15v4.5M9 15H4.5M9 15l-5 5m5-5l5-5m-5 5v-4.5m0 4.5h4.5" />
          </svg>
        </button>
        <button @click="closeWindow" class="p-1 hover:bg-red-600 rounded">
          <svg class="w-4 h-4 text-dark-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
    </header>

    <!-- Terminal Container -->
    <div ref="terminalContainerRef" class="flex-1 overflow-hidden"></div>

    <!-- Input Bar -->
    <div v-if="sessionStatus === 'running'" class="border-t border-dark-700 p-3 bg-dark-800 shrink-0">
      <div class="flex gap-2">
        <input
          v-model="inputText"
          type="text"
          placeholder="输入命令..."
          class="flex-1 bg-dark-700 border border-dark-600 rounded-lg px-4 py-2 text-white placeholder-dark-400 focus:border-primary-500 outline-none"
          @keydown.enter="sendInput"
          @keydown.tab.prevent="sendSpecialKey('tab')"
          @keydown.up.prevent="navigateHistory(-1)"
          @keydown.down.prevent="navigateHistory(1)"
        />
        <button @click="sendInput" class="px-4 py-2 bg-primary-600 hover:bg-primary-500 text-white rounded-lg">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8" />
          </svg>
        </button>
      </div>
      <div class="flex gap-2 mt-2">
        <button
          v-for="key in quickKeys"
          :key="key.value"
          class="px-3 py-1 bg-dark-700 hover:bg-dark-600 rounded text-xs text-dark-300 transition-colors"
          @click="sendSpecialKey(key.value)"
        >
          {{ key.label }}
        </button>
      </div>
    </div>

    <!-- Session Ended State -->
    <div v-else class="border-t border-dark-700 p-4 bg-dark-800 text-center text-dark-400 shrink-0">
      会话已 {{ sessionStatus === 'stopped' ? '停止' : '出错' }}
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import { useRoute } from 'vue-router'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import '@xterm/xterm/css/xterm.css'

const route = useRoute()
const sessionId = computed(() => route.params.id as string)
const sessionName = ref('')
const sessionStatus = ref<'running' | 'stopped' | 'error' | 'waitingInput'>('running')
const terminalContainerRef = ref<HTMLElement | null>(null)
const inputText = ref('')
const isMaximized = ref(false)

let terminal: Terminal | null = null
let fitAddon: FitAddon | null = null
let outputListener: (() => void) | null = null

const quickKeys = [
  { label: 'Tab', value: 'tab' },
  { label: 'Enter', value: 'enter' },
  { label: 'Esc', value: 'escape' },
  { label: 'Ctrl+C', value: 'ctrl_c' },
  { label: 'Ctrl+D', value: 'ctrl_d' },
  { label: '↑', value: 'arrow_up' },
  { label: '↓', value: 'arrow_down' },
]

const statusColor = computed(() => {
  switch (sessionStatus.value) {
    case 'running': return 'bg-green-500'
    case 'waitingInput': return 'bg-yellow-500 animate-pulse'
    case 'error': return 'bg-red-500'
    case 'stopped': return 'bg-dark-500'
    default: return 'bg-dark-500'
  }
})

const inputHistory = ref<string[]>([])
const historyIndex = ref(-1)

async function loadSessionInfo() {
  try {
    const session = await invoke<{ name: string; status: string }>('get_session', { sessionId: sessionId.value })
    sessionName.value = session.name
    sessionStatus.value = session.status as 'running' | 'stopped' | 'error' | 'waitingInput'
  } catch (e) {
    console.error('[TerminalWindow] Failed to load session info:', e)
  }
}

async function setupPtyListener() {
  outputListener = await listen<{ sessionId: string; data: string }>('pty-output', (event) => {
    if (event.payload.sessionId === sessionId.value && terminal) {
      terminal.write(event.payload.data)
    }
  })
}

function initTerminal() {
  if (!terminalContainerRef.value) return

  terminal = new Terminal({
    fontSize: 14,
    fontFamily: 'Consolas, Monaco, Courier New, monospace',
    theme: {
      background: '#1a1a2e',
      foreground: '#e0e0e0',
      cursor: '#ffffff',
      cursorAccent: '#1a1a2e',
      selectionBackground: '#4a4a6a',
    },
    cursorBlink: true,
    cursorStyle: 'block',
    scrollback: 10000,
    allowProposedApi: true,
  })

  fitAddon = new FitAddon()
  terminal.loadAddon(fitAddon)
  terminal.loadAddon(new WebLinksAddon())
  terminal.open(terminalContainerRef.value)
  fitAddon.fit()

  // 同步终端尺寸到 PTY
  syncTerminalSize()

  // 监听终端尺寸变化
  terminal.onResize(({ cols, rows }) => {
    invoke('resize_session', { sessionId: sessionId.value, cols, rows }).catch(console.error)
  })

  // 监听窗口大小变化
  const resizeObserver = new ResizeObserver(() => {
    if (fitAddon && terminal) {
      fitAddon.fit()
      syncTerminalSize()
    }
  })
  resizeObserver.observe(terminalContainerRef.value)
}

function syncTerminalSize() {
  if (!terminal) return
  const col = terminal.cols
  const row = terminal.rows
  if (col > 0 && row > 0) {
    invoke('resize_session', { sessionId: sessionId.value, cols: col, rows: row }).catch(console.error)
  }
}

async function sendInput() {
  if (!inputText.value.trim()) return

  const text = inputText.value
  inputHistory.value.push(text)
  historyIndex.value = -1

  if (terminal) {
    terminal.write(text + '\r\n')
  }

  await invoke('write_to_session', { sessionId: sessionId.value, text: text + '\n' })
  inputText.value = ''
}

async function sendSpecialKey(key: string) {
  if (!terminal) return

  if (key === 'tab') {
    inputText.value += '\t'
  } else {
    await invoke('send_special_key', { sessionId: sessionId.value, key })
  }
}

function navigateHistory(direction: number) {
  if (inputHistory.value.length === 0) return

  const newIndex = historyIndex.value + direction
  if (newIndex < -1) return
  if (newIndex >= inputHistory.value.length) return

  historyIndex.value = newIndex
  if (newIndex === -1) {
    inputText.value = ''
  } else {
    inputText.value = inputHistory.value[inputHistory.value.length - 1 - newIndex]
  }
}

async function minimizeWindow() {
  const win = getCurrentWindow()
  await win.minimize()
}

async function toggleMaximize() {
  const win = getCurrentWindow()
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
  const win = getCurrentWindow()
  await win.close()
}

onMounted(async () => {
  await loadSessionInfo()
  await setupPtyListener()
  nextTick(() => {
    initTerminal()
  })
})

onUnmounted(() => {
  if (outputListener) outputListener()
  if (terminal) {
    terminal.dispose()
    terminal = null
  }
})
</script>

<style scoped>
:deep(.xterm) {
  height: 100%;
  padding: 8px;
}
:deep(.xterm-viewport) {
  border-radius: 0;
}
</style>