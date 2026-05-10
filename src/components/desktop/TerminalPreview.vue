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

    <!-- Terminal Container (xterm.js) - 原生键盘输入 -->
    <div ref="terminalContainerRef" class="flex-1 overflow-hidden"></div>
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

// xterm.js 实例
let terminal: Terminal | null = null
let fitAddon: FitAddon | null = null

const sessionId = computed(() => props.session?.id || '')

const { output, clearOutput } = usePtyOutput(sessionId)

// 快速键（仅用于 UI 显示，实际功能已集成到 xterm 原生输入）
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

  // 将终端尺寸同步到 PTY，确保 Claude Code 输出格式正确
  syncTerminalSize()

  // 监听终端尺寸变化，同步到 PTY
  terminal.onResize(({ cols, rows }) => {
    if (props.session) {
      sessionStore.resizeSession(props.session.id, cols, rows)
    }
  })

  // 监听窗口大小变化
  const resizeObserver = new ResizeObserver(() => {
    if (fitAddon && terminal) {
      fitAddon.fit()
      syncTerminalSize()
    }
  })
  resizeObserver.observe(terminalContainerRef.value)

  // 捕获键盘输入，直接发送到 PTY（原生终端体验）
  terminal.onData((data: string) => {
    if (!props.session) return
    console.log('[Terminal] onData:', JSON.stringify(data))
    sessionStore.writeToSession(props.session.id, data)
  })
}

/** 将当前终端尺寸同步到 PTY */
function syncTerminalSize() {
  if (!terminal || !props.session) return
  const cols = terminal.cols
  const rows = terminal.rows
  if (cols > 0 && rows > 0) {
    sessionStore.resizeSession(props.session.id, cols, rows)
  }
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
}

// 监听 PTY 输出，写入 xterm
watch(output, (newOutput) => {
  if (!terminal) return
  // 写入所有新的输出
  for (const data of newOutput) {
    terminal.write(data)
  }
}, { deep: true })

let fontSizeSaveTimeout: ReturnType<typeof setTimeout> | null = null
// 监听本地 fontSize 变化并更新终端
watch(fontSize, (newSize) => {
  if (!terminal) return
  terminal.options.fontSize = newSize
  if (fitAddon) {
    fitAddon.fit()
  }
  nextTick(() => syncTerminalSize())
  if (fontSizeSaveTimeout) clearTimeout(fontSizeSaveTimeout)
  fontSizeSaveTimeout = setTimeout(() => {
    settingsStore.saveSettings({
      ui: { ...settingsStore.settings.ui, terminal_font_size: newSize }
    })
  }, 300)
})
// 监听设置中的字体大小变化（从设置页面加载时）
watch(() => settingsStore.settings.ui.terminal_font_size, (newSize, oldSize) => {
  console.log('[TerminalPreview] Store fontSize changed:', oldSize, '->', newSize)
  if (fontSize.value !== newSize) {
    fontSize.value = newSize
    if (terminal) {
      terminal.options.fontSize = newSize
      if (fitAddon) fitAddon.fit()
      nextTick(() => syncTerminalSize())
    }
  }
}, { immediate: true })

// 监听会话变化，重置终端并同步尺寸
watch(sessionId, (newId, oldId) => {
  if (newId !== oldId) {
    if (oldId) {
      clearTerminal()
    }
    // 新会话激活时同步当前终端尺寸到 PTY
    if (newId && terminal) {
      nextTick(() => syncTerminalSize())
    }
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

/**
 * 发送特殊键（通过快速键按钮触发）
 * 注意：xterm.js 的 onData 已处理普通键盘输入，此方法仅用于 UI 按钮
 */
async function sendSpecialKey(key: string) {
  if (!props.session) return

  console.log('[Terminal] Sending special key:', key, 'to session:', props.session.id)
  await sessionStore.sendSpecialKey(props.session.id, key)
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