<template>
  <div
    ref="terminalContainerRef"
    class="h-full w-full terminal-wrapper"
  >
    <!-- xterm 容器 -->
    <div ref="xtermContainerRef" class="xterm-container"></div>
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
  output?: string  // 保留用于兼容，但可选
  externalInstance?: Terminal | null  // 外部传入的 xterm.js 实例
}

const props = withDefaults(defineProps<Props>(), {
  output: '',
  externalInstance: null,
})

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
// 标记是否使用外部实例（外部实例由全局管理器管理，不在此 dispose）
let usingExternalInstance = false

// 当前已渲染的输出长度
const currentOutputLength = ref(0)

// ==================== Terminal Initialization ====================

function initTerminal() {
  if (!xtermContainerRef.value) {
    console.warn('[MobileTerminal] initTerminal: container not ready')
    return
  }

  const theme = getTheme()
  console.log('[MobileTerminal] initTerminal: container ready, externalInstance=', !!props.externalInstance)

  // 如果有外部实例，直接使用（全局管理器创建的隐藏实例）
  if (props.externalInstance) {
    terminal = props.externalInstance
    usingExternalInstance = true
    console.log('[MobileTerminal] Using external terminal instance')

    // 检查是否已挂载
    if (!terminal.element) {
      // 首次挂载：必须先 open，然后才能创建新的 FitAddon
      terminal.open(xtermContainerRef.value)
      console.log('[MobileTerminal] Terminal opened in container')
    } else {
      // 已挂载到其他容器，移动到当前容器
      const oldContainer = terminal.element.parentElement
      if (oldContainer && oldContainer !== xtermContainerRef.value) {
        oldContainer.removeChild(terminal.element)
        xtermContainerRef.value.appendChild(terminal.element)
        console.log('[MobileTerminal] Terminal moved to new container')
      }
    }

    // 每次挂载都创建新的 FitAddon（确保尺寸正确）
    fitAddon = new FitAddon()
    terminal.loadAddon(fitAddon)

    // 更新主题
    terminal.options.theme = theme

    // Fit 到容器
    nextTick(() => {
      fitTerminal()
      emit('ready')
      checkScrollState()
    })

    // 监听终端大小变化
    terminal.onResize(() => {
      if (terminal) {
        emit('resize', terminal.cols, terminal.rows)
      }
    })

    // 监听容器大小变化
    const resizeObserver = new ResizeObserver(() => {
      fitTerminal()
    })
    if (xtermContainerRef.value) {
      resizeObserver.observe(xtermContainerRef.value)
    }
    return
  }

  // 没有外部实例，创建新的
  usingExternalInstance = false
  console.log('[MobileTerminal] Creating new terminal instance')

  terminal = new Terminal({
    theme,
    fontFamily: '"SF Mono", "Menlo", "Monaco", "Courier New", monospace',
    fontSize: 14,
    lineHeight: 1.2,
    cursorBlink: true,
    cursorStyle: 'block',
    allowProposedApi: true,
    scrollback: 10000,
    convertEol: true,
  })

  // 先 open，再加载 FitAddon
  terminal.open(xtermContainerRef.value)

  fitAddon = new FitAddon()
  terminal.loadAddon(fitAddon)

  terminal.loadAddon(new WebLinksAddon())

  nextTick(() => {
    fitTerminal()
    emit('ready')
    checkScrollState()
  })

  terminal.onResize(() => {
    if (terminal) {
      emit('resize', terminal.cols, terminal.rows)
    }
  })

  const resizeObserver = new ResizeObserver(() => {
    fitTerminal()
  })
  if (xtermContainerRef.value) {
    resizeObserver.observe(xtermContainerRef.value)
  }
}

// 检查滚动状态
function checkScrollState() {
  if (!terminalContainerRef.value) return

  const viewport = terminalContainerRef.value.querySelector('.xterm-viewport') as HTMLElement
  if (viewport) {
    console.log('[MobileTerminal] Scroll state:', {
      scrollHeight: viewport.scrollHeight,
      clientHeight: viewport.clientHeight,
      scrollTop: viewport.scrollTop,
      canScroll: viewport.scrollHeight > viewport.clientHeight
    })

    // 尝试强制设置滚动
    if (viewport.scrollHeight > viewport.clientHeight) {
      viewport.style.overflowY = 'auto'
    }
  } else {
    console.warn('[MobileTerminal] xterm-viewport not found')
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
// 注意：当使用外部实例时，不处理 output prop（由全局管理器处理）
import { watch } from 'vue'

watch(
  () => props.output,
  (newOutput) => {
    // 外部实例时跳过，由全局管理器处理输出
    if (usingExternalInstance) return
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
  // 外部实例由全局管理器管理，这里不 dispose
  if (usingExternalInstance) {
    // 仅移除 DOM 元素，不 dispose 终端实例
    if (terminal?.element?.parentElement) {
      terminal.element.parentElement.removeChild(terminal.element)
    }
    terminal = null
    fitAddon = null
    return
  }

  // 内部创建的实例才 dispose
  if (terminal) {
    terminal.dispose()
    terminal = null
  }
})
</script>

<style scoped>
.terminal-wrapper {
  height: 100%;
  width: 100%;
}

.terminal-wrapper:focus,
.terminal-wrapper:focus-visible {
  outline: none;
}

/* xterm 容器 */
.xterm-container {
  height: 100%;
  width: 100%;
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

/* xterm 视口滚动 */
:deep(.xterm-viewport) {
  overflow-y: auto !important;
  overflow-x: hidden !important;
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