<template>
  <div
    ref="terminalContainerRef"
    class="h-full w-full terminal-wrapper"
  >
    <!-- 与桌面端一致：xterm 内部处理滚动，readonly 防止触发输入法 -->
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
}>()

const emit = defineEmits<{
  ready: []
  clear: []
  resize: [cols: number, rows: number]
}>()

/** 初始化 xterm.js - 与桌面端一致 */
function initTerminal() {
  if (!terminalContainerRef.value || !xtermContainerRef.value) return

  const fontSize = settingsStore.settings.ui.terminal_font_size || 14
  const fontFamily = 'Consolas, Monaco, Courier New, monospace'

  // 检测当前是否为深色模式
  const isDarkMode = document.documentElement.classList.contains('dark')

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

  // 与桌面端一致的配置
  terminal = new Terminal({
    fontSize,
    fontFamily,
    theme: isDarkMode ? darkTheme : lightTheme,
    cursorBlink: false, // 移动端禁用光标闪烁
    cursorStyle: 'bar',
    scrollback: 20000, // 移动端保留 20000 行历史
    allowProposedApi: true,
    cursorInactiveStyle: 'none',
    // 移动端：禁用终端直接输入，输入只能通过弹窗
    disableStdin: true,
  })

  fitAddon = new FitAddon()
  terminal.loadAddon(fitAddon)
  terminal.loadAddon(new WebLinksAddon())
  terminal.open(xtermContainerRef.value)

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

/** 写入数据到终端 */
function write(data: string) {
  if (!terminal) return
  terminal.write(data)
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

// 监听输出变化，增量写入（与桌面���一致）
watch(() => props.output, (newOutput) => {
  if (!terminal || !newOutput) {
    return
  }

  // 检测输出重置（远程重连等情况），重置索引
  if (newOutput.length < lastOutputIndex) {
    lastOutputIndex = 0
  }

  // 只写入新增的部分（增量写入）
  const startIndex = lastOutputIndex
  const newContent = newOutput.slice(startIndex)

  if (newContent.length > 0) {
    terminal.write(newContent)
    lastOutputIndex = newOutput.length

    // 只有用户不在滚动时才自动滚动到底部
    if (!isUserScrolling) {
      scrollToBottom()
    }
  }
}, { deep: true })

// 监听字体大小变化
watch(() => settingsStore.settings.ui.terminal_font_size, (newSize) => {
  if (!terminal) return
  terminal.options.fontSize = newSize
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

  // 监听主题变化
  const observer = new MutationObserver(() => {
    if (terminal) {
      const isDarkMode = document.documentElement.classList.contains('dark')
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
      terminal.options.theme = isDarkMode ? darkTheme : lightTheme
    }
  })
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['class'],
  })
})

// KeepAlive 恢复时重置输出索引，确保从正确位置开始渲染
onActivated(() => {
  lastOutputIndex = 0
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

  if (resizeObserver) {
    resizeObserver.disconnect()
    resizeObserver = null
  }

  if (terminal) {
    terminal.dispose()
    terminal = null
  }

  if (fitAddon) {
    fitAddon = null
  }

  if (fitRafId !== null) {
    cancelAnimationFrame(fitRafId)
  }

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
</style>