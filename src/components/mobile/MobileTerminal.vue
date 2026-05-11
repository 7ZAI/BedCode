<template>
  <div
    ref="terminalContainerRef"
    class="h-full w-full overflow-hidden terminal-wrapper"
  >
    <!-- 强制宽度容器，防止宽度变化 -->
    <div class="terminal-inner">
      <div ref="xtermContainerRef" class="h-full"></div>
    </div>

    <!-- 回到底部按钮 - 当用户滚动到上方时显示 -->
    <transition name="fade">
      <button
        v-if="showScrollToBottom"
        class="scroll-to-bottom-btn"
        @click="scrollToBottom"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 14l-7 7m0 0l-7-7m7 7V3" />
        </svg>
        底部
      </button>
    </transition>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, nextTick } from 'vue'
import { useSettingsStore } from '@/stores/settings'
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

// 滚动状态追踪
let isUserScrolling = false
let scrollTimeout: ReturnType<typeof setTimeout> | null = null
let scrollCheckScheduled = false

// 是否显示"回到底部"按钮
const showScrollToBottom = ref(false)

const props = defineProps<{
  output?: string
}>()

const emit = defineEmits<{
  ready: []
  clear: []
}>()

/** 初始化 xterm.js */
function initTerminal() {
  if (!terminalContainerRef.value || !xtermContainerRef.value) return

  // 强制容器宽度为 100%
  terminalContainerRef.value.style.width = '100%'
  xtermContainerRef.value.style.width = '100%'

  // 使用与桌面端一致的配置
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

  terminal = new Terminal({
    fontSize,
    fontFamily,
    theme: isDarkMode ? darkTheme : lightTheme,
    cursorBlink: false, // 移动端禁用光标闪烁，节省性能
    cursorStyle: 'bar',
    // 移动端减少滚动缓冲区以节省内存
    scrollback: 1000,
    allowProposedApi: true,
    // 禁用光标样式渲染优化
    cursorInactiveStyle: 'none',
    // 移动端优化：禁用终端直接输入，输入只能通过弹窗
    disableStdin: true,
    allowTransparency: false,
  })

  fitAddon = new FitAddon()
  terminal.loadAddon(fitAddon)
  terminal.loadAddon(new WebLinksAddon())
  terminal.open(xtermContainerRef.value)

  // 初始 fit，使用 nextTick 确保 DOM 完成
  nextTick(() => {
    performFit()
  })

  emit('ready')
}

/** 执行 fit 操作，带宽度检查防止重复计算 */
function performFit() {
  if (!fitAddon || !terminalContainerRef.value) return

  const containerWidth = terminalContainerRef.value.offsetWidth

  // 宽度没有变化，跳过 fit
  if (containerWidth === lastContainerWidth && containerWidth > 0) return

  lastContainerWidth = containerWidth
  fitAddon.fit()

  // 额外触发一次 resize 事件确保终端正确响应
  nextTick(() => {
    if (terminal) {
      terminal.resize(terminal.cols, terminal.rows)
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

/** 滚动到底部 */
function scrollToBottom() {
  if (!terminal) return
  const viewport = xtermContainerRef.value?.querySelector('.xterm-viewport') as HTMLElement
  if (viewport) {
    viewport.scrollTop = viewport.scrollHeight
    // 滚动到底部后隐藏按钮
    nextTick(() => {
      showScrollToBottom.value = false
      isUserScrolling = false
    })
  }
}

/** 滚动事件处理 - 检测用户是否在滚动 */
function handleScroll() {
  const viewport = xtermContainerRef.value?.querySelector('.xterm-viewport') as HTMLElement
  if (!viewport) return

  // 检测是否在底部（允许 50px 误差）
  const isAtBottom = viewport.scrollHeight - viewport.scrollTop <= viewport.clientHeight + 50

  // 用户不在底部 = 正在向上滚动查看历史
  isUserScrolling = !isAtBottom

  // 显示/隐藏回到底部按钮
  showScrollToBottom.value = !isAtBottom

  // 滚动停止后清除状态（300ms 防抖）
  if (scrollTimeout) clearTimeout(scrollTimeout)
  scrollTimeout = setTimeout(() => {
    isUserScrolling = false
    // 滚动停止后检查是否在底部
    const nowAtBottom = viewport.scrollHeight - viewport.scrollTop <= viewport.clientHeight + 50
    showScrollToBottom.value = !nowAtBottom
  }, 300)
}

/** 检测滚动位置，更新按钮显示状态 */
function checkScrollPosition() {
  if (scrollCheckScheduled) return

  scrollCheckScheduled = true
  requestAnimationFrame(() => {
    scrollCheckScheduled = false
    const viewport = xtermContainerRef.value?.querySelector('.xterm-viewport') as HTMLElement
    if (!viewport) return

    const isAtBottom = viewport.scrollHeight - viewport.scrollTop <= viewport.clientHeight + 50
    showScrollToBottom.value = !isAtBottom && !isUserScrolling
  })
}

// 监听输出变化，增量写入
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
    // 输出变化后检查滚动位置，更新按钮状态
    nextTick(() => {
      checkScrollPosition()
    })
  }
}, { deep: true })

// 监听字体大小变化
watch(() => settingsStore.settings.ui.terminal_font_size, (newSize) => {
  if (!terminal) return
  terminal.options.fontSize = newSize
  // 字体变化后需要重新 fit
  nextTick(() => {
    performFit()
  })
})

onMounted(() => {
  initTerminal()

  // 添加滚动事件监听
  const viewport = xtermContainerRef.value?.querySelector('.xterm-viewport') as HTMLElement
  if (viewport) {
    viewport.addEventListener('scroll', handleScroll)
  }

  // 使用 ResizeObserver 监听容器尺寸变化，使用节流
  if (terminalContainerRef.value) {
    resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        // 只在尺寸真正变化时触发 fit
        if (entry.contentRect.width !== lastContainerWidth) {
          scheduleFit()
        }
      }
    })
    resizeObserver.observe(terminalContainerRef.value)

    // 监听窗口 resize 事件（处理键盘弹出等场景）
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

onUnmounted(() => {
  // 清理滚动事件监听器
  const viewport = xtermContainerRef.value?.querySelector('.xterm-viewport') as HTMLElement
  if (viewport) {
    viewport.removeEventListener('scroll', handleScroll)
  }
  if (scrollTimeout) {
    clearTimeout(scrollTimeout)
  }

  // 清理 ResizeObserver
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
  /* 固定宽度布局，防止内容宽度变化 */
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

.terminal-inner {
  width: 100%;
  height: 100%;
  /* 确保内部元素不会超出容器 */
  overflow: hidden;
}

:deep(.xterm) {
  height: 100%;
  padding: 0;
}

:deep(.xterm-viewport) {
  border-radius: 0;
  overflow-y: auto !important;
  overflow-x: hidden !important;
  /* 防止视口宽度变化 */
  width: 100% !important;
}

:deep(.xterm-screen) {
  height: 100%;
  /* 确保屏幕宽度固定 */
  width: 100%;
}

/* 禁用 xterm 的 focus 样式 */
:deep(.xterm:focus) {
  outline: none;
}

:deep(.xterm-focus) {
  outline: none;
}

/* 回到底部按钮 */
.scroll-to-bottom-btn {
  position: absolute;
  bottom: 80px;
  right: 16px;
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 8px 12px;
  background: rgba(0, 0, 0, 0.7);
  color: white;
  border-radius: 20px;
  font-size: 12px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
  z-index: 10;
  transition: transform 0.2s ease, opacity 0.2s ease;
}

.scroll-to-bottom-btn:active {
  transform: scale(0.95);
}

:global(.dark) .scroll-to-bottom-btn {
  background: rgba(255, 255, 255, 0.2);
}

/* 过渡动画 */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>