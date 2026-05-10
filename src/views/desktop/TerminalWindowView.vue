<template>
  <div class="h-screen flex flex-col bg-dark-900">
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
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import '@xterm/xterm/css/xterm.css'

const route = useRoute()
const sessionId = computed(() => route.params.id as string)
const sessionStatus = ref<'running' | 'stopped' | 'error' | 'waitingInput'>('running')
const terminalContainerRef = ref<HTMLElement | null>(null)
const inputText = ref('')

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

const inputHistory = ref<string[]>([])
const historyIndex = ref(-1)

async function loadSessionInfo() {
  try {
    const session = await invoke<{ status: string }>('get_session', { sessionId: sessionId.value })
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

  syncTerminalSize()

  terminal.onResize(({ cols, rows }) => {
    invoke('resize_session', { sessionId: sessionId.value, cols, rows }).catch(console.error)
  })

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
  const cols = terminal.cols
  const rows = terminal.rows
  if (cols > 0 && rows > 0) {
    invoke('resize_session', { sessionId: sessionId.value, cols, rows }).catch(console.error)
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