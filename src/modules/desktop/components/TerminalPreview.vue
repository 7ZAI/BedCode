<template>
  <div class="h-full flex flex-col bg-gray-100 dark:bg-dark-900">
    <!-- Header -->
    <header class="px-4 py-3 flex items-center justify-between border-b border-gray-200 dark:border-dark-700 bg-white dark:bg-dark-800">
      <div class="flex items-center gap-3">
        <div
          :class="[
            'w-2 h-2 rounded-full',
            statusColor
          ]"
        ></div>
        <h3 class="font-medium text-gray-900 dark:text-white">{{ session?.name || '终端' }}</h3>
      </div>

      <div class="flex items-center gap-2">
        <!-- Theme Switch -->
        <select
          v-model="terminalTheme"
          class="bg-gray-100 dark:bg-dark-700 border border-gray-200 dark:border-dark-600 rounded px-2 py-1 text-sm text-gray-700 dark:text-white"
          title="终端主题"
        >
          <option v-for="(name, key) in themeNames" :key="key" :value="key">
            {{ name }}
          </option>
        </select>

        <!-- Font Size -->
        <select
          v-model="fontSize"
          class="bg-gray-100 dark:bg-dark-700 border border-gray-200 dark:border-dark-600 rounded px-2 py-1 text-sm text-gray-700 dark:text-white"
          title="字体大小"
        >
          <option v-for="size in [12, 14, 16, 18, 20]" :key="size" :value="size">
            {{ size }}px
          </option>
        </select>

        <!-- Clear Button -->
        <Button variant="ghost" size="sm" @click="clearTerminal" title="清屏">
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </Button>
      </div>
    </header>

    <!-- Terminal Container (xterm.js) -->
    <div ref="terminalContainerRef" class="flex-1 overflow-hidden"></div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from 'vue'
import type { SessionInfo } from '@/modules/shared/stores/session'
import { useSessionStore } from '@/modules/shared/stores/session'
import { useSettingsStore } from '@/modules/shared/stores/settings'
import Button from '@/modules/shared/components/Button.vue'
import { usePtyOutput } from '@/modules/desktop/composables/usePtyOutput'
import {
  useTerminalHistory,
  initSessionCache,
  destroySessionCache
} from '@/modules/desktop/composables/useGlobalTerminal'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { WebglAddon } from '@xterm/addon-webgl'
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
const terminalTheme = ref<string>('dracula')

// xterm.js 实例（组件内）
let terminal: Terminal | null = null
let fitAddon: FitAddon | null = null
let webglAddon: WebglAddon | null = null
let resizeObserver: ResizeObserver | null = null

// 滚动状态追踪
let isUserScrolling = false
let scrollTimeout: ReturnType<typeof setTimeout> | null = null

const sessionId = computed(() => props.session?.id || '')

// PTY 输出监听（组件内）
const { output: realtimeOutput, clearOutput } = usePtyOutput(sessionId)

// 终端历史缓存
const terminalHistory = useTerminalHistory(sessionId.value)

const statusColor = computed(() => {
  if (!props.session) return 'bg-gray-400 dark:bg-dark-500'

  switch (props.session.status) {
    case 'running':
      return 'bg-green-500'
    case 'waitingInput':
      return 'bg-yellow-500 animate-pulse'
    case 'error':
      return 'bg-red-500'
    case 'stopped':
      return 'bg-gray-400 dark:bg-dark-500'
    case 'starting':
      return 'bg-blue-500 animate-pulse'
    default:
      return 'bg-gray-400 dark:bg-dark-500'
  }
})

// 终端主题集合
const terminalThemes: Record<string, object> = {
  default: {
    background: '#000000',
    foreground: '#ffffff',
    cursor: '#ffffff',
    cursorAccent: '#000000',
    selectionBackground: '#4d4d4d',
    black: '#000000',
    red: '#cd0000',
    green: '#00cd00',
    yellow: '#cdcd00',
    blue: '#0000ee',
    magenta: '#cd00cd',
    cyan: '#00cdcd',
    white: '#e5e5e5',
    brightBlack: '#7f7f7f',
    brightRed: '#ff0000',
    brightGreen: '#00ff00',
    brightYellow: '#ffff00',
    brightBlue: '#5c5cff',
    brightMagenta: '#ff00ff',
    brightCyan: '#00ffff',
    brightWhite: '#ffffff',
  },
  dracula: {
    background: '#1e1e2e',
    foreground: '#f8f8f2',
    cursor: '#f8f8f2',
    cursorAccent: '#1e1e2e',
    selectionBackground: '#44475a',
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
  oneDark: {
    background: '#282c34',
    foreground: '#abb2bf',
    cursor: '#528bff',
    cursorAccent: '#282c34',
    selectionBackground: '#3e4451',
    black: '#282c34',
    red: '#e06c75',
    green: '#98c379',
    yellow: '#e5c07b',
    blue: '#61afef',
    magenta: '#c678dd',
    cyan: '#56b6c2',
    white: '#abb2bf',
    brightBlack: '#545862',
    brightRed: '#e06c75',
    brightGreen: '#98c379',
    brightYellow: '#e5c07b',
    brightBlue: '#61afef',
    brightMagenta: '#c678dd',
    brightCyan: '#56b6c2',
    brightWhite: '#ffffff',
  },
  solarizedDark: {
    background: '#002b36',
    foreground: '#839496',
    cursor: '#839496',
    cursorAccent: '#002b36',
    selectionBackground: '#073642',
    black: '#073642',
    red: '#dc322f',
    green: '#859900',
    yellow: '#b58900',
    blue: '#268bd2',
    magenta: '#d33682',
    cyan: '#2aa198',
    white: '#eee8d5',
    brightBlack: '#002b36',
    brightRed: '#cb4b16',
    brightGreen: '#586e75',
    brightYellow: '#657b83',
    brightBlue: '#839496',
    brightMagenta: '#6c71c4',
    brightCyan: '#93a1a1',
    brightWhite: '#fdf6e3',
  },
  solarizedLight: {
    background: '#fdf6e3',
    foreground: '#657b83',
    cursor: '#657b83',
    cursorAccent: '#fdf6e3',
    selectionBackground: '#eee8d5',
    black: '#073642',
    red: '#dc322f',
    green: '#859900',
    yellow: '#b58900',
    blue: '#268bd2',
    magenta: '#d33682',
    cyan: '#2aa198',
    white: '#eee8d5',
    brightBlack: '#002b36',
    brightRed: '#cb4b16',
    brightGreen: '#586e75',
    brightYellow: '#657b83',
    brightBlue: '#839496',
    brightMagenta: '#6c71c4',
    brightCyan: '#93a1a1',
    brightWhite: '#fdf6e3',
  },
  ubuntu: {
    background: '#300a24',
    foreground: '#cccccc',
    cursor: '#cccccc',
    cursorAccent: '#300a24',
    selectionBackground: '#5a3a72',
    black: '#300a24',
    red: '#e95420',
    green: '#3eb33f',
    yellow: '#ffb73b',
    blue: '#77216f',
    magenta: '#c748ba',
    cyan: '#23c7c7',
    white: '#cccccc',
    brightBlack: '#300a24',
    brightRed: '#e95420',
    brightGreen: '#3eb33f',
    brightYellow: '#ffb73b',
    brightBlue: '#77216f',
    brightMagenta: '#c748ba',
    brightCyan: '#23c7c7',
    brightWhite: '#ffffff',
  },
}

const themeNames: Record<string, string> = {
  default: 'Default',
  dracula: 'Dracula',
  oneDark: 'One Dark',
  solarizedDark: 'Solarized Dark',
  solarizedLight: 'Solarized Light',
  ubuntu: 'Ubuntu',
}

function getTheme() {
  return terminalThemes[terminalTheme.value] || terminalThemes.default
}

function initWebGL(terminal: Terminal): boolean {
  try {
    webglAddon = new WebglAddon()
    webglAddon.onContextLoss(() => {
      console.warn('[TerminalPreview] WebGL context lost')
      webglAddon?.dispose()
      webglAddon = null
    })
    terminal.loadAddon(webglAddon)
    return true
  } catch (e) {
    console.warn('[TerminalPreview] WebGL not supported:', e)
    webglAddon = null
    return false
  }
}

function initTerminal() {
  if (!terminalContainerRef.value) return

  terminal = new Terminal({
    fontSize: fontSize.value,
    fontFamily: 'Consolas, Monaco, Courier New, monospace',
    theme: getTheme(),
    cursorBlink: false,
    cursorStyle: 'block',
    cursorWidth: 1,
    scrollback: 50000,
    allowProposedApi: true,
  })

  fitAddon = new FitAddon()
  terminal.loadAddon(fitAddon)
  terminal.loadAddon(new WebLinksAddon())
  terminal.open(terminalContainerRef.value)
  initWebGL(terminal)
  fitAddon.fit()

  syncTerminalSize()

  terminal.onResize(({ cols, rows }) => {
    if (props.session) {
      sessionStore.resizeSession(props.session.id, cols, rows)
    }
  })

  // ResizeObserver
  let lastCols = 0
  let lastRows = 0
  let lastContainerWidth = 0
  let lastContainerHeight = 0
  resizeObserver = new ResizeObserver((entries) => {
    if (!fitAddon || !terminal) return

    const entry = entries[0]
    if (!entry) return

    const newWidth = Math.round(entry.contentRect.width)
    const newHeight = Math.round(entry.contentRect.height)

    if (newWidth === lastContainerWidth && newHeight === lastContainerHeight) {
      return
    }

    lastContainerWidth = newWidth
    lastContainerHeight = newHeight

    fitAddon.fit()
    const newCols = terminal.cols
    const newRows = terminal.rows

    const colsChanged = Math.abs(newCols - lastCols) > lastCols * 0.1
    const rowsChanged = Math.abs(newRows - lastRows) > 5

    if ((colsChanged || rowsChanged) && lastCols > 0 && lastRows > 0) {
      syncTerminalSize()
      refreshTerminal()
    } else {
      syncTerminalSize()
    }

    lastCols = newCols
    lastRows = newRows
  })
  resizeObserver.observe(terminalContainerRef.value)

  // 键盘输入
  terminal.onData((data: string) => {
    if (!props.session) return
    sessionStore.writeToSession(props.session.id, data)
  })
}

function syncTerminalSize() {
  if (!terminal || !props.session) return
  const cols = terminal.cols
  const rows = terminal.rows
  if (cols > 0 && rows > 0) {
    sessionStore.resizeSession(props.session.id, cols, rows)
  }
}

function refreshTerminal() {
  if (!terminal || !props.session) return
  terminal.clear()
  terminal.write('\x1b[2J\x1b[H')
  scrollToBottom()
}

function scrollToBottom() {
  if (!terminal) return
  const viewport = terminalContainerRef.value?.querySelector('.xterm-viewport') as HTMLElement
  if (viewport) {
    viewport.scrollTop = viewport.scrollHeight
  }
}

function handleScroll() {
  const viewport = terminalContainerRef.value?.querySelector('.xterm-viewport') as HTMLElement
  if (!viewport) return

  const isAtBottom = viewport.scrollHeight - viewport.scrollTop <= viewport.clientHeight + 50
  isUserScrolling = !isAtBottom

  if (scrollTimeout) clearTimeout(scrollTimeout)
  scrollTimeout = setTimeout(() => {
    isUserScrolling = false
  }, 300)
}

function clearTerminal() {
  if (!terminal) return
  terminal.clear()
  clearOutput()
  terminalHistory.clear()
}

// 增量写入计数器
let lastOutputLength = 0

// 监听 PTY 输出：写入组件 xterm + 同步到全局缓存
watch(realtimeOutput, (newOutput) => {
  if (!terminal) return

  const newLength = newOutput.length
  if (newLength > lastOutputLength) {
    const newData = newOutput.slice(lastOutputLength)
    // 写入组件 xterm
    terminal.write(newData)
    // 同步到全局缓存
    terminalHistory.append(newData)
    lastOutputLength = newLength
  }

  if (!isUserScrolling) {
    scrollToBottom()
  }
}, { deep: true })

// 字体大小变化
let fontSizeSaveTimeout: ReturnType<typeof setTimeout> | null = null
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

watch(() => settingsStore.settings.ui.terminal_font_size, (newSize) => {
  if (fontSize.value !== newSize) {
    fontSize.value = newSize
    if (terminal) {
      terminal.options.fontSize = newSize
      if (fitAddon) fitAddon.fit()
      nextTick(() => syncTerminalSize())
    }
  }
}, { immediate: true })

// 会话变化
watch(sessionId, async (newId, oldId) => {
  if (newId !== oldId) {
    if (oldId) {
      clearTerminal()
      lastOutputLength = 0
    }

    if (newId) {
      lastOutputLength = 0
      await nextTick()

      if (terminal) {
        syncTerminalSize()
      }

      if (props.session?.status === 'starting') {
        await sessionStore.startSession(newId)
      }
    }
  }
}, { immediate: true })

onMounted(async () => {
  await nextTick()

  // 确保会话缓存已初始化
  if (sessionId.value) {
    initSessionCache(sessionId.value)
  }

  initTerminal()

  // 添加滚动事件监听
  const viewport = terminalContainerRef.value?.querySelector('.xterm-viewport') as HTMLElement
  if (viewport) {
    viewport.addEventListener('scroll', handleScroll)
  }

  // 从全局缓存恢复历史
  if (terminal && sessionId.value) {
    const history = terminalHistory.getHistory()
    if (history) {
      terminal.write(history)
      // 更新计数器，避免重复写入
      lastOutputLength = history.length
      scrollToBottom()
    }
  }

  terminal?.focus()
})

watch(terminalTheme, () => {
  if (terminal) {
    terminal.options.theme = getTheme()
  }
})

onUnmounted(() => {
  const viewport = terminalContainerRef.value?.querySelector('.xterm-viewport') as HTMLElement
  if (viewport) {
    viewport.removeEventListener('scroll', handleScroll)
  }
  if (scrollTimeout) {
    clearTimeout(scrollTimeout)
  }

  if (resizeObserver) {
    resizeObserver.disconnect()
    resizeObserver = null
  }

  if (terminal) {
    terminal.dispose()
    terminal = null
    webglAddon = null
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
  overflow-y: auto !important;
  overflow-x: hidden;
}

:deep(.xterm-viewport)::-webkit-scrollbar {
  width: 8px;
}

:deep(.xterm-viewport)::-webkit-scrollbar-track {
  background: transparent;
  margin: 4px 0;
}

:deep(.xterm-viewport)::-webkit-scrollbar-thumb {
  background: rgba(128, 128, 128, 0.3);
  border-radius: 4px;
  transition: background 0.2s ease;
}

:deep(.xterm-viewport)::-webkit-scrollbar-thumb:hover {
  background: rgba(128, 128, 128, 0.6);
}

.dark :deep(.xterm-viewport)::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.15);
}

.dark :deep(.xterm-viewport)::-webkit-scrollbar-thumb:hover {
  background: rgba(255, 255, 255, 0.35);
}
</style>
