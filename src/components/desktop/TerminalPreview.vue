<template>
  <div class="h-full flex flex-col bg-dark-900">
    <!-- Header -->
    <header class="bg-dark-800 border-b border-dark-700 px-4 py-3 flex items-center justify-between">
      <div class="flex items-center gap-3">
        <div
          :class="[
            'w-2 h-2 rounded-full',
            statusColor
          ]"
        ></div>
        <h3 class="font-medium">{{ session?.name || '终端' }}</h3>
      </div>

      <div class="flex items-center gap-2">
        <!-- Font Size -->
        <select
          v-model="fontSize"
          class="bg-dark-700 border border-dark-600 rounded px-2 py-1 text-sm text-white"
        >
          <option v-for="size in [12, 14, 16, 18, 20]" :key="size" :value="size">
            {{ size }}px
          </option>
        </select>

        <!-- Clear Button -->
        <Button variant="ghost" size="sm" @click="clearTerminal">
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </Button>
      </div>
    </header>

    <!-- Terminal Container (xterm.js) -->
    <div ref="terminalContainerRef" class="flex-1 overflow-hidden"></div>

    <!-- Input Bar -->
    <div v-if="showInput && session" class="border-t border-dark-700 p-3 bg-dark-800">
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
        <Button variant="primary" @click="sendInput">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8" />
          </svg>
        </Button>
      </div>

      <!-- Quick Keys -->
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
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from 'vue'
import type { SessionInfo } from '@/stores/session'
import { useSessionStore } from '@/stores/session'
import { useSettingsStore } from '@/stores/settings'
import Button from '@/components/common/Button.vue'
import { usePtyOutput } from '@/composables/useTauri'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import '@xterm/xterm/css/xterm.css'

interface Props {
  session?: SessionInfo | null
  showInput?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  showInput: true,
})

const sessionStore = useSessionStore()
const settingsStore = useSettingsStore()
const terminalContainerRef = ref<HTMLElement | null>(null)
const fontSize = ref(settingsStore.settings.ui.terminal_font_size)
const inputText = ref('')
const historyIndex = ref(-1)

// xterm.js 实例
let terminal: Terminal | null = null
let fitAddon: FitAddon | null = null
let lastOutputIndex = 0 // 记录上次处理的输出索引

const sessionId = computed(() => props.session?.id || '')

const { output, clearOutput } = usePtyOutput(sessionId)

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
  if (!props.session) return 'bg-dark-500'

  switch (props.session.status) {
    case 'running':
      return 'bg-green-500'
    case 'waitingInput':
      return 'bg-yellow-500 animate-pulse'
    case 'error':
      return 'bg-red-500'
    case 'stopped':
      return 'bg-dark-500'
    case 'starting':
      return 'bg-blue-500 animate-pulse'
    default:
      return 'bg-dark-500'
  }
})

const inputHistory = ref<string[]>([])

// 初始化 xterm.js
function initTerminal() {
  if (!terminalContainerRef.value) return

  terminal = new Terminal({
    fontSize: fontSize.value,
    fontFamily: 'Consolas, Monaco, Courier New, monospace',
    theme: {
      background: '#1a1a2e',
      foreground: '#e0e0e0',
      cursor: '#ffffff',
      cursorAccent: '#1a1a2e',
      selectionBackground: '#4a4a6a',
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

  // 监听窗口大小变化
  const resizeObserver = new ResizeObserver(() => {
    if (fitAddon && terminal) {
      fitAddon.fit()
    }
  })
  resizeObserver.observe(terminalContainerRef.value)
}

// 写入输出到终端
function writeToTerminal(data: string) {
  if (!terminal) return
  terminal.write(data)
}

// 清空终端
function clearTerminal() {
  if (!terminal) return
  terminal.clear()
  clearOutput()
  lastOutputIndex = 0
}

// 监听 PTY 输出，写入 xterm（只处理新增的）
watch(output, (newOutput) => {
  if (!terminal) return

  // 只写入新增的输出（从上次索引之后）
  for (let i = lastOutputIndex; i < newOutput.length; i++) {
    terminal.write(newOutput[i])
  }
  lastOutputIndex = newOutput.length
}, { deep: true })

let fontSizeSaveTimeout: ReturnType<typeof setTimeout> | null = null
// 监听字体大小变化
watch(fontSize, (newSize) => {
  if (!terminal) return
  terminal.options.fontSize = newSize
  if (fitAddon) {
    fitAddon.fit()
  }
  // 持久化到设置（带去抖）
  if (fontSizeSaveTimeout) clearTimeout(fontSizeSaveTimeout)
  fontSizeSaveTimeout = setTimeout(() => {
    settingsStore.saveSettings({
      ui: { ...settingsStore.settings.ui, terminal_font_size: newSize }
    })
  }, 300)
})

// 监听会话变化，重置终端
watch(sessionId, (newId, oldId) => {
  if (newId !== oldId && oldId) {
    console.log('[Terminal] Session changed, clearing terminal')
    clearTerminal()
    lastOutputIndex = 0
  }
})

onMounted(() => {
  nextTick(() => {
    initTerminal()
  })
})

onUnmounted(() => {
  if (terminal) {
    terminal.dispose()
    terminal = null
  }
})

async function sendInput() {
  if (!inputText.value.trim() || !props.session) return

  const text = inputText.value
  console.log('[Terminal] Sending input:', text, 'to session:', props.session.id)

  inputHistory.value.push(text)
  historyIndex.value = -1

  // 写入终端显示用户输入
  if (terminal) {
    terminal.write(text + '\n')
  }

  // 发送到 PTY
  await sessionStore.writeToSession(props.session.id, text + '\n')

  inputText.value = ''
}

async function sendSpecialKey(key: string) {
  if (!props.session) return

  console.log('[Terminal] Sending special key:', key, 'to session:', props.session.id)

  if (key === 'tab') {
    inputText.value += '\t'
  } else {
    await sessionStore.sendSpecialKey(props.session.id, key)
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
</script>

<style scoped>
/* xterm.js 容器样式 */
:deep(.xterm) {
  height: 100%;
  padding: 8px;
}

:deep(.xterm-viewport) {
  border-radius: 0;
}
</style>