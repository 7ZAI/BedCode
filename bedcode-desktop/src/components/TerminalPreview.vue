<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- Header（终端窗口模式下隐藏，由外层统一管理） -->
    <header
      v-if="showHeader"
      class="px-4 py-3 flex items-center justify-between border-b border-[var(--border)] bg-[var(--bg-card)]"
    >
      <div class="flex items-center gap-3 min-w-0">
        <div :class="['w-2 h-2 rounded-full shrink-0', statusColor]"></div>
        <h3 class="font-medium text-[var(--text-primary)] truncate">
          {{ session?.name || $t('desktop.terminal.defaultName') }}
        </h3>
      </div>

      <div class="flex items-center gap-2">
        <Select v-model="terminalTheme" :options="themeSelectOptions" size="sm" :title="$t('desktop.terminal.theme')" />
        <Select v-model="fontSize" :options="fontSizeSelectOptions" size="sm" :title="$t('desktop.terminal.fontSize')" />
        <Button variant="ghost" size="sm" @click="clearTerminal" :title="$t('desktop.terminal.clearScreen')">
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </Button>
        <Button variant="ghost" size="sm" @click="refreshTerminal" :title="$t('desktop.terminal.refreshFormat')">
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
        </Button>
        <PluginTerminalToolbar />
      </div>
    </header>

    <!-- 终端主体：xterm 挂载点（唯一渲染宿主） + 背景图片层 + 滚动到底指示器 -->
    <div
      ref="terminalHostRef"
      class="relative flex-1 min-h-0 overflow-hidden"
      :style="{ backgroundColor: containerBgColor }"
    >
      <!-- 终端背景图片层：渲染在 xterm 画布下方，不透明度由设置控制；
           铺满容器（cover + center），窗口调整大小时背景自适应缩放 -->
      <div
        v-if="bgImageUrl"
        class="absolute inset-0 z-0 pointer-events-none"
        :style="{
          backgroundImage: `url('${bgImageUrl}')`,
          backgroundSize: 'cover',
          backgroundPosition: 'center',
          backgroundRepeat: 'no-repeat',
          opacity: bgOpacity / 100,
        }"
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

      <!-- 输入导航条：右侧悬浮，默认透明仅横线，hover 展开列表，点击滚动到对应输入 -->
      <TerminalInputRail
        :markers="visibleMarkers"
        :buffer-length="bufferLength"
        :is-alt-buffer="isAltBuffer"
        @navigate="handleNavigate"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 终端预览组件 — 桌面端终端渲染内核（xterm.js）
 *
 * 设计目标：VS Code 终端体验 — 输出渲染正确无重影、滚动流畅、高吞吐性能。
 * 分层职责：
 * - 写入管线：实时输出合并为单次 write（DEC 2026 同步输出包裹），
 *   渲染器缓存所有变更到下一帧统一绘制，避免逐块绘制的撕裂/重影
 * - 渲染：WebGL addon（context loss 自动回退），滚动/重绘完全交给
 *   xterm 渲染循环，不做手动全量 refresh 补丁
 * - 滚动：onScroll 仅驱动"是否在底部"状态，scrollToBottom 经 rAF 合并，
 *   同一帧内多次输出只滚动一次
 * - 尺寸：ResizeObserver + rAF 节流 fit，cols/rows 实际变化才同步 PTY
 *
 * 终端窗口模式（TerminalWindowView）下 show-header=false，工具栏由外层
 * 统一管理；本组件仅通过 defineExpose 暴露主题/字号/清屏/刷新等能力。
 */
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import type { SessionInfo } from '@/stores/session'
import { useSessionStore } from '@/stores/session'
import { useSettingsStore } from '@/stores/settings'
import { useToast } from '@/composables/useToast'
import Button from '@/components/Button.vue'
import { Select } from '@/components'
import PluginTerminalToolbar from '@/plugin/components/PluginTerminalToolbar.vue'
import { useTerminalOutputStream } from '@/composables/useTerminalOutputStream'
import { useTerminalInputMarkers } from '@/composables/useTerminalInputMarkers'
import TerminalInputRail from '@bedcode/plugin-sdk-desktop/ui/terminal-input-rail'
import { TERMINAL_SCROLLBACK } from '@/utils/terminalScrollback'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { WebglAddon } from '@xterm/addon-webgl'
import { on as pluginEventOn, emit as pluginEventEmit, clearPluginEvents } from '@/plugin/events'
import { invoke } from '@tauri-apps/api/core'
import '@xterm/xterm/css/xterm.css'

/** Rust 端本地 WS 订阅裁决（服务端基于真源裁决，消费者零猜测） */
interface SubscribeControl {
  mode: string
  minOffset: number
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
const terminalHostRef = ref<HTMLElement | null>(null)
const fontSize = ref(settingsStore.settings.ui.terminal_font_size)
const terminalTheme = ref<string>(settingsStore.settings.ui.terminal_theme || 'dracula')

// 背景图片：设置中存原始文件名（仅用于判断是否启用与回显），
// 实际图片由本地服务器 /static/terminal-bg 端点提供
const bgImage = ref<string>(settingsStore.settings.ui.terminal_bg_image || '')
const bgOpacity = ref<number>(settingsStore.settings.ui.terminal_bg_opacity ?? 30)
const bgImageUrl = ref('')

// DEC Mode 2026 同步输出：包裹一次写入，让 xterm 缓存所有变化到下一帧
// 统一渲染，避免 WebGL 渲染器逐块绘制产生的视觉撕裂/重影（预编码为字节，
// 与写入管线统一为 Uint8Array，避免字符串中间态）
const SYNC_OUTPUT_START = new TextEncoder().encode('\x1b[?2026h')
const SYNC_OUTPUT_END = new TextEncoder().encode('\x1b[?2026l')

// ==================== xterm 实例 ====================

let terminal: Terminal | null = null
let fitAddon: FitAddon | null = null
let webglAddon: WebglAddon | null = null
let resizeObserver: ResizeObserver | null = null
let resizeRaf = 0

// 滚动状态追踪
const isUserScrolling = ref(false)

// rAF 节流：同一帧内多次 scrollToBottom 调用只执行一次
let pendingScrollRaf = 0

// xterm onScroll 取消监听（IDisposable 接口）
let scrollDisposable: import('@xterm/xterm').IDisposable | null = null

// 选区状态：有选区时 Ctrl+C 应复制而非发送中断（VS Code 终端行为）
let hasSelection = false

// Ctrl+滚轮缩放监听（passive:false），卸载时移除
let wheelHandler: ((e: WheelEvent) => void) | null = null

// 追踪当前行输入（MVP：仅追踪可打印字符和退格，供 AI 插件读取）
let currentLineBuffer = ''

// 输入导航条数据：每次回车提交记录一条输入标记（供右侧 TerminalInputRail 渲染横线）
const inputMarkers = useTerminalInputMarkers()
// 模板顶层绑定：ComputedRef 在模板中自动 unwrap
const { visibleMarkers } = inputMarkers
// 导航条位置计算依赖的 buffer 总行数 / alternate buffer 状态（输出解析后更新）
const bufferLength = ref(0)
const isAltBuffer = ref(false)

const sessionId = computed(() => props.session?.id || '')

// ==================== 本地 WS 二进制输出流 ====================
// 单一通道（历史回放 + 实时推送），字节游标连续性由 composable 守护：
// - onData：游标校验通过后的原始字节帧，直接入 rAF 写入管线（无去重/无补序）
// - onReset：服务端裁决 reset（环形头部淘汰/流重建），清屏后回放帧从 minOffset 起重播
// - onTruncated：min_offset > 0 说明会话开头输出已不可恢复，提示用户

// ==================== 写入管线 ====================
// 实时输出合并：同一渲染帧内的多个输出事件合并为一次 write（2026 包裹），
// 渲染器只刷新一次，高频输出（spinner/进度条/日志洪流）时吞吐显著提升。
// 为什么用 rAF 而不是 queueMicrotask：Tauri 事件每个都是独立 macrotask，
// 微任务会在每个事件后立即 flush，无法跨事件合并；rAF 才能把同一帧内
// 到达的所有事件合为一次 write。窗口最小化时 rAF 暂停，由兜底定时器保证
// 队列最终被清空。

let writeQueue: Uint8Array[] = []
let writeQueueBytes = 0
let flushRaf = 0
let flushTimer: ReturnType<typeof setTimeout> | null = null

// 单次 write 上限：超过则拆块，让 xterm parser 在块间让出主线程，
// 避免单帧解析超大字符串导致 UI 卡顿
const MAX_WRITE_CHUNK = 64 * 1024

function flushWriteQueue() {
  flushRaf = 0
  if (flushTimer) {
    clearTimeout(flushTimer)
    flushTimer = null
  }
  if (!terminal) {
    // 终端未就绪：丢弃（数据已进全局缓存，可从历史恢复）
    writeQueue = []
    writeQueueBytes = 0
    return
  }
  if (writeQueue.length === 0) return
  const chunks = writeQueue
  const totalBytes = writeQueueBytes
  writeQueue = []
  writeQueueBytes = 0

  // 合并同帧所有事件为单块字节，一次 write
  const combined = new Uint8Array(totalBytes)
  let offset = 0
  for (const chunk of chunks) {
    combined.set(chunk, offset)
    offset += chunk.byteLength
  }

  if (totalBytes <= MAX_WRITE_CHUNK) {
    terminal.write(wrapSyncOutput(combined))
    return
  }
  // 大块拆分为多次 write（2026 包裹整体）：渲染器仍缓存变更到帧末统一绘制；
  // subarray 零拷贝切片，避免大块复制
  terminal.write(SYNC_OUTPUT_START)
  for (let i = 0; i < combined.length; i += MAX_WRITE_CHUNK) {
    terminal.write(combined.subarray(i, i + MAX_WRITE_CHUNK))
  }
  terminal.write(SYNC_OUTPUT_END)
}

/** 用 DEC Mode 2026 同步输出序列包裹字节数据 */
function wrapSyncOutput(data: Uint8Array): Uint8Array {
  const wrapped = new Uint8Array(SYNC_OUTPUT_START.length + data.byteLength + SYNC_OUTPUT_END.length)
  wrapped.set(SYNC_OUTPUT_START, 0)
  wrapped.set(data, SYNC_OUTPUT_START.length)
  wrapped.set(SYNC_OUTPUT_END, SYNC_OUTPUT_START.length + data.byteLength)
  return wrapped
}

/** 入队输出：合并到下一渲染帧统一写入 */
function enqueueOutput(data: Uint8Array) {
  if (data.length === 0) return
  writeQueue.push(data)
  writeQueueBytes += data.byteLength
  if (flushRaf) return
  // rAF 合并同帧事件；100ms 兜底：窗口最小化（rAF 暂停）时也能及时清空队列
  flushRaf = requestAnimationFrame(flushWriteQueue)
  if (!flushTimer) {
    flushTimer = setTimeout(() => {
      flushTimer = null
      if (flushRaf) {
        cancelAnimationFrame(flushRaf)
        flushRaf = 0
      }
      flushWriteQueue()
    }, 100)
  }
}

// 本地 WS 输出流：
// - onData 帧已通过字节级连续性校验，与游标无缝衔接，直接写入（无去重）
// - onReset 时清屏：回放帧随后从 minOffset 流式写入，重建自洽帧
// - onTruncated 保留原有"历史被截断"提示 UX（触发条件从 minSeq > 0 改为 minOffset > 0）
const terminalStream = useTerminalOutputStream({
  onData: ({ data }) => {
    enqueueOutput(data)
    if (!isUserScrolling.value) {
      scrollToBottom()
    }
  },
  onReset: (_control: SubscribeControl) => {
    if (terminal) {
      terminal.clear()
    }
    // 流重置（清屏重播）：输入位置坐标失效，清除导航条标记
    inputMarkers.clear()
  },
  onTruncated: (minOffset: number) => {
    console.warn(`[TerminalPreview] 终端历史已被环形缓冲截断：minOffset=${minOffset}，会话开头输出不可用`)
    toast.warning(t('desktop.terminal.historyTruncated'))
  },
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

// 主题/字号下拉选项：与原生 <option> 一一对应，供共享 Select 使用
const themeSelectOptions = computed(() =>
  Object.entries(themeNames).map(([value, label]) => ({ value, label })),
)
const fontSizeSelectOptions = computed(() =>
  [8, 10, 12, 14, 16, 18, 20].map(size => ({ value: size, label: `${size}px` })),
)

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

// ==================== 初始化 ====================

/** WebGL 渲染器：加载并处理上下文丢失（丢失时回退 DOM 渲染，1s 后尝试重建） */
function initWebGL(term: Terminal): boolean {
  try {
    webglAddon = new WebglAddon()
    webglAddon.onContextLoss(() => {
      console.warn('[TerminalPreview] WebGL context lost, attempting recovery')
      webglAddon?.dispose()
      webglAddon = null
      // 上下文丢失时恢复 DOM 光标
      term.element?.classList.remove('xterm-hidden-cursor')
      // 延迟 1s 后尝试重新创建 WebGL 渲染器
      setTimeout(() => {
        if (!term || webglAddon) return
        try {
          const newAddon = new WebglAddon()
          newAddon.onContextLoss(() => {
            console.warn('[TerminalPreview] WebGL context lost again')
            newAddon.dispose()
            if (webglAddon === newAddon) webglAddon = null
            term.element?.classList.remove('xterm-hidden-cursor')
          })
          term.loadAddon(newAddon)
          webglAddon = newAddon
          // 恢复后重新隐藏 DOM 光标
          term.element?.classList.add('xterm-hidden-cursor')
          console.info('[TerminalPreview] WebGL context recovered')
        } catch (e) {
          console.warn('[TerminalPreview] WebGL recovery failed, using canvas fallback:', e)
          webglAddon = null
        }
      }, 1000)
    })
    term.loadAddon(webglAddon)
    return true
  } catch (e) {
    console.warn('[TerminalPreview] WebGL not supported:', e)
    webglAddon = null
    return false
  }
}

function initTerminal() {
  if (!terminalHostRef.value) return

  terminal = new Terminal({
    // 字体与尺寸
    fontSize: fontSize.value,
    // VS Code 终端默认字体（Windows 11 自带），其后为跨平台回退
    fontFamily: 'Cascadia Mono, Consolas, Monaco, Courier New, monospace',
    lineHeight: 1,
    // 滚动历史行数（与后端事件队列容量对齐）
    scrollback: TERMINAL_SCROLLBACK,
    // 即时滚动：关闭平滑滚动，避免 WebGL 滚动动画期间合成器缓存旧帧导致重影
    smoothScrollDuration: 0,
    // 光标统一不显示（见下方 DECTCEM 隐藏）；此处配置为 VS Code 风格的
    // 块光标 + 不闪烁，作为未来恢复光标时的合理默认
    cursorBlink: false,
    cursorStyle: 'block',
    cursorWidth: 1,
    // 交互：与 VS Code 终端一致
    rightClickSelectsWord: true,
    altClickMovesCursor: true,
    drawBoldTextInBrightColors: true,
    // 主题
    theme: getTheme(),
    // 允许背景透明：必须在 open() 前设置，否则渲染器会把 rgba 背景强制转为不透明，
    // 导致背景图片层被终端背景色遮盖
    allowTransparency: true,
    allowProposedApi: true,
  })

  fitAddon = new FitAddon()
  terminal.loadAddon(fitAddon)
  terminal.loadAddon(new WebLinksAddon())
  terminal.open(terminalHostRef.value)
  initWebGL(terminal)

  // 移除光标：用 DECTCEM 隐藏序列（\x1b[?25l）在 buffer 层隐藏光标，
  // WebGL 与 DOM 渲染器均不再绘制（TUI 程序主动发送 \x1b[?25h 时除外）
  terminal.write('\x1b[?25l')

  // WebGL 渲染器激活后，隐藏 DOM 层光标避免双光标问题
  // 只隐藏 DOM 层，保留 WebGL 层光标（WebGL 光标更流畅且不会出现双光标）
  if (webglAddon) {
    terminal.element?.classList.add('xterm-hidden-cursor')
  }

  fitAddon.fit()
  syncTerminalSize()

  // PTY 尺寸同步：xterm 内部 resize（含 fit 触发）时同步到后端会话
  terminal.onResize(({ cols, rows }) => {
    if (props.session) {
      sessionStore.resizeSession(props.session.id, cols, rows)
    }
    // resize 改变 rows → buffer 总行数变化，刷新导航条位置分母
    bufferLength.value = terminal?.buffer.active.length ?? 0
  })

  // ResizeObserver — rAF 节流，避免快速连续 fit 导致的重复渲染；
  // 仅当 cols/rows 实际变化时同步 PTY（xterm 自身负责重绘）
  resizeObserver = new ResizeObserver(() => {
    if (resizeRaf) return
    resizeRaf = requestAnimationFrame(() => {
      resizeRaf = 0
      if (!fitAddon || !terminal) return
      const cols = terminal.cols
      const rows = terminal.rows
      fitAddon.fit()
      if (terminal.cols !== cols || terminal.rows !== rows) {
        syncTerminalSize()
      }
    })
  })
  resizeObserver.observe(terminalHostRef.value)

  // 滚动状态：xterm onScroll API（比 DOM addEventListener 更可靠，
  // 不会因 xterm 内部 DOM 重建而丢失监听）；仅更新"是否在底部"状态，
  // 重绘完全交给 xterm 渲染循环，不做手动 refresh 补丁
  scrollDisposable = terminal.onScroll(() => {
    if (!terminal) return
    const buffer = terminal.buffer.active
    const viewportBottom = buffer.viewportY + terminal.rows
    isUserScrolling.value = viewportBottom < buffer.length - 1
  })

  // buffer 变化（输出解析完成）：刷新导航条的总行数与 alternate buffer 状态；
  // onWriteParsed 在每次 write 解析完成后触发，覆盖输出/清屏/TUI 切换全部场景
  terminal.onWriteParsed(() => {
    if (!terminal) return
    bufferLength.value = terminal.buffer.active.length
    isAltBuffer.value = terminal.buffer.active.type === 'alternate'
  })

  // 选区状态跟踪：有选区时 Ctrl+C 复制（VS Code 终端行为），不发送 SIGINT
  terminal.onSelectionChange(() => {
    hasSelection = !!terminal?.getSelection()
  })

  // Ctrl+滚轮缩放字号（VS Code 终端行为）；passive:false 才能阻止默认滚动
  wheelHandler = (e: WheelEvent) => {
    if (!e.ctrlKey) return
    e.preventDefault()
    const sizes = [8, 10, 12, 14, 16, 18, 20]
    const idx = sizes.indexOf(fontSize.value)
    const next = Math.min(
      sizes.length - 1,
      Math.max(0, idx < 0 ? 0 : idx + (e.deltaY < 0 ? 1 : -1)),
    )
    fontSize.value = sizes[next]
  }
  terminalHostRef.value.addEventListener('wheel', wheelHandler, { passive: false })

  // 键盘输入
  terminal.onData((data: string) => {
    if (!props.session) return

    // 有选区时 Ctrl+C 仅复制（VS Code 终端行为），不向 PTY 发送中断
    if (data === '\x03' && hasSelection) {
      const sel = terminal?.getSelection()
      if (sel) {
        navigator.clipboard?.writeText(sel).catch(() => {})
      }
      return
    }

    sessionStore.writeToSession(props.session.id, data)

    // 多行粘贴（一次事件含换行）：每行视为一次独立输入，逐行记录
    if (data.length > 1 && /[\r\n]/.test(data)) {
      for (const line of data.split(/\r\n|\r|\n/)) {
        if (line.length > 0) inputMarkers.record(terminal!, line)
      }
      currentLineBuffer = ''
      return
    }

    // 追踪当前行输入
    if (data === '\r' || data === '\n') {
      // 回车提交：先记录本次输入（供导航条），再清空追踪
      inputMarkers.record(terminal!, currentLineBuffer)
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

/** 同步当前终端尺寸到后端会话（PTY cols/rows） */
function syncTerminalSize() {
  if (!terminal || !props.session) return
  const cols = terminal.cols
  const rows = terminal.rows
  if (cols > 0 && rows > 0) {
    sessionStore.resizeSession(props.session.id, cols, rows)
  }
}

/** 刷新格式：重新 fit 终端尺寸并同步到 PTY，不清除内容 */
function refreshTerminal() {
  if (!fitAddon || !terminal || !props.session) return
  fitAddon.fit()
  syncTerminalSize()
}

/** 滚动到底：rAF 合并，同一帧内多次调用只执行一次 */
function scrollToBottom() {
  if (!pendingScrollRaf) {
    pendingScrollRaf = requestAnimationFrame(() => {
      pendingScrollRaf = 0
      terminal?.scrollToBottom()
    })
  }
}

/** 用户点击"回到底部"按钮：重置滚动状态并滚到底 */
function scrollToBottomManual() {
  isUserScrolling.value = false
  terminal?.scrollToBottom()
}

/** 导航条点击：滚动终端到指定 buffer 行（触发 onScroll → 自动显示"回到底部"指示器） */
function handleNavigate(line: number) {
  terminal?.scrollToLine(line)
}

function clearTerminal() {
  if (!terminal) return
  terminal.clear()
  // 清屏后历史输入位置全部失效，同步清除导航条标记
  inputMarkers.clear()
}

// ==================== 设置同步 ====================

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

// 会话变化
// 游标重置（新会话坐标空间独立），断开旧流并连接新流；
// 历史回放由服务端裁决后以二进制帧流式送达（无需 invoke 拉取）。
// 首次挂载（terminal 未就绪）只握手不订阅，订阅由 onMounted 触发
let streamMounted = false
watch(sessionId, async (newId, oldId) => {
  if (newId !== oldId) {
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

      terminalStream.start(newId)
      if (streamMounted) {
        terminalStream.subscribe()
      }
    } else {
      terminalStream.stop()
    }
  }
}, { immediate: true })

// 会话状态变化：停止/出错时断开输出流；重新运行时恢复订阅
watch(() => props.session?.status, (status) => {
  if (!sessionId.value) return
  if (status === 'stopped' || status === 'error') {
    terminalStream.stop()
  } else if (status === 'running') {
    terminalStream.start(sessionId.value)
    terminalStream.subscribe()
  }
})

onMounted(async () => {
  await nextTick()

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

  // terminal 就绪后启动本地 WS 输出流：历史回放 + 实时推送同通道流式到达
  terminalStream.start(sessionId.value)
  terminalStream.subscribe()
  streamMounted = true

  terminal?.focus()
})

onUnmounted(() => {
  // 断开本地 WS 输出流（停止重连）
  terminalStream.stop()

  // 清理 AI 插件事件监听
  clearPluginEvents('__host__')

  // 清理输入导航条标记（dispose 全部 xterm marker）
  inputMarkers.clear()

  // 清理 xterm onScroll 监听
  if (scrollDisposable) {
    scrollDisposable.dispose()
    scrollDisposable = null
  }

  // 清理 Ctrl+滚轮缩放监听
  if (wheelHandler && terminalHostRef.value) {
    terminalHostRef.value.removeEventListener('wheel', wheelHandler)
    wheelHandler = null
  }

  // 清理待处理的滚动 rAF
  if (pendingScrollRaf) {
    cancelAnimationFrame(pendingScrollRaf)
    pendingScrollRaf = 0
  }

  // 清理写入队列（未 flush 的数据仍存于服务端环形，重开窗口可恢复）
  if (flushRaf) {
    cancelAnimationFrame(flushRaf)
    flushRaf = 0
  }
  if (flushTimer) {
    clearTimeout(flushTimer)
    flushTimer = null
  }
  writeQueue.length = 0
  writeQueueBytes = 0

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
/* ==================== xterm 渲染层 ==================== */

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

/* xterm 6 中 .xterm-viewport 不承载滚动（内容高度=视口高度，滚动由
   .xterm-scrollable-element 的 JS 状态驱动），其原生滚动条永远满格且拖不动，
   会误导用户认为滚动失效。隐藏它，滚动条统一由 xterm 自绘 slider 提供。 */
:deep(.xterm-viewport)::-webkit-scrollbar {
  display: none;
}

/* xterm 自绘滚动条（.xterm-scrollable-element > .scrollbar）默认仅鼠标悬停
   时显示（VS Code 风格），且 slider 高度可能只有最小保护值，深色主题下几乎
   不可见。强制常显，让用户能发现并拖动真正的滚动条。 */
:deep(.xterm .xterm-scrollable-element > .scrollbar.vertical) {
  opacity: 1 !important;
  transition: none;
}

/* WebGL 模式下隐藏 DOM 层光标（双光标防御）。
   光标已通过 DECTCEM（\x1b[?25l）在 buffer 层移除，此规则作为渲染层
   冗余保护：若未来恢复光标显示，WebGL 与 DOM 层不会同时绘制 */
:deep(.xterm-hidden-cursor .xterm-cursor) {
  display: none !important;
}

/* ==================== 滚动到底指示器 ==================== */

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
