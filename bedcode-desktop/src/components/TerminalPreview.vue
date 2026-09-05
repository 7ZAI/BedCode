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
        <Select
          v-model="terminalTheme"
          :options="themeSelectOptions"
          size="sm"
          :title="$t('desktop.terminal.theme')"
        />
        <Select
          v-model="fontSize"
          :options="fontSizeSelectOptions"
          size="sm"
          :title="$t('desktop.terminal.fontSize')"
        />
        <Button
          variant="ghost"
          size="sm"
          :title="$t('desktop.terminal.clearScreen')"
          @click="clearTerminal"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
            />
          </svg>
        </Button>
        <Button
          variant="ghost"
          size="sm"
          :title="$t('desktop.terminal.refreshFormat')"
          @click="refreshTerminal"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
            />
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
          :title="$t('desktop.terminal.scrollToBottom')"
          @click="scrollToBottomManual"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M19 14l-7 7m0 0l-7-7m7 7V3"
            />
          </svg>
        </button>
      </transition>
    </div>

    <!-- 正统渲染端覆盖确认弹窗：本端 resize 被服务端裁决为
         needsConfirmation（另一个端正在渲染输出）时弹出，确认后 force 重发 -->
    <Modal
      v-model="showRendererOverrideModal"
      :title="$t('desktop.terminal.rendererOverrideTitle')"
      size="sm"
      :closable="false"
      :close-on-backdrop="false"
    >
      <p class="text-sm text-[var(--text-secondary)] whitespace-pre-line">
        {{
          $t('desktop.terminal.rendererOverrideBody', {
            renderer: rendererOverrideTarget?.rendererName ?? '',
          })
        }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-2">
          <Button variant="ghost" size="sm" @click="cancelRendererOverride">
            {{ $t('desktop.terminal.rendererOverrideCancel') }}
          </Button>
          <Button size="sm" @click="confirmRendererOverride">
            {{ $t('desktop.terminal.rendererOverrideConfirm') }}
          </Button>
        </div>
      </template>
    </Modal>
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
 * - 尺寸：ResizeObserver + 分层防抖 fit（垂直立即 / 水平 100ms 合并；
 *   小缓冲 <200 行免防抖立即应用）+ DPR 感知行列计算，cols/rows 实际变化
 *   才全量重绘与同步 PTY；0 尺寸（最小化）忽略不缩 PTY，恢复可见时 flush
 *   一次兑现挂起防抖
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
import type { RendererSource } from '@/composables/useDesktopCommands'
import Button from '@/components/Button.vue'
import Modal from '@/components/Modal.vue'
import { Select } from '@/components'
import PluginTerminalToolbar from '@/plugin/components/PluginTerminalToolbar.vue'
import { useTerminalOutputStream } from '@/composables/useTerminalOutputStream'
import { TERMINAL_SCROLLBACK } from '@/utils/terminalScrollback'
import { TerminalResizeDebouncer } from '@/utils/terminalResizeDebouncer'
import { computeDesktopInitialTerminalSize } from '@/utils/terminalInitialSize'
import {
  shouldApplyGridResize,
  ATLAS_PREHEAT_DELAY_MS,
} from '@/utils/terminalResizePolicy'
import { getXtermScaledDimensions } from '@/utils/terminalDimensions'
import { TerminalImeStateMachine } from '@/utils/terminalImeStateMachine'
import { initPlatform } from '@/composables/usePlatform'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { WebglAddon } from '@xterm/addon-webgl'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import { on as pluginEventOn, emit as pluginEventEmit, clearPluginEvents } from '@/plugin/events'
import { invoke } from '@tauri-apps/api/core'
import '@xterm/xterm/css/xterm.css'

/** XP 会话本地 WS 订阅（快照模型，07）：min_seq/max_seq/history_count 元数据 + TB v2 帧 */

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

// Linux 平台优化：isLinux 在 onMounted 中 await initPlatform() 后确定（消除
// setup 时 platform 尚未解析的竞态）；仅 Linux 启用 IME 去重与专用字体栈
const isLinux = ref(false)
// Linux 专用等宽字体栈：优先系统自带、hint 较强的等宽字体（DejaVu Sans Mono /
// Liberation Mono 对 canvas fillText 的像素对齐更好，Ubuntu Mono 笔画偏软），确保
// WebKitGTK 用真实系统等宽字体渲染，避免默认栈（Cascadia Mono 等 Windows 字体）
// 在 Linux 上回退非等宽字体导致的字符间距过大/模糊；Windows/macOS 保持原有字体栈
const LINUX_FONT_STACK =
  "'DejaVu Sans Mono', 'Liberation Mono', 'Ubuntu Mono', 'Noto Sans Mono', 'Noto Mono', 'Cascadia Mono', 'Consolas', 'Courier New', monospace"
const DEFAULT_FONT_STACK = 'Cascadia Mono, Consolas, Monaco, Courier New, monospace'
// Linux WebKitGTK 终端渲染器选择：WebGL 字形图集在 dpr=1 时按 Math.floor(advance)
// 切格 + LINEAR 采样，文字比原生终端发蒙；DOM(canvas) 渲染器逐字形 fillText，
// 边缘更清晰（对齐 VS Code Linux 默认关 GPU 加速的做法）。置 false 可切回 WebGL 对比。
const LINUX_USE_DOM_RENDERER = true
// 背景图片：设置中存原始文件名（仅用于判断是否启用与回显），
// 实际图片由本地服务器 /static/terminal-bg 端点提供
const bgImage = ref<string>(settingsStore.settings.ui.terminal_bg_image || '')
const bgOpacity = ref<number>(settingsStore.settings.ui.terminal_bg_opacity ?? 30)
const bgImageUrl = ref('')

// ==================== xterm 实例 ====================

let terminal: Terminal | null = null
let fitAddon: FitAddon | null = null
// Linux 平台 IME 去重状态机（WebKitGTK 输入法双发去重，仅 Linux 启用；
// 事件信号由 onMounted 在终端初始化后挂接到 terminal.textarea）
let imeStateMachine: TerminalImeStateMachine | null = null
// WebGL resize 后字符图集重建的补刷定时器（atlas 预热：等 idle 分片光栅化
// 基本完成后补一次全量重绘，避免整屏字形缺失的“临时失明”）
let atlasPreheatTimer: ReturnType<typeof setTimeout> | null = null
let webglAddon: WebglAddon | null = null
let resizeObserver: ResizeObserver | null = null
// resize 分层防抖：垂直立即 / 水平 100ms 合并（对齐 VS Code TerminalResizeDebouncer）
let resizeDebouncer: TerminalResizeDebouncer | null = null
// 前台 flush 监听（对齐 VS Code 切回前台时对 debouncer 的 flush 语义）：
// 后台/最小化期间的尺寸变化挂起防抖，恢复可见时以最终尺寸一次性兑现
let windowVisibleHandler: (() => void) | null = null
// DPR 变化监听（跨屏拖动 / 系统缩放变化）：matchMedia 递归注册，卸载时移除
let dprMediaQuery: MediaQueryList | null = null
let dprChangeHandler: ((e: MediaQueryListEvent) => void) | null = null

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

const sessionId = computed(() => props.session?.id || '')

// ==================== 本地 WS 二进制输出流 ====================
// 单一通道（历史回放 + 实时推送），seq 级连续性由 composable 守护：
// - onData：去重/连续性校验通过后的原始字节帧，直接入 rAF 写入管线
// - onReset：快照重订阅遇到历史截断（min_seq > last_rendered_seq + 1）时清屏，
//   重播帧随后全量流式写入
// - onTruncated:min_seq > 0 说明会话开头输出已不可恢复，提示用户

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

// 回放静止补刷：订阅/重订阅后的历史回放与渲染器冷启动时序竞态，可能留下
// 未被后续帧覆盖的中间态（字符残缺/错位，用户需手动点刷新才恢复）。
// 机制：订阅后置补刷标记，每次有数据入队重置静止计时器；回放完毕连续
// REPLAY_IDLE_MS 无新数据 → 自动补一次全量重绘（等价用户点击刷新，
// glyph 缓存全部命中，无副作用；实时持续输出时计时器不断重置不会触发）
const REPLAY_IDLE_MS = 250
let replayRefreshTimer: ReturnType<typeof setTimeout> | null = null
let pendingReplayRefresh = false

// 单次 write 上限：超过则拆块，让 xterm parser 在块间让出主线程，
// 避免单帧解析超大字符串导致 UI 卡顿
const MAX_WRITE_CHUNK = 64 * 1024

// ==================== 临时调试：终端侧输出字节 dump（已注释禁用，恢复排查时取消注释） ====================
// 仅用于排查「PTY 源头输出 vs 终端显示」字节不一致问题。
// [已注释禁用] 恢复时取消下方 /* */ 注释，并恢复后端命令 append_terminal_output_dump 注册。
/*
// 本项目 tsconfig 未引入 vite/client 类型，此处强转桥接避免 TS2339（不必为临时调试改配置）
const TERMINAL_OUTPUT_DUMP_ENABLED = Boolean(
  (import.meta as unknown as { env?: { DEV?: boolean } }).env?.DEV,
)

// 记录上一次 dump 所属会话：新 PTY 会话（sessionId 变化）时第一个 flush 前重置终端侧
// 文件（覆盖旧数据），与源头 pty dump 每次会话 truncate 对齐；同会话内重连/重订阅不重置
let dumpLastSessionId = ''

// 追加待写入终端的字节到 dump 文件（临时调试；失败仅告警，不阻塞写入管线）
function dumpTerminalOutput(data: Uint8Array, sessionId: string) {
  if (!TERMINAL_OUTPUT_DUMP_ENABLED || data.length === 0) return
  const reset = sessionId !== dumpLastSessionId
  dumpLastSessionId = sessionId
  // 只记录真实输出负载，便于与源头逐字节对比；async 调用不阻塞 flush
  invoke('append_terminal_output_dump', { data: Array.from(data), reset }).catch((e) => {
    console.warn('[debug-dump] 终端输出 dump 写入失败:', e)
  })
}
*/

// 写入策略：rAF 合并同帧事件为一次 write（对齐 VS Code 行为，xterm.js 内部再统一调度渲染）；
// 大块（> MAX_WRITE_CHUNK）经 writeInChunks 分片，每 WRITE_YIELD_THRESHOLD 让出主线程一次。

let flushing = false
// 写入水线阈值（对齐 VS Code high-water 思想）：单次 flush 累计写入达到该值即让出
// 主线程一次（宏任务），使渲染/输入能在历史回放或输出风暴期间插入，避免 UI 冻结。
// 仅控制"写节奏"，不限制总量——积压数据仍在 writeQueue，下个 while 轮次继续写。
const WRITE_YIELD_THRESHOLD = 256 * 1024

async function flushWriteQueue() {
  // 避免重入：已有 flush 在跑（其 while 轮次会消费新入队数据），直接返回
  if (flushing) return
  flushing = true
  // 清掉 rAF / timer 标记（本次 flush 接管调度）
  flushRaf = 0
  if (flushTimer) {
    clearTimeout(flushTimer)
    flushTimer = null
  }
  try {
    while (writeQueue.length > 0) {
      if (!terminal) {
        // 终端未就绪：丢弃（数据已进全局缓存，可从历史恢复）
        writeQueue = []
        writeQueueBytes = 0
        return
      }
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

      // [已注释禁用] 临时调试 dump：terminal.write 前记录待写字节（恢复排查时取消注释）
      // dumpTerminalOutput(combined, sessionId.value)

      if (totalBytes <= MAX_WRITE_CHUNK) {
        terminal.write(combined)
      } else {
        await writeInChunks(combined)
      }
    }
  } finally {
    flushing = false
  }
}

/** 分片写入：按 MAX_WRITE_CHUNK 拆块写，每累积 WRITE_YIELD_THRESHOLD 让出主线程一次 */
async function writeInChunks(buf: Uint8Array) {
  // 防御：调用方 flushWriteQueue 已保证 terminal 非空，此处收窄类型（并发 agent 新增函数）
  if (!terminal) return
  let written = 0
  for (let i = 0; i < buf.length; i += MAX_WRITE_CHUNK) {
    terminal.write(buf.subarray(i, i + MAX_WRITE_CHUNK))
    written += MAX_WRITE_CHUNK
    if (written >= WRITE_YIELD_THRESHOLD) {
      written = 0
      // 宏任务让出：风暴期间允许渲染/输入插入，防 UI 冻结
      await new Promise((r) => setTimeout(r, 0))
    }
  }
}

function armReplayRefresh() {
  pendingReplayRefresh = true
  if (replayRefreshTimer) clearTimeout(replayRefreshTimer)
  replayRefreshTimer = setTimeout(() => {
    replayRefreshTimer = null
    if (!pendingReplayRefresh) return
    pendingReplayRefresh = false
    // xterm 已销毁（element 已脱离 DOM）则不再重绘
    if (terminal && terminal.element?.isConnected) {
      terminal.refresh(0, terminal.rows - 1)
    }
  }, REPLAY_IDLE_MS)
}

/** 入队输出：合并到下一渲染帧统一写入 */
function enqueueOutput(data: Uint8Array) {
  if (data.length === 0) return
  writeQueue.push(data)
  writeQueueBytes += data.byteLength
  // 数据继续到达 = 回放/输出仍在进行：重置静止补刷计时器（仅在补刷待命期）
  if (pendingReplayRefresh) {
    armReplayRefresh()
  }
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
      void flushWriteQueue()
    }, 100)
  }
}

// 本地 WS 输出流（快照模型）:
// - onData 帧已通过 seq 连续性校验/重播去重，直接写入（无去重/无补序）
// - onReset 时清屏：快照重订阅遇到历史截断（已渲染区被环形淘汰）后全量重播
// - onTruncated（min_seq > 0，参数即 min_seq）：会话开头输出已不可恢复，
//   仅首次提示一次，后续重连/重订阅触发时仅后台日志记录，避免反复打扰
let historyTruncatedNotified = false

const terminalStream = useTerminalOutputStream({
  onData: ({ data }) => {
    enqueueOutput(data)
    if (!isUserScrolling.value) {
      scrollToBottom()
    }
  },
  onReset: () => {
    if (terminal) {
      terminal.clear()
      // 清屏后即将全量重播：重播结束静止时补刷一次（防重播中间态残留）
      armReplayRefresh()
    }
  },
  onTruncated: (minSeq: number) => {
    if (historyTruncatedNotified) {
      // 已提示过：仅后台日志记录，不再弹 toast 打扰用户
      console.warn(
        `[TerminalPreview] 终端历史已被环形缓冲截断（已提示过，仅记录）：min_seq=${minSeq}`,
      )
      return
    }
    historyTruncatedNotified = true
    console.warn(
      `[TerminalPreview] 终端历史已被环形缓冲截断：min_seq=${minSeq}，会话开头输出不可用`,
    )
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
  [8, 10, 12, 14, 16, 18, 20].map((size) => ({ value: size, label: `${size}px` })),
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
watch(
  () => settingsStore.settings.ui.terminal_bg_image,
  (v) => {
    bgImage.value = v || ''
  },
)
watch(
  () => settingsStore.settings.ui.terminal_bg_opacity,
  (v) => {
    if (v != null) bgOpacity.value = v
  },
)

// 背景图片变化：重新解析 URL 并刷新终端主题（透明/不透明切换）
watch(bgImage, () => {
  resolveBgImageUrl()
})
watch([bgImageUrl, bgOpacity], () => {
  if (terminal) {
    terminal.options.theme = getTheme()
    // 仅背景图启用时透明（让图片透出）；其余场景关闭透明，避免 WebGL 透明
    // 帧缓冲滚动不清帧导致的残影/行入侵。切换后强制重绘一次清除旧模式残留帧
    terminal.options.allowTransparency = !!bgImageUrl.value
    terminal.refresh(0, terminal.rows - 1)
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
          // WebGL 渲染器 cell 尺寸与 DOM 渲染器不同（VS Code 在 webgl 加载后同样
          // 触发刷新重测网格），恢复后重算一次避免行列差 1 的漂移
          applyResize()
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
    // Linux 用系统等宽字体栈（优先 Ubuntu Mono/DejaVu Sans Mono 等系统自带等宽字体），
    // 其余平台保持 VS Code 终端默认字体栈（Windows 11 自带 Cascadia Mono）不变
    fontFamily: isLinux.value ? LINUX_FONT_STACK : DEFAULT_FONT_STACK,
    lineHeight: 1,
    // 滚动历史行数（与后端事件队列容量对齐）
    scrollback: TERMINAL_SCROLLBACK,
    // 即时滚动：维持关闭。重影根因（07 调查结论）= 无条件 allowTransparency 使 WebGL
    // 启用 alpha 帧缓冲 + 复制帧缓冲滚动优化，旧行像素不清 → 残影/行入侵；已改为按
    // 背景图条件开启透明（见下方 allowTransparency），不透明默认场景重影消失。平滑滚动
    // 属观感新动画，需物理滚轮分类器（issue 08）且真机验证后再引入，当前保守维持 0
    smoothScrollDuration: 0,
    // WebGL custom glyphs：unicode/box-drawing（╔═╗║╚╝、Powerline 等）由渲染器
    // 内置字形直接 GPU 光栅化，不依赖字体 canvas 采样，渲染一致且开销更低。
    // 0.19 版 addon-webgl 无构造参数（仅 preserveDrawingBuffer），开关走 xterm
    // Terminal 选项（TextureAtlas 读 config.customGlyphs）；此处显式声明防默认值漂移
    customGlyphs: true,
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
    // 仅在启用背景图片时允许透明（让背景图透出）；其余场景关闭透明。
    // 透明模式会迫使 WebGL 使用 alpha 帧缓冲并启用"复制帧缓冲区域"滚动优化，
    // 旧行像素不被清除 → 滚动/刷新时出现残影与"行入侵"；不透明时 WebGL 每帧
    // 正常清帧，残影消失。background 透明主题由 getTheme() 在 bgImageUrl 时返回
    allowTransparency: !!bgImageUrl.value,
    allowProposedApi: true,
  })

  fitAddon = new FitAddon()
  terminal.loadAddon(fitAddon)
  terminal.loadAddon(new WebLinksAddon())

  // Unicode 11 字符宽度计算：启用后 emoji / box-drawing（╔═╗║╚╝）等
  // 列宽按 Unicode 11 表计算，避免默认 Unicode 5 宽度表导致的光标漂移与重影
  // （TUI 应用如 opencode / vim 边框尤其明显）。移动端已加载，桌面端对齐。
  const unicode11 = new Unicode11Addon()
  terminal.loadAddon(unicode11)
  terminal.unicode.activeVersion = '11'

  terminal.open(terminalHostRef.value)
  // Linux 走 DOM(canvas) 渲染器（见 LINUX_USE_DOM_RENDERER，消除 WebGL 图集发蒙）；
  // Windows/macOS 保持 WebGL（高吞吐 + 已修复的透明残影路径）。DOM 渲染器下
  // webglAddon 为 null，后续 clearTextureAtlas / xterm-hidden-cursor 分支天然跳过。
  if (!isLinux.value || !LINUX_USE_DOM_RENDERER) {
    initWebGL(terminal)
  }

  // Linux WebKitGTK IME 根因修复：关闭 xterm 6.0.0 的 keydown(229) 遗留差值路径
  // _handleAnyTextareaChanges。现代 WebKitGTK 文本插入一律经 input 事件
  // （_inputEvent / compositionend finalize）发出，该差值路径是旧 IME 兜底；
  // 它在 keydown 229 时用 setTimeout(0) 对 textarea 前后值求差并再次发出，
  // 与 input 路径竞态时会把「已上屏文本的后缀」重复发出（输入 "bug" 后被补发
  // "ug"）。差值恒为 textarea 当前值的后缀，状态机的精确去重覆盖不了部分后缀，
  // 故从源头关闭。仅 Linux 关闭；Windows/macOS 走 xterm 原生路径不变。
  if (isLinux.value) {
    const core = (terminal as unknown as {
      _core?: { _compositionHelper?: { _handleAnyTextareaChanges?: () => void } }
    })._core
    if (core?._compositionHelper) {
      core._compositionHelper._handleAnyTextareaChanges = () => {}
    }
  }

  // Linux WebKitGTK IME 去重：组合事件/keydown(229)/input 信号喂给状态机，
  // onData 出口按组合窗口去重（见下方 onData 拦截）。仅 Linux 挂接，
  // Windows/macOS 保持 xterm 原生行为不变。
  if (isLinux.value && terminal.textarea) {
    imeStateMachine = new TerminalImeStateMachine()
    const ta = terminal.textarea
    ta.addEventListener('compositionstart', () => imeStateMachine?.onCompositionStart())
    ta.addEventListener('compositionupdate', () => imeStateMachine?.onCompositionUpdate())
    ta.addEventListener('compositionend', () => imeStateMachine?.onCompositionEnd())
    ta.addEventListener('keydown', (e: KeyboardEvent) => imeStateMachine?.onKeyDown(e.keyCode))
    ta.addEventListener('input', (e: Event) => {
      const ie = e as InputEvent
      imeStateMachine?.onInput({
        inputType: ie.inputType || '',
        isComposing: ie.isComposing ?? false,
        data: ie.data,
      })
    })
  }

  // 移除光标：用 DECTCEM 隐藏序列（\x1b[?25l）在 buffer 层隐藏光标，
  // WebGL 与 DOM 渲染器均不再绘制（TUI 程序主动发送 \x1b[?25h 时除外）
  terminal.write('\x1b[?25l')

  // WebGL 渲染器激活后，隐藏 DOM 层光标避免双光标问题
  // 只隐藏 DOM 层，保留 WebGL 层光标（WebGL 光标更流畅且不会出现双光标）
  if (webglAddon) {
    terminal.element?.classList.add('xterm-hidden-cursor')
  }

  applyDprFit()
  syncTerminalSize()

  // PTY 尺寸同步：xterm 内部 resize（含 fit 触发）时同步到后端会话
  // （经正统渲染端裁决：本端非正统时服务端返回需要确认，由弹窗处理）
  terminal.onResize(({ cols, rows }) => {
    if (props.session) {
      requestResize(cols, rows)
    }
  })

  // ResizeObserver — 分层防抖（垂直立即 / 水平 100ms 合并，flush 保证最终尺寸必达），
  // 避免拖窗时每帧整屏 reflow；仅当 cols/rows 实际变化时同步 PTY（见 applyResize）。
  // 小缓冲（<200 行，VS Code StartDebouncingThreshold）连宽度变化也立即应用：
  // 新终端/输出少的 shell reflow 便宜，拖窗时网格即时反应不滞后；
  // buffer 行数不可得时保守防抖（terminal 未就绪场景）
  resizeDebouncer = new TerminalResizeDebouncer({
    onApply: applyResize,
    getBufferLength: () => (terminal ? terminal.buffer.active.length : null),
  })
  resizeObserver = new ResizeObserver((entries) => {
    const rect = entries[0]?.contentRect
    if (rect) resizeDebouncer?.resize(rect.width, rect.height)
  })
  resizeObserver.observe(terminalHostRef.value)

  // 窗口恢复可见/聚焦时立即兑现挂起的防抖（最小化时 ResizeObserver 报 0 尺寸
  // 已被防抖器过滤，恢复后的首帧通过 RO 正常触发；flush 负责隐藏期间
  // 已入队的非 0 尺寸滞后兑现，不必等 100ms 计时器）
  windowVisibleHandler = () => {
    if (document.visibilityState === 'visible') {
      resizeDebouncer?.flush()
    }
  }
  document.addEventListener('visibilitychange', windowVisibleHandler)
  window.addEventListener('focus', windowVisibleHandler)

  // DPR 动态变化监听（跨屏拖动 / 系统缩放变化时窗口尺寸可能不变，
  // ResizeObserver 不触发）：matchMedia 只能匹配固定 dppx 值，变化后需按新值递归注册
  watchDprChanges()

  // 滚动状态：xterm onScroll API（比 DOM addEventListener 更可靠，
  // 不会因 xterm 内部 DOM 重建而丢失监听）；仅更新"是否在底部"状态，
  // 重绘完全交给 xterm 渲染循环，不做手动 refresh 补丁
  scrollDisposable = terminal.onScroll(() => {
    if (!terminal) return
    const buffer = terminal.buffer.active
    const viewportBottom = buffer.viewportY + terminal.rows
    isUserScrolling.value = viewportBottom < buffer.length - 1
  })

  // 输出解析完成（每次 write 后触发，覆盖输出/清屏/TUI 切换场景）：
  // 作为背压 ack 的写解析完成信号（确认已渲染到的 last_rendered_seq）
  terminal.onWriteParsed(() => {
    if (!terminal) return
    terminalStream.confirmWriteParsed()
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

    // Linux WebKitGTK IME 双发去重：同一组合文本被 xterm 两条路径重复发出时，
    // 丢弃重复载荷（仅 Linux 启用；Windows/macOS 不受影响）
    if (imeStateMachine && !imeStateMachine.shouldForward(data)) {
      return
    }

    // 有选区时 Ctrl+C 仅复制（VS Code 终端行为），不向 PTY 发送中断
    if (data === '\x03' && hasSelection) {
      const sel = terminal?.getSelection()
      if (sel) {
        navigator.clipboard?.writeText(sel).catch(() => {})
      }
      return
    }

    sessionStore.writeToSession(props.session.id, data)

    // 多行粘贴（一次事件含换行）：仅重建当前行追踪（不再记录输入标记）
    if (data.length > 1 && /[\r\n]/.test(data)) {
      currentLineBuffer = ''
      return
    }

    // 追踪当前行输入（供 AI 插件读取）
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

// ==================== 正统渲染端 resize 裁决交互 ====================
// 桌面端与移动端共用同一 PTY 尺寸：服务端裁决本端是否正统渲染端。
// 非正统时 resize 返回 needsConfirmation（未应用），此处弹窗确认，
// 确认后 force 重发覆盖；取消则记下被拒尺寸，防 RO/resize 事件风暴。

/** 用户拒绝覆盖的尺寸（成功后清空；同尺寸不再重发） */
let rejectedSize: { cols: number; rows: number } | null = null

/** 待确认的覆盖目标（弹窗内容源） */
const rendererOverrideTarget = ref<{
  cols: number
  rows: number
  rendererName: string
} | null>(null)
const showRendererOverrideModal = ref(false)

/** 渲染端显示名：桌面端用 i18n 标签，移动端用设备名；source 异常时兜底，避免弹窗崩溃 */
function rendererDisplayName(source?: RendererSource | null): string {
  if (!source || source.kind === 'desktop') return t('desktop.terminal.rendererDesktop')
  return source.deviceName || t('desktop.terminal.rendererMobile')
}

/**
 * 请求调整会话尺寸（经服务端正统渲染端裁决）
 *
 * force=false：本端非正统且用户未确认前，服务端不改底层 PTY；
 * needsConfirmation 时弹出确认框。用户拒绝过的同尺寸直接忽略，
 * 避免拖窗/RO 事件持续触发弹窗风暴。
 */
async function requestResize(cols: number, rows: number, force = false) {
  if (!props.session) return
  // 用户刚拒绝过的相同尺寸：抑制（每次成功应用后清空）
  if (!force && rejectedSize && rejectedSize.cols === cols && rejectedSize.rows === rows) return
  // 相同尺寸已在确认弹窗中：避免并发弹窗
  if (rendererOverrideTarget.value && rendererOverrideTarget.value.cols === cols && rendererOverrideTarget.value.rows === rows) {
    return
  }
  const outcome = await sessionStore.resizeSession(props.session.id, cols, rows, force)
  if (outcome.status === 'applied') {
    rejectedSize = null
    return
  }
  // needsConfirmation：当前有另一个端在渲染，弹窗确认是否覆盖
  rendererOverrideTarget.value = {
    cols,
    rows,
    rendererName: rendererDisplayName(outcome.currentCanonical),
  }
  showRendererOverrideModal.value = true
}

/** 用户确认覆盖：force 重发（尺寸移交服务端正统归属） */
async function confirmRendererOverride() {
  const target = rendererOverrideTarget.value
  showRendererOverrideModal.value = false
  rendererOverrideTarget.value = null
  if (target) await requestResize(target.cols, target.rows, true)
}

/** 用户拒绝覆盖：记录被拒尺寸，同尺寸不再打扰 */
function cancelRendererOverride() {
  const target = rendererOverrideTarget.value
  showRendererOverrideModal.value = false
  rendererOverrideTarget.value = null
  if (target) rejectedSize = { cols: target.cols, rows: target.rows }
}

/** 同步当前终端尺寸到后端会话（PTY cols/rows） */
function syncTerminalSize() {
  if (!terminal || !props.session) return
  const cols = terminal.cols
  const rows = terminal.rows
  if (cols > 0 && rows > 0) {
    requestResize(cols, rows)
  }
}

/**
 * resize 实际应用（防抖器 onApply 接线）：DPR 感知 fit + 仅 cols/rows
 * 实际变化才全量重绘 + PTY 同步。subpixel 抖动（容器尺寸微调但行列不变）
 * 不触发多余重绘，避免拖动窗口时每帧全量重绘的浪费
 */
function applyResize() {
  if (!terminal) return
  const cols = terminal.cols
  const rows = terminal.rows
  applyDprFit()
  if (terminal.cols !== cols || terminal.rows !== rows) {
    terminal.refresh(0, terminal.rows - 1)
    syncTerminalSize()
  }
}

/** 读取 xterm 实测 cell CSS 尺寸（DPR 感知计算的输入）；不可用时返回 null */
function measureCellSize(): { width: number; height: number } | null {
  if (!terminal) return null
  // addon-fit 0.11 内部即此访问路径（FitAddon.proposeDimensions）；
  // 私有 API 无类型声明，逐级防御，任一环节缺失即回退 fitAddon.fit()
  const core = (terminal as unknown as { _core?: unknown })._core as
    | { _renderService?: { dimensions?: { css?: { cell?: { width: number; height: number } } } } }
    | undefined
  const cell = core?._renderService?.dimensions?.css?.cell
  if (!cell || cell.width <= 0 || cell.height <= 0) return null
  return { width: cell.width, height: cell.height }
}

/**
 * DPR 感知 fit：容器 CSS 尺寸 × devicePixelRatio 换算设备像素后计算 cols/rows
 * （行高 ceil、列宽 floor），替换 fitAddon.fit() 的裸 DPR 不感知计算。
 * 容器/cell 尺寸不可用时优雅降级回 fitAddon.fit()，不炸。
 *
 * 触发 resize 前经 shouldApplyGridResize 锌制 ±1 列/行测量漂移：applyDprFit 的
 * 尺寸口径（clientWidth × 渲染器 css.cell）与当前网格存在 ±1~2 列系统性偏差，
 * 每次精确比较都 resize 会让 WebGL 在点“刷新”时重建整个字符图集（非 ASCII
 * 字形按 idle 分片异步重新光栅化），表现为前几次刷新格式乱、图集预热完才正常。
 * 真实 resize（拖窗/字号变化）后 scheduleAtlasPreheat 补刷收尾。
 */
function applyDprFit() {
  if (!terminal || !fitAddon) return
  const host = terminalHostRef.value
  const cell = measureCellSize()
  if (!host || host.clientWidth <= 0 || host.clientHeight <= 0 || !cell) {
    fitAddon.fit()
    return
  }
  const { cols, rows } = getXtermScaledDimensions({
    containerWidthCss: host.clientWidth,
    containerHeightCss: host.clientHeight,
    cellWidthCss: cell.width,
    cellHeightCss: cell.height,
    devicePixelRatio: window.devicePixelRatio,
  })
  // 与 FitAddon.fit() 一致：尺寸不变不动（避免无谓 resize 事件），
  // 变化时经 ±1 漂移锌制，仅在真实变化时 resize
  if (terminal.cols !== cols || terminal.rows !== rows) {
    if (!shouldApplyGridResize(terminal.cols, terminal.rows, cols, rows)) {
      return
    }
    terminal.resize(cols, rows)
    scheduleAtlasPreheat()
  }
}

/**
 * WebGL atlas 预热补刷：resize 重建字符图集后，非 ASCII 字形（中文/box-drawing/
 * emoji）按 requestIdleCallback 分片异步光栅化，等基本完成后补一次全量重绘，
 * 让屏幕一次恢复完整（否则用户看到“前几次乱，第三次好”）。DOM 渲染器下无害
 * （refresh 只是重建 DOM 行）。同窗口多次 resize 合并为一次（重置计时器）。
 */
function scheduleAtlasPreheat() {
  if (atlasPreheatTimer) clearTimeout(atlasPreheatTimer)
  atlasPreheatTimer = setTimeout(() => {
    atlasPreheatTimer = null
    // xterm 已销毁（element 已脱离 DOM）则不再重绘
    if (terminal && terminal.element?.isConnected) {
      terminal.refresh(0, terminal.rows - 1)
    }
  }, ATLAS_PREHEAT_DELAY_MS)
}

/**
 * Linux 首帧模糊修复：WebKitGTK 的 document.fonts 不追踪系统字体（DejaVu 等经
 * fontconfig 解析），fonts.ready 会提前 resolve——首次 measure 可能仍用回退字体/
 * 旧指标，导致字符尺寸按错指标光栅化 → 整屏文字发蒙。挂载并跑完首帧后延迟重测
 * + 全量重绘一次，消除首帧模糊残留。DOM 渲染器重绘即重建行；WebGL 渲染器还会
 * 重建字形图集（clearTextureAtlas 在 DOM 渲染器下为 no-op）。
 */
function scheduleInitialFontRemeasure() {
  if (!isLinux.value) return
  setTimeout(() => {
    if (!terminal || !terminal.element?.isConnected) return
    // 强制重测字符尺寸（等价 xterm open/resize 时的 measure；私有路径与
    // measureCellSize 同源，缺失时优雅跳过，refresh 仍保证重绘一次）
    const core = (terminal as unknown as {
      _core?: { _charSizeService?: { measure?: () => void } }
    })._core
    core?._charSizeService?.measure?.()
    // WebGL 图集按新指标重建；DOM 渲染器下 clearTextureAtlas 为 no-op
    if (webglAddon) {
      terminal.clearTextureAtlas()
    }
    terminal.refresh(0, terminal.rows - 1)
  }, 300)
}

/**
 * 监听 DPR 变化并应用新尺寸：matchMedia 只匹配固定 dppx 值，
 * 每次命中后按新 DPR 重新注册（递归），直到组件卸载
 */
function watchDprChanges() {
  if (dprMediaQuery && dprChangeHandler) {
    dprMediaQuery.removeEventListener('change', dprChangeHandler)
  }
  dprChangeHandler = () => {
    // DPI 变化后重新 fit + 条件重绘/同步（窗口尺寸可能未变，ResizeObserver 不触发）
    applyResize()
    watchDprChanges()
  }
  dprMediaQuery = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`)
  dprMediaQuery.addEventListener('change', dprChangeHandler)
}

/**
 * fit 后强制全量重绘：WebGL 渲染器在容器尺寸变化（全屏/滚动触发布局微变/字号变化）
 * 后不会自动重绘可见区域，旧纹理残留导致字符错位与格式错乱，故 fit 后立即
 * refresh 整屏修正。覆盖 resize 与字号变化两类重绘场景（刷新按钮不经过此处，
 * 见 refreshTerminal 的“为什么不 fit”说明）。
 */
function fitAndRefresh() {
  if (!fitAddon || !terminal) return
  applyDprFit()
  terminal.refresh(0, terminal.rows - 1)
}

/**
 * 刷新格式：纯重绘 + 同步 PTY，不清除内容、**不重算尺寸**。
 *
 * 为什么不在这里 fit：尺寸已由三条路径维护——ResizeObserver（容器变化）、
 * 字号 watcher、DPR 监听；而 applyDprFit 的口径（clientWidth × 渲染器 css.cell）
 * 与当前网格存在 ±1~2 列恒定测量偏差，点击时重算几乎必然触发一次 resize →
 * WebGL 重建整个字符图集（非 ASCII 字形重新光栅化）→ 当次显示混乱，再点才
 * 恢复。刷新按钮的语义就是“重绘一次”，fit 在这里只会帮倒忙（尺寸偏差已由
 * 防抖器经 shouldApplyGridResize 鉗制兜底，偏差 >1 才可能真触发 resize）。
 */
function refreshTerminal() {
  if (!terminal || !props.session) return
  terminal.refresh(0, terminal.rows - 1)
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

function clearTerminal() {
  if (!terminal) return
  terminal.clear()
}

// ==================== 设置同步 ====================

// 字体大小变化
let fontSizeSaveTimeout: ReturnType<typeof setTimeout> | null = null
watch(fontSize, (newSize) => {
  if (!terminal) return
  terminal.options.fontSize = newSize
  if (fitAddon) {
    fitAndRefresh()
  }
  nextTick(() => syncTerminalSize())
  if (fontSizeSaveTimeout) clearTimeout(fontSizeSaveTimeout)
  fontSizeSaveTimeout = setTimeout(() => {
    settingsStore.saveSettings({
      ui: { ...settingsStore.settings.ui, terminal_font_size: newSize },
    })
  }, 300)
})

watch(
  () => settingsStore.settings.ui.terminal_font_size,
  (newSize) => {
    if (fontSize.value !== newSize) {
      fontSize.value = newSize
      if (terminal) {
        terminal.options.fontSize = newSize
        if (fitAddon) fitAndRefresh()
        nextTick(() => syncTerminalSize())
      }
    }
  },
  { immediate: true },
)

// 主题变化：更新终端 + 持久化
let themeSaveTimeout: ReturnType<typeof setTimeout> | null = null
watch(terminalTheme, (newTheme) => {
  if (terminal) {
    terminal.options.theme = getTheme()
  }
  if (themeSaveTimeout) clearTimeout(themeSaveTimeout)
  themeSaveTimeout = setTimeout(() => {
    settingsStore.saveSettings({
      ui: { ...settingsStore.settings.ui, terminal_theme: newTheme },
    })
  }, 300)
})

// 外部设置变化同步主题
watch(
  () => settingsStore.settings.ui.terminal_theme,
  (newTheme) => {
    if (newTheme && terminalTheme.value !== newTheme) {
      terminalTheme.value = newTheme
    }
  },
)

// 会话变化
// 游标重置（新会话坐标空间独立），断开旧流并连接新流；
// 历史回放由服务端裁决后以二进制帧流式送达（无需 invoke 拉取）。
// 首次挂载（terminal 未就绪）只握手不订阅，订阅由 onMounted 触发
let streamMounted = false
watch(
  sessionId,
  async (newId, oldId) => {
    if (newId !== oldId) {
      if (oldId) {
        clearTerminal()
      }

      if (newId) {
        await nextTick()

        if (terminal) {
          syncTerminalSize()
        }

        // 新会话：重置历史截断提示标记，允许再次提示
        historyTruncatedNotified = false

        if (props.session?.status === 'starting') {
          // 延迟启动第二阶段：终端已挂载传当前实际网格；否则按当前终端窗口
          // 实际尺寸精确预测（widthRatio=1，本组件只存在于终端窗口内），
          // spawn 前 resize PTY，子进程从正确行列起步
          const size =
            terminal != null
              ? { cols: terminal.cols, rows: terminal.rows }
              : await computeDesktopInitialTerminalSize(fontSize.value)
          await sessionStore.startSession(newId, size ?? undefined)
        }

        terminalStream.start(newId)
        armReplayRefresh()
        if (streamMounted) {
          terminalStream.subscribe()
        }
      } else {
        terminalStream.stop()
      }
    }
  },
  { immediate: true },
)

// 会话状态变化：停止/出错时断开输出流；重新运行时恢复订阅
watch(
  () => props.session?.status,
  (status) => {
    if (!sessionId.value) return
    if (status === 'stopped' || status === 'error') {
      terminalStream.stop()
    } else if (status === 'running') {
      terminalStream.start(sessionId.value)
      armReplayRefresh()
      terminalStream.subscribe()
    }
  },
)

onMounted(async () => {
  await nextTick()

  // 确定运行平台：await 已 memoized 的 initPlatform()（main.ts 启动前已发起，
  // 此处最多一个 microtask 即返回），消除 setup 时平台尚未解析的竞态
  const platform = await initPlatform()
  isLinux.value = platform?.isLinux === true

  // Linux：等待系统字体加载完成再初始化终端，避免 WebKitGTK 在字体未就绪时
  // 用回退字体测量字符尺寸，导致字符间距过大/模糊；带超时兜底不阻塞首帧。
  // Windows/macOS 保持原有行为（字体即装即用，无需等待）。
  if (isLinux.value && typeof document !== 'undefined' && 'fonts' in document) {
    try {
      await Promise.race([
        document.fonts.ready,
        new Promise((resolve) => setTimeout(resolve, 400)),
      ])
    } catch {
      // fonts.ready 异常不阻塞终端初始化
    }
  }

  initTerminal()

  // 初始化背景图片（在 initTerminal 之后，仅影响后续主题刷新；
  // 首次挂载时若已有背景图，通过一次主题刷新生效）
  await resolveBgImageUrl()
  if (terminal) {
    terminal.options.theme = getTheme()
  }

  // Linux：首帧后重测字符尺寸 + 全量重绘（见 scheduleInitialFontRemeasure），
  // 消除 WebKitGTK 首次 measure 用回退字体指标导致的模糊
  scheduleInitialFontRemeasure()

  // 渲染链路诊断（排查模糊/回退问题时日志可见 renderer 与 DPR）
  console.info(
    `[TerminalPreview] renderer=${webglAddon ? 'webgl' : 'dom'} ` +
      `dpr=${window.devicePixelRatio} fontSize=${fontSize.value} ` +
      `isLinux=${isLinux.value}`,
  )

  // 监听 AI 插件请求当前终端输入
  pluginEventOn('__host__', 'ai-chatbox:getCurrentInput', () => {
    pluginEventEmit('ai-chatbox:currentInput', {
      sessionId: sessionId.value,
      text: currentLineBuffer,
    })
  })

  // terminal 就绪后启动本地 WS 输出流：历史回放 + 实时推送同通道流式到达
  terminalStream.start(sessionId.value)
  armReplayRefresh()
  terminalStream.subscribe()
  streamMounted = true

  terminal?.focus()
})

onUnmounted(() => {
  // 断开本地 WS 输出流（停止重连）
  terminalStream.stop()

  // 清理 AI 插件事件监听
  clearPluginEvents('__host__')

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

  // 清理回放静止补刷定时器
  if (replayRefreshTimer) {
    clearTimeout(replayRefreshTimer)
    replayRefreshTimer = null
    pendingReplayRefresh = false
  }

  // 清理 WebGL atlas 预热补刷定时器
  if (atlasPreheatTimer) {
    clearTimeout(atlasPreheatTimer)
    atlasPreheatTimer = null
  }

  // 清理前台 flush 监听
  if (windowVisibleHandler) {
    document.removeEventListener('visibilitychange', windowVisibleHandler)
    window.removeEventListener('focus', windowVisibleHandler)
    windowVisibleHandler = null
  }

  // 清理 resize 防抖器（挂起的水平防抖直接丢弃：组件已卸载无需应用）
  if (resizeDebouncer) {
    resizeDebouncer.dispose()
    resizeDebouncer = null
  }

  // 清理 DPR 变化监听
  if (dprMediaQuery && dprChangeHandler) {
    dprMediaQuery.removeEventListener('change', dprChangeHandler)
    dprMediaQuery = null
    dprChangeHandler = null
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

  // 清理 Linux IME 状态机（监听随 textarea 一并销毁，此处仅复位状态）
  if (imeStateMachine) {
    imeStateMachine.reset()
    imeStateMachine = null
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
  transition:
    opacity 0.2s ease,
    transform 0.2s ease;
}

.scroll-indicator-enter-from,
.scroll-indicator-leave-to {
  opacity: 0;
  transform: translateY(8px);
}
</style>
