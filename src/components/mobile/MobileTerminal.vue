<template>
  <div ref="terminalContainerRef" class="h-full w-full overflow-hidden"></div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import { useSettingsStore } from '@/stores/settings'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import '@xterm/xterm/css/xterm.css'

const settingsStore = useSettingsStore()
const terminalContainerRef = ref<HTMLElement | null>(null)

// xterm.js 实例
let terminal: Terminal | null = null
let fitAddon: FitAddon | null = null
let lastOutputIndex = 0

const props = defineProps<{
  output?: string
}>()

const emit = defineEmits<{
  ready: []
  clear: []
}>()

/** 初始化 xterm.js */
function initTerminal() {
  if (!terminalContainerRef.value) return

  // 确保容器宽度为 100%
  terminalContainerRef.value.style.width = '100%'

  const fontSize = settingsStore.settings.ui.terminal_font_size || 14
  // 移动端优先使用等宽字体
  const fontFamily = "'SF Mono', 'Fira Code', Consolas, Monaco, 'Courier New', monospace"

  terminal = new Terminal({
    fontSize,
    fontFamily,
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
    // 移动端减少滚动缓冲区以节省内存
    scrollback: 5000,
    allowProposedApi: true,
    // 禁用光标样式渲染优化
    cursorInactiveStyle: 'none',
  })

  fitAddon = new FitAddon()
  terminal.loadAddon(fitAddon)
  terminal.loadAddon(new WebLinksAddon())
  terminal.open(terminalContainerRef.value)

  // 移动端适配：fit 后等待 DOM 完成，使用防抖
  let fitTimeout: ReturnType<typeof setTimeout> | null = null
  const debouncedFit = () => {
    if (fitTimeout) clearTimeout(fitTimeout)
    fitTimeout = setTimeout(() => {
      if (fitAddon) fitAddon.fit()
    }, 100)
  }
  debouncedFit()

  // 监听窗口大小变化，使用防抖避免频繁 fit
  const resizeObserver = new ResizeObserver(() => {
    debouncedFit()
  })
  resizeObserver.observe(terminalContainerRef.value)

  emit('ready')
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
  const viewport = terminalContainerRef.value?.querySelector('.xterm-viewport') as HTMLElement
  if (viewport) {
    viewport.scrollTop = viewport.scrollHeight
  }
}

// 监听输出变化，增量写入
watch(() => props.output, (newOutput) => {
  if (!terminal || !newOutput) return

  // 只写入新增的部分（增量写入）
  for (let i = lastOutputIndex; i < newOutput.length; i++) {
    terminal!.write(newOutput[i])
  }
  lastOutputIndex = newOutput.length

  // 自动滚动到底部
  scrollToBottom()
}, { deep: true })

// 监听字体大小变化
watch(() => settingsStore.settings.ui.terminal_font_size, (newSize) => {
  if (!terminal) return
  terminal.options.fontSize = newSize
  if (fitAddon) {
    fitAddon.fit()
  }
})

onMounted(() => {
  initTerminal()
})

onUnmounted(() => {
  if (terminal) {
    terminal.dispose()
    terminal = null
  }
})

defineExpose({
  write,
  clear,
  scrollToBottom,
})
</script>

<style scoped>
:deep(.xterm) {
  height: 100%;
  padding: 0;
}

:deep(.xterm-viewport) {
  border-radius: 0;
  overflow-y: auto !important;
  overflow-x: hidden;
}

:deep(.xterm-screen) {
  height: 100%;
}
</style>