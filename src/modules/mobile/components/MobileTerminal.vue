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
import { ref, onMounted, onUnmounted, onActivated, watch, nextTick } from 'vue'
import { useSettingsStore } from '@/modules/shared/stores/settings'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import '@xterm/xterm/css/xterm.css'

// ==================== 主题常量 ====================

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

// ==================== 写批次处理 ====================
// 移动端 CPU 较弱，将连续 write 调用合并为批次写入
let writeBatchBuffer = ''
let writeBatchTimer: ReturnType<typeof setTimeout> | null = null
const WRITE_BATCH_DELAY = 16 // ~60fps 一帧的时间

function flushWriteBatch() {
  if (!terminal || !writeBatchBuffer) return
  terminal.write(writeBatchBuffer)
  writeBatchBuffer = ''
}

function scheduleWrite(data: string) {
  writeBatchBuffer += data
  if (writeBatchTimer) clearTimeout(writeBatchTimer)
  writeBatchTimer = setTimeout(flushWriteBatch, WRITE_BATCH_DELAY)
}

// ==================== 组件状态 ====================

const settingsStore = useSettingsStore()
const terminalContainerRef = ref<HTMLElement | null>(null)
const xtermContainerRef = ref<HTMLElement | null>(null)

// xterm.js 实例
let terminal: Terminal | null = null
let fitAddon: FitAddon | null = null
let resizeObserver: ResizeObserver | null = null
let lastOutputIndex = 0

// 记录容器宽度，防止多次计算
let lastContainerWidth = 0

// 滚动状态追踪（与桌面端一致）
let isUserScrolling = false
let scrollTimeout: ReturnType<typeof setTimeout> | null = null

const props = defineProps<{
  output?: string
  /** 父组件已渲染的输出索引，用于增量写入 */
  renderedIndex?: number
}>()

const emit = defineEmits<{
  ready: []
  clear: []
  resize: [cols: number, rows: number]
  /** KeepAlive 恢复时触发，由父组件处理重置逻辑 */
  activated: []
}>()

/** 根据屏幕宽度计算移动端自适应字号 */
function getAdaptiveFontSize(): number {
  const baseSize = settingsStore.settings.ui.terminal_font_size || 14
  const screenWidth = window.innerWidth
  // 小屏手机 (< 400px): 使用较小字号
  // 大屏手机/小平板 (400-600px): 保持默认
  // 大平板 (> 600px): 适当增大
  if (screenWidth < 400) {
    return Math.min(baseSize, 13)
  } else if (screenWidth >= 600) {
    return Math.max(baseSize, 15)
  }
  return baseSize
}

/** 禁用 xterm 内部 textarea，防止移动端弹出键盘 */
function disableXtermTextarea() {
  if (!xtermContainerRef.value) return
  const textarea = xtermContainerRef.value.querySelector('textarea')
  if (textarea) {
    textarea.setAttribute('readonly', '')
    textarea.setAttribute('inputmode', 'none')
    textarea.style.pointerEvents = 'none'
    textarea.style.display = 'none'
  }
}

/** 初始化 xterm.js */
function initTerminal() {
  if (!terminalContainerRef.value || !xtermContainerRef.value) return

  const fontSize = getAdaptiveFontSize()
  const fontFamily = 'Consolas, Monaco, Courier New, monospace'
  const isDarkMode = document.documentElement.classList.contains('dark')

  terminal = new Terminal({
    fontSize,
    fontFamily,
    theme: isDarkMode ? DARK_THEME : LIGHT_THEME,
    cursorBlink: false,
    cursorStyle: 'bar',
    scrollback: 20000,
    allowProposedApi: true,
    cursorInactiveStyle: 'none',
    disableStdin: true,
    convertEol: true, // 自动转换 \n → \r\n，移动端程序输出更可靠
  })

  fitAddon = new FitAddon()
  terminal.loadAddon(fitAddon)
  terminal.loadAddon(new WebLinksAddon())
  terminal.open(xtermContainerRef.value)

  // 禁用内部 textarea，防止键盘弹出
  disableXtermTextarea()

  // 初始 fit
  nextTick(() => {
    performFit()
    if (terminal) {
      emit('resize', terminal.cols, terminal.rows)
    }
  })

  // 监听终端尺寸变化
  terminal.onResize(({ cols, rows }) => {
    emit('resize', cols, rows)
  })

  // 添加滚动事件监听（与桌面端一致）
  nextTick(() => {
    const viewport = terminalContainerRef.value?.querySelector('.xterm-viewport') as HTMLElement
    if (viewport) {
      viewport.addEventListener('scroll', handleScroll)
    }
  })

  emit('ready')
}

/** 执行 fit 操作 */
function performFit() {
  if (!fitAddon || !terminalContainerRef.value) return

  const containerWidth = terminalContainerRef.value.offsetWidth

  // 宽度没有变化，跳过 fit
  if (containerWidth === lastContainerWidth && containerWidth > 0) return

  lastContainerWidth = containerWidth
  fitAddon.fit()

  nextTick(() => {
    if (terminal) {
      terminal.resize(terminal.cols, terminal.rows)
      emit('resize', terminal.cols, terminal.rows)
    }
  })
}

// 使用 requestAnimationFrame 节流 fit 操作
let fitRafId: number | null = null
let fitRafScheduled = false

function scheduleFit() {
  if (fitRafScheduled) return

  fitRafScheduled = true
  fitRafId = requestAnimationFrame(() => {
    fitRafScheduled = false
    performFit()
  })
}

/** 写入数据到终端（使用批次写入优化性能） */
function write(data: string) {
  if (!terminal) return
  scheduleWrite(data)
}

/** 清空终端 */
function clear() {
  if (!terminal) return
  terminal.clear()
  lastOutputIndex = 0
  emit('clear')
}

/** 滚动到底部 - 与桌面端一致 */
function scrollToBottom() {
  if (!terminal) return
  const viewport = terminalContainerRef.value?.querySelector('.xterm-viewport') as HTMLElement
  if (viewport) {
    viewport.scrollTop = viewport.scrollHeight
  }
  nextTick(() => {
    isUserScrolling = false
  })
}

/** 滚动事件处理 - 检测用户是否在滚动（与桌面端一致） */
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

// 监听输出变化，增量写入
// 由父组件通过 renderedIndex 控制增量位置，避免索引不同步问题
watch(() => props.output, (newOutput) => {
  if (!terminal || !newOutput) return

  // 使用父组件传入的 renderedIndex，如果未提供则使用内部 lastOutputIndex（兼容旧版）
  const startIndex = props.renderedIndex ?? lastOutputIndex

  // 边界保护：若 startIndex 超过当前输出长度，重置为 0
  const safeStartIndex = Math.min(startIndex, newOutput.length)
  const newContent = newOutput.slice(safeStartIndex)

  if (newContent.length > 0) {
    scheduleWrite(newContent)
    // 只有当没有传入 renderedIndex 时才更新内部索引
    if (props.renderedIndex === undefined) {
      lastOutputIndex = newOutput.length
    }

    // 只有用户不在滚动时才自动滚动到底部
    if (!isUserScrolling) {
      scrollToBottom()
    }
  }
}, { deep: true })

// 监听字体大小变化（使用自适应字号）
watch(() => settingsStore.settings.ui.terminal_font_size, () => {
  if (!terminal) return
  terminal.options.fontSize = getAdaptiveFontSize()
  nextTick(() => {
    performFit()
  })
})

onMounted(() => {
  initTerminal()

  // 监听容器尺寸变化
  if (terminalContainerRef.value) {
    resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        if (entry.contentRect.width !== lastContainerWidth) {
          scheduleFit()
        }
      }
    })
    resizeObserver.observe(terminalContainerRef.value)

    // 监听窗口 resize 事件
    window.addEventListener('resize', scheduleFit)
  }

  // 监听主题变化（使用主题常量）
  const observer = new MutationObserver(() => {
    if (terminal) {
      const isDarkMode = document.documentElement.classList.contains('dark')
      terminal.options.theme = isDarkMode ? DARK_THEME : LIGHT_THEME
    }
  })
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['class'],
  })
})

// KeepAlive 恢复时通知父组件，由父组件控制是否重置输出
// 避免子组件独自重置索引导致重复渲染
onActivated(() => {
  emit('activated')
})

onUnmounted(() => {
  // 清理写批次定时器
  if (writeBatchTimer) {
    clearTimeout(writeBatchTimer)
    flushWriteBatch() // 清空剩余缓冲区
  }

  // 清理滚动事件监听器
  const viewport = terminalContainerRef.value?.querySelector('.xterm-viewport') as HTMLElement
  if (viewport) {
    viewport.removeEventListener('scroll', handleScroll)
  }

  if (scrollTimeout) clearTimeout(scrollTimeout)

  if (resizeObserver) {
    resizeObserver.disconnect()
    resizeObserver = null
  }

  if (terminal) {
    terminal.dispose()
    terminal = null
  }

  if (fitAddon) fitAddon = null

  if (fitRafId !== null) cancelAnimationFrame(fitRafId)

  window.removeEventListener('resize', scheduleFit)
})

defineExpose({
  write,
  clear,
  scrollToBottom,
  performFit,
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