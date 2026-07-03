<template>
  <div class="h-full flex flex-col bg-slate-100 dark:bg-dark-900">
    <!-- Header（终端窗口模式下隐藏，由外层统一管理） -->
    <header v-if="showHeader" class="px-4 py-3 flex items-center justify-between border-b border-slate-200 dark:border-dark-700 bg-white dark:bg-dark-800">
      <div class="flex items-center gap-3">
        <div
          :class="[
            'w-2 h-2 rounded-full',
            statusColor
          ]"
        ></div>
        <h3 class="font-medium text-slate-900 dark:text-white">{{ session?.name || $t('desktop.terminal.defaultName') }}</h3>
      </div>

      <div class="flex items-center gap-2">
        <!-- Theme Switch -->
        <select
          v-model="terminalTheme"
          class="bg-slate-100 dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1 text-sm text-slate-700 dark:text-white shadow-xs dark:shadow-none"
          :title="$t('desktop.terminal.theme')"
        >
          <option v-for="(name, key) in themeNames" :key="key" :value="key">
            {{ name }}
          </option>
        </select>

        <!-- Font Size -->
        <select
          v-model="fontSize"
          class="bg-slate-100 dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1 text-sm text-slate-700 dark:text-white shadow-xs dark:shadow-none"
          :title="$t('desktop.terminal.fontSize')"
        >
          <option v-for="size in [12, 14, 16, 18, 20]" :key="size" :value="size">
            {{ size }}px
          </option>
        </select>

        <!-- Clear Button -->
        <Button variant="ghost" size="sm" @click="clearTerminal" :title="$t('desktop.terminal.clearScreen')">
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </Button>

        <!-- Refresh Format Button -->
        <Button variant="ghost" size="sm" @click="refreshTerminal" :title="$t('desktop.terminal.refreshFormat')">
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
        </Button>

        <!-- Plugin Toolbar Extension -->
        <PluginTerminalToolbar />
      </div>
    </header>

    <!-- Terminal Container (xterm.js) -->
    <div ref="terminalContainerRef" class="flex-1 overflow-hidden relative">
      <!-- 滚动到底部指示器：用户向上滚动时显示，点击回到底部 -->
      <transition name="scroll-indicator">
        <button
          v-if="isUserScrolling"
          class="scroll-to-bottom-btn"
          @click="scrollToBottomManual"
          :title="$t('desktop.terminal.scrollToBottom')"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 14l-7 7m0 0l-7-7m7 7V3" />
          </svg>
        </button>
      </transition>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from 'vue'
import type { SessionInfo } from '@/stores/session'
import { useSessionStore } from '@/stores/session'
import { useSettingsStore } from '@/stores/settings'
import Button from '@/components/Button.vue'
import PluginTerminalToolbar from '@/plugin/components/PluginTerminalToolbar.vue'
import { usePtyOutput } from '@/composables/usePtyOutput'
import {
  useTerminalHistory,
  initSessionCache,
  destroySessionCache,
  resizeHiddenTerminal
} from '@/composables/useGlobalTerminal'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { WebglAddon } from '@xterm/addon-webgl'
import { on as pluginEventOn, emit as pluginEventEmit, clearPluginEvents } from '@/plugin/events'
import '@xterm/xterm/css/xterm.css'

interface Props {
  session?: SessionInfo | null
  showInput?: boolean
  /** 是否显示组件内 header（终端窗口模式下由外层统一管理 header） */
  showHeader?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  showInput: true,
  showHeader: true,
})

const sessionStore = useSessionStore()
const settingsStore = useSettingsStore()
const terminalContainerRef = ref<HTMLElement | null>(null)
const fontSize = ref(settingsStore.settings.ui.terminal_font_size)
const terminalTheme = ref<string>(settingsStore.settings.ui.terminal_theme || 'dracula')

// xterm.js 实例（组件内）
let terminal: Terminal | null = null
let fitAddon: FitAddon | null = null
let webglAddon: WebglAddon | null = null
let resizeObserver: ResizeObserver | null = null
let resizeRaf = 0

// 滚动状态追踪
const isUserScrolling = ref(false)

// rAF 节流：防止快速连续 scrollToBottom 调用导致 WebGL 重影
// 多次输出事件在同一帧内触发时，只执行一次 scrollToBottom
let pendingScrollRaf = 0

// 追踪当前行输入（MVP：仅追踪可打印字符和退格，供 AI 插件读取）
let currentLineBuffer = ''

const sessionId = computed(() => props.session?.id || '')

// PTY 输出监听（组件内）
const { output: realtimeOutput, clearOutput } = usePtyOutput(sessionId)

// 终端历史缓存
const terminalHistory = useTerminalHistory(sessionId.value)

const statusColor = computed(() => {
  if (!props.session) return 'bg-slate-400 dark:bg-dark-500'

  switch (props.session.status) {
    case 'running':
      return 'bg-green-500'
    case 'waitingInput':
      return 'bg-yellow-500 animate-pulse'
    case 'error':
      return 'bg-red-500'
    case 'stopped':
      return 'bg-slate-400 dark:bg-dark-500'
    case 'starting':
      return 'bg-blue-500 animate-pulse'
    default:
      return 'bg-slate-400 dark:bg-dark-500'
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
      console.warn('[TerminalPreview] WebGL context lost, attempting recovery')
      webglAddon?.dispose()
      webglAddon = null
      // 延迟 1s 后尝试重新创建 WebGL 渲染器
      setTimeout(() => {
        if (!terminal || webglAddon) return
        try {
          const newAddon = new WebglAddon()
          newAddon.onContextLoss(() => {
            console.warn('[TerminalPreview] WebGL context lost again')
            newAddon.dispose()
            if (webglAddon === newAddon) webglAddon = null
          })
          terminal.loadAddon(newAddon)
          webglAddon = newAddon
          console.info('[TerminalPreview] WebGL context recovered')
        } catch (e) {
          console.warn('[TerminalPreview] WebGL recovery failed, using canvas fallback:', e)
          webglAddon = null
        }
      }, 1000)
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
    cursorBlink: true,
    cursorStyle: 'bar',
    cursorWidth: 1,
    scrollback: 10000,
    allowProposedApi: true,
  })

  fitAddon = new FitAddon()
  terminal.loadAddon(fitAddon)
  terminal.loadAddon(new WebLinksAddon())
  terminal.open(terminalContainerRef.value)
  initWebGL(terminal)

  // WebGL 渲染器激活后，隐藏 DOM 层光标避免双光标问题
  // 只隐藏 DOM 层，保留 WebGL 层光标（WebGL 光标更流畅且不会出现双光标）
  if (webglAddon) {
    terminal.element?.classList.add('xterm-hidden-cursor')
  }

  fitAddon.fit()

  syncTerminalSize()

  terminal.onResize(({ cols, rows }) => {
    if (props.session) {
      sessionStore.resizeSession(props.session.id, cols, rows)
      // 同步隐藏终端尺寸，确保行数计算一致
      resizeHiddenTerminal(props.session.id, cols, rows)
    }
  })

  // ResizeObserver — 使用 rAF 节流避免快速连续 fit 导致 WebGL 重影
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

    // 节流：同一帧内多次 resize 只执行一次 fit
    if (!resizeRaf) {
      resizeRaf = requestAnimationFrame(() => {
        resizeRaf = 0
        if (!fitAddon || !terminal) return
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
    }
  })
  resizeObserver.observe(terminalContainerRef.value)

  // 键盘输入
  terminal.onData((data: string) => {
    if (!props.session) return
    sessionStore.writeToSession(props.session.id, data)

    // 追踪当前行输入
    if (data === '\r' || data === '\n') {
      currentLineBuffer = ''
    } else if (data === '\x7f' || data === '\b') {
      currentLineBuffer = currentLineBuffer.slice(0, -1)
    } else if (data === '\x15') {
      // Ctrl+U 清除当前行
      currentLineBuffer = ''
    } else if (data.length === 1 && data.charCodeAt(0) >= 32) {
      currentLineBuffer += data
    }
    // 忽略方向键、控制序列等复杂场景
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
  // 刷新格式：重新 fit 终端尺寸并同步到 PTY，不清除内容
  if (!fitAddon || !terminal || !props.session) return
  fitAddon.fit()
  syncTerminalSize()
}

function scrollToBottom() {
  // rAF 节流：同一帧内多次调用只执行一次 scrollToBottom
  // 避免 WebGL 渲染器双缓冲不同步导致的重影
  if (!pendingScrollRaf) {
    pendingScrollRaf = requestAnimationFrame(() => {
      pendingScrollRaf = 0
      terminal?.scrollToBottom()
    })
  }
}

function handleScroll() {
  // 使用 xterm.js buffer 判断是否在底部，比手动计算 scrollTop 更准确
  if (!terminal) return
  const buffer = terminal.buffer.active
  const viewportTop = terminal.buffer.active.viewportY
  const viewportBottom = viewportTop + terminal.rows
  const totalLines = buffer.length
  isUserScrolling.value = viewportBottom < totalLines - 1
}

/// 用户点击"回到底部"按钮：重置滚动状态并滚到底
function scrollToBottomManual() {
  isUserScrolling.value = false
  terminal?.scrollToBottom()
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

  if (!isUserScrolling.value) {
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

  // 确保会话缓存已初始化（如果已存在则跳过）
  if (sessionId.value) {
    initSessionCache(sessionId.value)
  }

  initTerminal()

  // 监听 AI 插件请求当前终端输入
  pluginEventOn('__host__', 'ai-chatbox:getCurrentInput', () => {
    pluginEventEmit('ai-chatbox:currentInput', { sessionId: sessionId.value, text: currentLineBuffer })
  })

  // 显示终端 fit 后，同步隐藏终端尺寸
  if (terminal && sessionId.value) {
    resizeHiddenTerminal(sessionId.value, terminal.cols, terminal.rows)
  }

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

// 主题变化：更新终端 + 持久化
let themeSaveTimeout: ReturnType<typeof setTimeout> | null = null
watch(terminalTheme, (newTheme) => {
  if (terminal) {
    terminal.options.theme = getTheme()
  }
  if (themeSaveTimeout) clearTimeout(themeSaveTimeout)
  themeSaveTimeout = setTimeout(() => {
    settingsStore.saveSettings({
      ui: { ...settingsStore.settings.ui, terminal_theme: newTheme }
    })
  }, 300)
})

// 外部设置变化同步主题
watch(() => settingsStore.settings.ui.terminal_theme, (newTheme) => {
  if (newTheme && terminalTheme.value !== newTheme) {
    terminalTheme.value = newTheme
  }
})

onUnmounted(() => {
  // 清理 AI 插件事件监听
  clearPluginEvents('__host__')

  // 清理待处理的滚动 rAF
  if (pendingScrollRaf) {
    cancelAnimationFrame(pendingScrollRaf)
    pendingScrollRaf = 0
  }

  // 清理 resize rAF
  if (resizeRaf) {
    cancelAnimationFrame(resizeRaf)
    resizeRaf = 0
  }

  const viewport = terminalContainerRef.value?.querySelector('.xterm-viewport') as HTMLElement
  if (viewport) {
    viewport.removeEventListener('scroll', handleScroll)
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

// ==================== Expose ====================

/** 暴露给父组件：终端窗口模式下外层 header 需要访问的响应式状态和方法 */
defineExpose({
  fontSize,
  terminalTheme,
  themeNames,
  isUserScrolling,
  clearTerminal,
  refreshTerminal,
  scrollToBottomManual,
})
</script>

<style scoped>
:deep(.xterm) {
  height: 100%;
  padding: 8px;
}

:deep(.xterm-viewport) {
  border-radius: 0;
  overflow-x: hidden;
}

:deep(.xterm-viewport)::-webkit-scrollbar {
  width: 6px;
}

:deep(.xterm-viewport)::-webkit-scrollbar-track {
  background: transparent;
  margin: 8px 2px;
  border-radius: 3px;
}

:deep(.xterm-viewport)::-webkit-scrollbar-thumb {
  background: rgba(128, 128, 128, 0.25);
  border-radius: 3px;
  transition: background 0.2s ease, width 0.2s ease;
}

:deep(.xterm-viewport)::-webkit-scrollbar-thumb:hover {
  background: rgba(128, 128, 128, 0.5);
}

:deep(.xterm-viewport:hover)::-webkit-scrollbar-thumb {
  background: rgba(128, 128, 128, 0.35);
}

.dark :deep(.xterm-viewport)::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.12);
  border-radius: 3px;
}

.dark :deep(.xterm-viewport)::-webkit-scrollbar-thumb:hover {
  background: rgba(255, 255, 255, 0.3);
}

.dark :deep(.xterm-viewport:hover)::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.2);
}

/* WebGL 模式下隐藏 DOM 层光标，避免双光标问题 */
/* 只隐藏 DOM 光标元素，不隐藏 cursor-layer（WebGL 渲染器有自己的光标实现） */
:deep(.xterm-hidden-cursor .xterm-cursor) {
  display: none !important;
}

/* 滚动到底部指示器 */
.scroll-to-bottom-btn {
  position: absolute;
  bottom: 16px;
  right: 16px;
  width: 36px;
  height: 36px;
  border-radius: 50%;
  background: rgba(128, 128, 128, 0.6);
  color: white;
  border: none;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: background 0.2s ease;
  z-index: 10;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
}

.scroll-to-bottom-btn:hover {
  background: rgba(128, 128, 128, 0.85);
}

.dark .scroll-to-bottom-btn {
  background: rgba(255, 255, 255, 0.25);
  color: var(--text-primary);
}

.dark .scroll-to-bottom-btn:hover {
  background: rgba(255, 255, 255, 0.45);
}

/* 滚动指示器过渡 */
.scroll-indicator-enter-active,
.scroll-indicator-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}

.scroll-indicator-enter-from,
.scroll-indicator-leave-to {
  opacity: 0;
  transform: translateY(8px);
}
</style>
