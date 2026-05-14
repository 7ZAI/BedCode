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
        <!-- Font Size -->
        <select
          v-model="fontSize"
          class="bg-gray-100 dark:bg-dark-700 border border-gray-200 dark:border-dark-600 rounded px-2 py-1 text-sm text-gray-700 dark:text-white"
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
let lastOutputIndex = 0

// 滚动状态追踪
let isUserScrolling = false
let scrollTimeout: ReturnType<typeof setTimeout> | null = null

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

// 检测当前是否为深色模式
const isDarkMode = computed(() => {
  return document.documentElement.classList.contains('dark')
})

// 浅色主题
const lightTheme = {
  background: '#ffffff',
  foreground: '#333333',
  cursor: '#000000',
  cursorAccent: '#ffffff',
  selectionBackground: '#b4d7ff',
  black: '#000000',
  red: '#cd3131',
  green: '#0dbc79',
  yellow: '#e5e510',
  blue: '#2472c8',
  magenta: '#bc3fbc',
  cyan: '#11a8cd',
  white: '#e5e5e5',
  brightBlack: '#666666',
  brightRed: '#f14c4c',
  brightGreen: '#23d18b',
  brightYellow: '#f5f543',
  brightBlue: '#3b8eea',
  brightMagenta: '#d670d6',
  brightCyan: '#29b8db',
  brightWhite: '#ffffff',
}

// 深色主题
const darkTheme = {
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
}

// 根据当前主题返回对应的 xterm 主题
function getTheme() {
  return isDarkMode.value ? darkTheme : lightTheme
}

// 初始化 xterm.js
function initTerminal() {
  if (!terminalContainerRef.value) return

  terminal = new Terminal({
    fontSize: fontSize.value,
    fontFamily: 'Consolas, Monaco, Courier New, monospace',
    theme: getTheme(),
    cursorBlink: true,
    cursorStyle: 'block',
    scrollback: 50000, // 桌面端实时预览保留 50000 行历史
    allowProposedApi: true,
    // 确保光标样式正确
    cursorWidth: 1,
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
  let lastCols = 0
  let lastRows = 0
  const resizeObserver = new ResizeObserver(() => {
    if (fitAddon && terminal) {
      fitAddon.fit()
      const newCols = terminal.cols
      const newRows = terminal.rows

      // 检查终端尺寸是否发生显著变化（列宽变化超过 10% 或行高变化超过 5 行）
      // 这种情况下需要刷新终端内容，因为已输出的文本是按旧尺寸换行的
      const colsChanged = Math.abs(newCols - lastCols) > lastCols * 0.1
      const rowsChanged = Math.abs(newRows - lastRows) > 5

      if ((colsChanged || rowsChanged) && lastCols > 0 && lastRows > 0) {
        // 尺寸发生显著变化，重新同步 PTY 大小并刷新终端内容
        syncTerminalSize()
        refreshTerminal()
      } else {
        syncTerminalSize()
      }

      lastCols = newCols
      lastRows = newRows
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

/** 刷新终端显示 - 当窗口尺寸发生显著变化时调用
 *
 * 原因：已输出的内容是按照旧的终端宽度换行的
 * 当窗口变宽/变窄时，这些换行符位置不变，导致显示错乱
 * 解决方案：
 * 1. 清空终端显示
 * 2. 发送清屏 + 光标归位序列，触发应用程序重新绘制
 * 3. 重新写入输出缓冲区的内容（作为后备）
 */
function refreshTerminal() {
  if (!terminal || !props.session) return

  // 清空终端显示
  terminal.clear()

  // 发送终端刷新序列：
  // - \x1b[2J: 清屏（保持光标位置）
  // - \x1b[H: 光标归位到左上角
  // 这会触发大多数终端应用程序重新绘制当前屏幕
  terminal.write('\x1b[2J\x1b[H')

  // 重新写入输出缓冲区的内容
  for (const data of output.value) {
    terminal.write(data)
  }

  // 滚动到底部
  scrollToBottom()
}

/** 滚动到底部 */
function scrollToBottom() {
  if (!terminal) return
  const viewport = terminalContainerRef.value?.querySelector('.xterm-viewport') as HTMLElement
  if (viewport) {
    viewport.scrollTop = viewport.scrollHeight
  }
}

/** 滚动事件处理 - 检测用户是否在滚动 */
function handleScroll() {
  const viewport = terminalContainerRef.value?.querySelector('.xterm-viewport') as HTMLElement
  if (!viewport) return

  // 检测是否在底部（允许 50px 误差）
  const isAtBottom = viewport.scrollHeight - viewport.scrollTop <= viewport.clientHeight + 50

  // 用户不在底部 = 正在向上滚动查看历史
  isUserScrolling = !isAtBottom

  // 滚动停止后清除状态（300ms 防抖）
  if (scrollTimeout) clearTimeout(scrollTimeout)
  scrollTimeout = setTimeout(() => {
    isUserScrolling = false
  }, 300)
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
  // 增量写入：只写入新增的部分
  for (let i = lastOutputIndex; i < newOutput.length; i++) {
    terminal.write(newOutput[i])
  }
  lastOutputIndex = newOutput.length

  // 只有用户不在滚动时才自动滚动到底部
  if (!isUserScrolling) {
    scrollToBottom()
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

    // 添加滚动事件监听
    const viewport = terminalContainerRef.value?.querySelector('.xterm-viewport') as HTMLElement
    if (viewport) {
      viewport.addEventListener('scroll', handleScroll)
    }
  })

  // 监听主题变化
  const observer = new MutationObserver(() => {
    if (terminal) {
      terminal.options.theme = getTheme()
    }
  })
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['class'],
  })
})

onUnmounted(() => {
  // 清理滚动事件监听器
  const viewport = terminalContainerRef.value?.querySelector('.xterm-viewport') as HTMLElement
  if (viewport) {
    viewport.removeEventListener('scroll', handleScroll)
  }
  if (scrollTimeout) {
    clearTimeout(scrollTimeout)
  }

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
  overflow-y: auto !important;
  overflow-x: hidden;
}

/* 确保滚动条始终可见 */
:deep(.xterm-viewport)::-webkit-scrollbar {
  width: 10px;
}

:deep(.xterm-viewport)::-webkit-scrollbar-track {
  background: transparent;
}

:deep(.xterm-viewport)::-webkit-scrollbar-thumb {
  background: #666;
  border-radius: 5px;
}

:deep(.xterm-viewport)::-webkit-scrollbar-thumb:hover {
  background: #888;
}
</style>