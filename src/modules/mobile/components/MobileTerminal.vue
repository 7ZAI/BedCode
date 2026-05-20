<template>
  <div
    ref="terminalContainerRef"
    class="h-full w-full terminal-wrapper"
  >
    <!-- xterm 容器，移动端禁用点击和触摸选择 -->
    <div ref="xtermContainerRef" class="xterm-container" readonly></div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, nextTick, onBeforeUnmount } from 'vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import '@xterm/xterm/css/xterm.css'

// ==================== Props ====================

interface Props {
  output: string
}

const props = defineProps<Props>()

// ==================== Emits ====================

const emit = defineEmits<{
  ready: []
  clear: []
  resize: [cols: number, rows: number]
  activated: []
}>()

// ==================== Theme Constants ====================

const LIGHT_THEME = {
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

const DARK_THEME = {
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

// ==================== Theme Helpers ====================

function getCurrentTheme(): 'light' | 'dark' {
  return document.documentElement.classList.contains('dark') ? 'dark' : 'light'
}

function getTheme() {
  const theme = getCurrentTheme()
  return theme === 'dark' ? DARK_THEME : LIGHT_THEME
}

// ==================== Refs ====================

const terminalContainerRef = ref<HTMLElement | null>(null)
const xtermContainerRef = ref<HTMLElement | null>(null)

let terminal: Terminal | null = null
let fitAddon: FitAddon | null = null

// 当前已渲染的输出长度
const currentOutputLength = ref(0)

// ==================== Terminal Initialization ====================

function initTerminal() {
  if (!xtermContainerRef.value) return

  const theme = getTheme()

  terminal = new Terminal({
    theme,
    fontFamily: '"SF Mono", "Menlo", "Monaco", "Courier New", monospace',
    fontSize: 14,
    lineHeight: 1.2,
    cursorBlink: true,
    cursorStyle: 'block',
    allowProposedApi: true,
    // 移动端优化
    scrollback: 10000,
    convertEol: true,
  })

  // 添加 Fit addon
  fitAddon = new FitAddon()
  terminal.loadAddon(fitAddon)

  // 添加 Web Links addon
  terminal.loadAddon(new WebLinksAddon())

  // 禁用右键菜单（移动端）
  terminal.element?.addEventListener('contextmenu', (e) => e.preventDefault())

  // 打开终端
  terminal.open(xtermContainerRef.value)

  // Fit 到容器
  nextTick(() => {
    fitTerminal()
    emit('ready')
  })

  // 监听终端大小变化
  terminal.onResize(() => {
    if (terminal) {
      emit('resize', terminal.cols, terminal.rows)
    }
  })

  // 监听窗口大小变化
  const resizeObserver = new ResizeObserver(() => {
    fitTerminal()
  })
  if (xtermContainerRef.value) {
    resizeObserver.observe(xtermContainerRef.value)
  }
}

function fitTerminal() {
  if (fitAddon && terminal) {
    try {
      fitAddon.fit()
      // 强制调整大小以触发 PTY 更新
      emit('resize', terminal.cols, terminal.rows)
    } catch (e) {
      console.warn('[MobileTerminal] fit failed:', e)
    }
  }
}

// ==================== Output Handling ====================

// 监听输出变化，追加新数据
import { watch } from 'vue'

watch(
  () => props.output,
  (newOutput) => {
    if (!terminal) return

    // 获取新数据（从上次渲染的位置开始）
    const newData = newOutput.slice(currentOutputLength.value)

    if (newData) {
      // 写入终端
      terminal.write(newData)
      currentOutputLength.value = newOutput.length

      // 滚动到底部
      nextTick(() => {
        terminal?.scrollToBottom()
      })
    }
  }
)

// ==================== Public Methods ====================

function clear() {
  if (terminal) {
    terminal.clear()
    currentOutputLength.value = 0
  }
  emit('clear')
}

function getTerminal(): Terminal | null {
  return terminal
}

function getCols(): number {
  return terminal?.cols ?? 80
}

function getRows(): number {
  return terminal?.rows ?? 24
}

// 暴露给父组件
defineExpose({
  clear,
  getTerminal,
  getCols,
  getRows,
})

// ==================== Lifecycle ====================

onMounted(() => {
  initTerminal()

  // 监听 DOM 变化以检测主题切换
  const observer = new MutationObserver(() => {
    if (terminal) {
      const theme = getTheme()
      terminal.options.theme = theme
    }
  })
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] })

  // 触发 activated 事件（用于 KeepAlive 恢复）
  emit('activated')
})

onBeforeUnmount(() => {
  if (terminal) {
    terminal.dispose()
    terminal = null
  }
})
</script>

<style scoped>
.terminal-wrapper {
  /* 固定宽度布局 */
  contain: layout style;
  /* 禁止文本选择，避免移动端误触 */
  user-select: none;
  -webkit-user-select: none;
  /* 禁止获取焦点，防止点击触发输入法 */
  -webkit-tap-highlight-color: transparent;
}

.terminal-wrapper:focus,
.terminal-wrapper:focus-visible {
  outline: none;
}

/* xterm 容器 - 与桌面端一致 */
.xterm-container {
  height: 100%;
}

:deep(.xterm) {
  height: 100%;
  padding: 0;
}

:deep(.xterm-screen) {
  width: 100%;
}

/* 禁用 xterm 的 focus 样式 */
:deep(.xterm:focus) {
  outline: none;
}

:deep(.xterm-focus) {
  outline: none;
}

/* xterm 视口滚动 - 修复移动端无法滚动问题 */
:deep(.xterm-viewport) {
  overflow-y: auto !important;
  overflow-x: hidden;
  /* 启用移动端弹性滚动 */
  -webkit-overflow-scrolling: touch;
  /* 防止滚动到边界时触发页面整体滚动 */
  overscroll-behavior: contain;
}

:deep(.xterm-viewport)::-webkit-scrollbar {
  width: 6px;
}

:deep(.xterm-viewport)::-webkit-scrollbar-track {
  background: transparent;
}

:deep(.xterm-viewport)::-webkit-scrollbar-thumb {
  background: rgba(128, 128, 128, 0.4);
  border-radius: 3px;
}
</style>