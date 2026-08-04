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
    <div ref="terminalContainerRef" class="flex-1 overflow-hidden relative" :style="{ backgroundColor: containerBgColor }">
      <!-- 终端背景图片层：渲染在 xterm 画布下方，不透明度由设置控制 -->
      <div
        v-if="bgImageUrl"
        class="absolute inset-0 pointer-events-none bg-cover bg-center"
        :style="{ backgroundImage: `url('${bgImageUrl}')`, opacity: bgOpacity / 100 }"
      ></div>
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
import { useI18n } from 'vue-i18n'
import type { SessionInfo } from '@/stores/session'
import { useSessionStore } from '@/stores/session'
import { useSettingsStore } from '@/stores/settings'
import { useToast } from '@/composables/useToast'
import Button from '@/components/Button.vue'
import PluginTerminalToolbar from '@/plugin/components/PluginTerminalToolbar.vue'
import { usePtyOutput } from '@/composables/usePtyOutput'
import {
  useTerminalHistory,
  initSessionCache,
  destroySessionCache,
  resizeHiddenTerminal
} from '@/composables/useGlobalTerminal'
import { pendingReplayEvents, advanceWatermark } from '@/utils/ptyReplay'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { WebglAddon } from '@xterm/addon-webgl'
import { on as pluginEventOn, emit as pluginEventEmit, clearPluginEvents } from '@/plugin/events'
import { invoke } from '@tauri-apps/api/core'
import '@xterm/xterm/css/xterm.css'

/** Rust 端历史回放响应 */
interface OutputHistoryResponse {
  minSeq: number
  maxSeq: number
  events: Array<{
    sessionId: string
    data: string      // Base64 编码
    index: number
    timestamp: string
    isWaiting: boolean
  }>
}

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

const { t } = useI18n()
const toast = useToast()

const sessionStore = useSessionStore()
const settingsStore = useSettingsStore()
const terminalContainerRef = ref<HTMLElement | null>(null)
const fontSize = ref(settingsStore.settings.ui.terminal_font_size)
const terminalTheme = ref<string>(settingsStore.settings.ui.terminal_theme || 'dracula')

// 背景图片：设置中存原始文件名（仅用于判断是否启用与回显），
// 实际图片由本地服务器 /static/terminal-bg 端点提供
const bgImage = ref<string>(settingsStore.settings.ui.terminal_bg_image || '')
const bgOpacity = ref<number>(settingsStore.settings.ui.terminal_bg_opacity ?? 30)
const bgImageUrl = ref('')

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

// 滚动后强制重绘可见区：清除 WebGL 渲染器滚动遗留的重影纹理行
let pendingScrollRefreshRaf = 0

// xterm onScroll 取消监听（IDisposable 接口）
let scrollDisposable: import('@xterm/xterm').IDisposable | null = null

// 追踪当前行输入（MVP：仅追踪可打印字符和退格，供 AI 插件读取）
let currentLineBuffer = ''

const sessionId = computed(() => props.session?.id || '')

// 终端历史缓存（传入 computed ref，会话切换时自动更新目标）
const terminalHistory = useTerminalHistory(sessionId)

// 历史回放去重：记录已回放的最大 index，实时事件 index <= 此值时忽略
let lastReplayedIndex = 0

/** 解码 Base64 编码的 PTY 输出数据为 UTF-8 字符串 */
function decodeBase64(base64: string): string {
  try {
    const binaryString = atob(base64)
    const bytes = new Uint8Array(binaryString.length)
    for (let i = 0; i < binaryString.length; i++) {
      bytes[i] = binaryString.charCodeAt(i)
    }
    return new TextDecoder('utf-8', { fatal: false }).decode(bytes)
  } catch (e) {
    console.error('[TerminalPreview] Failed to decode base64:', e)
    return base64
  }
}

// PTY 输出监听：增量回调模式，每次输出直接写入 xterm + 全局缓存
// 严格去重：忽略 index <= lastReplayedIndex 的事件（已在历史回放中写入）
usePtyOutput(sessionId, (data: string, index: number) => {
  if (index <= lastReplayedIndex) return

  // 始终写入全局缓存，即使 terminal 未初始化（数据可从历史恢复）
  terminalHistory.append(data)

  if (terminal) {
    terminal.write(data)
    // 只在实际写入终端时推进水位：历史回放会跳过 <= 水位的重叠事件，
    // 避免窗口打开时同一批输出被实时流与历史回放各写一次（重复行）
    lastReplayedIndex = advanceWatermark(lastReplayedIndex, index)
    if (!isUserScrolling.value) {
      scrollToBottom()
    }
  }
})

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
  const base = terminalThemes[terminalTheme.value] || terminalThemes.default
  // 背景图片启用时终端背景设为全透明，让图片层透出
  if (bgImageUrl.value) {
    return { ...base, background: 'rgba(0, 0, 0, 0)' }
  }
  return base
}

/** 终端容器底色：背景图片启用时 xterm 背景透明，由容器补上主题背景色 */
const containerBgColor = computed(() => {
  const base = terminalThemes[terminalTheme.value] || terminalThemes.default
  return (base as { background: string }).background
})

/** 解析背景图片 URL：本地服务器静态端点提供图片（先查实际运行端口，?t= 时间戳防缓存） */
async function resolveBgImageUrl() {
  if (!bgImage.value) {
    bgImageUrl.value = ''
    return
  }
  try {
    const status = await invoke<{ port: number }>('get_server_status')
    // 端口为 0 表示服务器尚未启动，回退到配置端口（服务器可能稍后启动）
    const port = status.port || settingsStore.settings.network.port
    const url = `http://127.0.0.1:${port}/static/terminal-bg?t=${Date.now()}`
    // 预加载校验：图片不可达（服务器未启动/404 等）时不启用透明主题，
    // 避免终端背景已切为全透明、图片却加载不出来，看起来像丢失了背景色
    await new Promise<void>((resolve, reject) => {
      const probe = new Image()
      probe.onload = () => resolve()
      probe.onerror = () => reject(new Error(`background image not loadable: ${url}`))
      probe.src = url
    })
    bgImageUrl.value = url
  } catch (e) {
    console.error('[TerminalPreview] Failed to resolve background image URL:', e)
    bgImageUrl.value = ''
  }
}

// 外部设置变化同步背景图片配置
watch(() => settingsStore.settings.ui.terminal_bg_image, (v) => {
  bgImage.value = v || ''
})
watch(() => settingsStore.settings.ui.terminal_bg_opacity, (v) => {
  if (v != null) bgOpacity.value = v
})

// 背景图片变化：重新解析 URL 并刷新终端主题（透明/不透明切换）
watch(bgImage, () => {
  resolveBgImageUrl()
})
watch([bgImageUrl, bgOpacity], () => {
  if (terminal) {
    terminal.options.theme = getTheme()
  }
})

function initWebGL(terminal: Terminal): boolean {
  try {
    webglAddon = new WebglAddon()
    webglAddon.onContextLoss(() => {
      console.warn('[TerminalPreview] WebGL context lost, attempting recovery')
      webglAddon?.dispose()
      webglAddon = null
      // 上下文丢失时恢复 DOM 光标
      terminal.element?.classList.remove('xterm-hidden-cursor')
      // 延迟 1s 后尝试重新创建 WebGL 渲染器
      setTimeout(() => {
        if (!terminal || webglAddon) return
        try {
          const newAddon = new WebglAddon()
          newAddon.onContextLoss(() => {
            console.warn('[TerminalPreview] WebGL context lost again')
            newAddon.dispose()
            if (webglAddon === newAddon) webglAddon = null
            terminal.element?.classList.remove('xterm-hidden-cursor')
          })
          terminal.loadAddon(newAddon)
          webglAddon = newAddon
          // 恢复后重新隐藏 DOM 光标
          terminal.element?.classList.add('xterm-hidden-cursor')
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
    // 允许背景透明：必须在 open() 前设置，否则渲染器会把 rgba 背景强制转为不透明，
    // 导致背景图片层被终端背景色遮盖
    allowTransparency: true,
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

  // 滚动事件：使用 xterm onScroll API，比 DOM addEventListener 更可靠
  // 不会因 xterm 内部 DOM 重建而丢失监听
  scrollDisposable = terminal.onScroll(() => handleScroll())

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
  // 使用 xterm.js buffer 判断是否在底部
  if (!terminal) return
  const buffer = terminal.buffer.active
  const viewportTop = buffer.viewportY
  const viewportBottom = viewportTop + terminal.rows
  const totalLines = buffer.length
  isUserScrolling.value = viewportBottom < totalLines - 1

  // WebGL 渲染器在滚动时可能残留上一帧的纹理行（重影）。
  // 每帧至多一次强制重绘可见区，从 buffer 重新生成，清除残留。
  if (!pendingScrollRefreshRaf) {
    pendingScrollRefreshRaf = requestAnimationFrame(() => {
      pendingScrollRefreshRaf = 0
      terminal?.refresh(0, terminal.rows - 1)
    })
  }
}

/// 用户点击"回到底部"按钮：重置滚动状态并滚到底
function scrollToBottomManual() {
  isUserScrolling.value = false
  terminal?.scrollToBottom()
}

function clearTerminal() {
  if (!terminal) return
  terminal.clear()
  terminalHistory.clear()
}

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
    // 切换会话时重置去重状态
    lastReplayedIndex = 0

    if (oldId) {
      clearTerminal()
    }

    if (newId) {
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

  // 初始化背景图片（在 initTerminal 之后，仅影响后续主题刷新；
  // 首次挂载时若已有背景图，通过一次主题刷新生效）
  await resolveBgImageUrl()
  if (terminal) {
    terminal.options.theme = getTheme()
  }

  // 监听 AI 插件请求当前终端输入
  pluginEventOn('__host__', 'ai-chatbox:getCurrentInput', () => {
    pluginEventEmit('ai-chatbox:currentInput', { sessionId: sessionId.value, text: currentLineBuffer })
  })

  // 显示终端 fit 后，同步隐藏终端尺寸
  if (terminal && sessionId.value) {
    resizeHiddenTerminal(sessionId.value, terminal.cols, terminal.rows)
  }

  // 从 Rust 端获取历史输出（覆盖窗口关闭期间丢失的数据）
  if (terminal && sessionId.value) {
    try {
      const history = await invoke<OutputHistoryResponse>('get_session_output_history', {
        sessionId: sessionId.value,
        startSeq: null,
      })

      // 环形缓冲回卷检测：minSeq > 0 说明会话开头输出已被环形缓冲淘汰，
      // 当前历史不完整（最早输出不可恢复），提示用户而非静默缺失
      if (history.minSeq > 0) {
        console.warn(`[TerminalPreview] 终端历史已被环形缓冲截断：minSeq=${history.minSeq}，会话开头输出不可用`)
        toast.warning(t('desktop.terminal.historyTruncated'))
      }

      if (history.events.length > 0) {
        // 逐个事件解码并写入 xterm + 全局缓存
        // 不合并为单次写入，避免隐藏 xterm 实例处理超长字符串时卡顿
        // 跳过已由实时流写入的重叠事件（index <= 水位），避免重复行
        const { events: pending, nextWatermark } = pendingReplayEvents(history.events, lastReplayedIndex)
        for (const event of pending) {
          const data = decodeBase64(event.data)
          terminal.write(data)
          terminalHistory.append(data)
          lastReplayedIndex = advanceWatermark(lastReplayedIndex, event.index)
        }
        // 水位不倒退（实时流可能已推进到更高 index）
        lastReplayedIndex = Math.max(lastReplayedIndex, nextWatermark, history.maxSeq)
        scrollToBottom()
      }
    } catch (e) {
      console.error('[TerminalPreview] Failed to get output history:', e)
      // 回放失败时回退到全局缓存
      const cachedHistory = terminalHistory.getHistory()
      if (cachedHistory) {
        terminal.write(cachedHistory)
        scrollToBottom()
      }
    }
  } else if (terminal && sessionId.value) {
    // 无 Rust 端历史时，从全局缓存恢复
    const history = terminalHistory.getHistory()
    if (history) {
      terminal.write(history)
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

  // 清理 xterm onScroll 监听
  if (scrollDisposable) {
    scrollDisposable.dispose()
    scrollDisposable = null
  }

  // 清理待处理的滚动 rAF
  if (pendingScrollRaf) {
    cancelAnimationFrame(pendingScrollRaf)
    pendingScrollRaf = 0
  }

  // 清理滚动后重绘 rAF
  if (pendingScrollRefreshRaf) {
    cancelAnimationFrame(pendingScrollRefreshRaf)
    pendingScrollRefreshRaf = 0
  }

  // 清理 resize rAF
  if (resizeRaf) {
    cancelAnimationFrame(resizeRaf)
    resizeRaf = 0
  }

  // 清理设置保存定时器
  if (fontSizeSaveTimeout) {
    clearTimeout(fontSizeSaveTimeout)
    fontSizeSaveTimeout = null
  }
  if (themeSaveTimeout) {
    clearTimeout(themeSaveTimeout)
    themeSaveTimeout = null
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
  /* 保证 xterm 画布位于背景图片层之上 */
  position: relative;
  z-index: 1;
}

:deep(.xterm-viewport) {
  border-radius: 0;
  overflow-x: hidden;
}

/* xterm.css 默认为 .xterm-viewport 设置 background-color:#000（不透明黑）。
   xterm 6 中滚动已由 .xterm-scrollable-element 接管，但该元素仍是覆盖整个
   终端区域的定位层，位于背景图片层之上、渲染画布之下。置为透明后背景图片
   才能透出；未设置背景图片时主题背景色由画布/滚动层绘制，此覆盖无副作用。
   选择器带 .xterm 前缀，优先级高于 xterm.css 的 `.xterm .xterm-viewport`，
   不依赖样式表加载顺序。 */
:deep(.xterm .xterm-viewport) {
  background-color: transparent;
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
