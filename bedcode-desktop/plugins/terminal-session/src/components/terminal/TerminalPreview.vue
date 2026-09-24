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
          {{ session?.name || t('session.terminal.defaultName') }}
        </h3>
      </div>

      <div class="flex items-center gap-2">
        <select
          class="h-7 rounded-[6px] px-2 cursor-pointer text-[calc(12px*var(--ui-scale))] bg-[var(--bg-card)] border border-[var(--border)] text-[var(--text-primary)] focus:outline-none focus:border-brand"
          :value="terminalTheme"
          :title="t('session.terminal.theme')"
          @change="terminalTheme = ($event.target as HTMLSelectElement).value"
        >
          <option v-for="opt in themeSelectOptions" :key="opt.value" :value="opt.value">
            {{ opt.label }}
          </option>
        </select>
        <select
          class="h-7 rounded-[6px] px-2 cursor-pointer text-[calc(12px*var(--ui-scale))] bg-[var(--bg-card)] border border-[var(--border)] text-[var(--text-primary)] focus:outline-none focus:border-brand"
          :value="fontSize"
          :title="t('session.terminal.fontSize')"
          @change="fontSize = Number(($event.target as HTMLSelectElement).value)"
        >
          <option v-for="opt in fontSizeSelectOptions" :key="opt.value" :value="opt.value">
            {{ opt.label }}
          </option>
        </select>
        <button
          class="w-7 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors"
          :title="t('session.terminal.clearScreen')"
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
        </button>
        <button
          class="w-7 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors"
          :title="t('session.terminal.refreshFormat')"
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
        </button>
      </div>
    </header>

    <!-- 终端主体：xterm 挂载点（唯一渲染宿主） + 背景图片层 + 滚动到底指示器 -->
    <div
      ref="terminalHostRef"
      class="relative flex-1 min-h-0 overflow-hidden"
      :class="{ 'terminal-transparent': rendererDecision.allowTransparency }"
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
          :title="t('session.terminal.scrollToBottom')"
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
    <div
      v-if="showRendererOverrideModal"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
      @click.self="cancelRendererOverride"
    >
      <div
        class="w-96 rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] shadow-xl"
      >
        <div class="px-4 py-3 border-b border-[var(--border)]">
          <span class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">
            {{ t('session.terminal.rendererOverrideTitle') }}
          </span>
        </div>
        <div class="px-4 py-4 text-[calc(12.5px*var(--ui-scale))] text-[var(--text-secondary)] whitespace-pre-line">
          {{
            t('session.terminal.rendererOverrideBody', {
              renderer: rendererOverrideTarget?.rendererName ?? '',
            })
          }}
        </div>
        <div class="px-4 py-3 flex justify-end gap-2 border-t border-[var(--border)]">
          <button
            class="wb-btn-ghost !h-7 !px-3 text-[calc(12px*var(--ui-scale))]"
            @click="cancelRendererOverride"
          >
            {{ t('session.terminal.rendererOverrideCancel') }}
          </button>
          <button
            class="wb-btn-primary !h-7 !px-3 text-[calc(12px*var(--ui-scale))]"
            @click="confirmRendererOverride"
          >
            {{ t('session.terminal.rendererOverrideConfirm') }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 终端渲染组件（插件版，票 03a A1 收尾）— 桌面端终端渲染内核（xterm.js）
 *
 * 自宿主 `TerminalPreview.vue` 拆分产物迁入后的**组装层**：01 子票只迁了
 * composables/utils（渲染管线各域），本组件把 xterm 实例化 + 各域接线
 * （kernel / writePipeline / renderer / resize / scroll / settingsSync / IME）。
 *
 * 与宿主版本差异（方案 1 迁入适配）：
 * - xterm 实例由插件创建（宿主不再提供 TerminalPreview）；
 * - 输出源（票 04 起）经插件 WASM 命令面 `session.output.pull` 轮询拉取——插件
 *   Rust 后端调 `host-session.output-ring-fetch` 原语（WIT `list<u8>` 二进制直传，
 *   不 JSON 化），前端定时拉取写入管线；宿主 Channel 桥（`caps.output.attachSink`）
 *   已不再调用，契约字段保留至票 05 宿主摘除；
 * - 终端设置经注入 `TerminalSettingsAccessor`（宿主 settingsStore 桥；
 *   无注入环境用内存版 fallback，dev-shell/vitest 可独立渲染）；
 * - resize 请求经插件 WASM 命令面 `session.action.resize`（host-session
 *   原语 + 服务端裁决）；
 * - 输入经插件命令通道 `session.input`（票 08 起宿主不再有会话输入命令面，
 *   写 PTY / 提交行重建 / 任务域观察全在本插件 WASM 内完成）；
 * - AI 插件输入追踪（宿主 plugin/events `ai-chatbox:getCurrentInput` 协议）暂
 *   不迁移（宿主插件事件总线非插件可见面，票 05 摘除宿主时按互调协议另议）；
 * - rendererOverride 弹窗用插件内联覆盖层（宿主 Modal 组件不迁入插件）。
 *
 * 渲染/写入/IME/设置同步逻辑逐字等价（行为契约以宿主 terminal-flow 集成
 * 测试 + 01/02 各域单测为准）。
 */
import { ref, computed, watch, onMounted, onUnmounted, nextTick, inject } from 'vue'
import { useI18n } from 'vue-i18n'
import { toast } from 'vue-sonner'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { Unicode11Addon } from '@xterm/addon-unicode11'
// 宿主 OS 平台：同步 API（非 Tauri 环境抛错，try/catch 回退非 Linux；
// 与 index.ts 的 platform() 用法一致，dev-shell 为浏览器环境）
import { platform } from '@tauri-apps/plugin-os'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { SessionInfo } from '../../composables/terminal/model'
import { createTerminalKernel } from '../../composables/terminal/terminalKernel'
import { useTerminalWritePipeline, type TerminalOutputSink } from '../../composables/terminal/useTerminalWritePipeline'
import { useTerminalScroll } from '../../composables/terminal/useTerminalScroll'
import { useTerminalRenderer } from '../../composables/terminal/useTerminalRenderer'
import { useTerminalResize, type ResizeRequester, type ResizeOutcome } from '../../composables/terminal/useTerminalResize'
import { useTerminalSettingsSync } from '../../composables/terminal/useTerminalSettingsSync'
import { attachLinuxImeGuard, type LinuxImeGuard } from '../../utils/terminal/terminalLinuxImeGuard'
import { TerminalResizeDebouncer } from '../../utils/terminal/terminalResizeDebouncer'
import {
  LINUX_FONT_STACK,
  DEFAULT_FONT_STACK,
  TERMINAL_FONT_SIZES,
  TERMINAL_THEME_NAMES,
} from '../../utils/terminal/terminalThemes'
import { TERMINAL_SCROLLBACK } from '../../utils/terminal/terminalScrollback'
import {
  useTerminalHostCapabilities,
  createFallbackHostCapabilities,
} from './terminalHostCapabilities'

interface Props {
  session?: SessionInfo | null
  showInput?: boolean
  /** 是否显示组件内 header（终端窗口模式下由外层统一管理 header） */
  showHeader?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  session: null,
  showInput: true,
  showHeader: true,
})

const { t } = useI18n()

// 插件上下文（PluginViewHost / dev-shell 注入；插件组件既有模式）
const context = inject<PluginContext>('pluginContext')!
// 终端宿主能力（宿主插件窗口注入；dev-shell/vitest 回退内存版）
const caps = useTerminalHostCapabilities() ?? createFallbackHostCapabilities()

const terminalHostRef = ref<HTMLElement | null>(null)

// ==================== 内核与域实例化 ====================

/** resize 请求实现：经插件 WASM 命令面 `session.action.resize`（服务端正统
 *  渲染端裁决；dev-shell 无后端时回退 applied 保持不阻塞）。
 *  必须先于 useTerminalResize 定义（setup 同步执行，const 提升 TDZ） */
const requestResizeImpl: ResizeRequester = async (sessionId, cols, rows, force) => {
  try {
    const result = await context.commands.execute('session.action.resize', {
      sessionId,
      cols,
      rows,
      force,
    })
    // 防御：后端返回形状异常（dev-shell / 命令未接）时回退 applied，不阻塞 resize 链路
    if (result && typeof result === 'object' && 'status' in result) {
      return result as ResizeOutcome
    }
    return { status: 'applied', canonical: { kind: 'desktop' } }
  } catch (e) {
    console.warn('[terminal-session] resize 命令失败，回退 applied:', e)
    return { status: 'applied', canonical: { kind: 'desktop' } }
  }
}

const kernel = createTerminalKernel(() => props.session, terminalHostRef)
const settings = useTerminalSettingsSync(kernel, caps.settings, console)
const pipeline = useTerminalWritePipeline(kernel, {
  notifyTruncated: () => toast.warning(t('session.terminal.historyTruncated')),
  logTruncated: (message) => console.warn(message),
})
const scroll = useTerminalScroll(kernel)
const renderer = useTerminalRenderer(kernel, console)
const resize = useTerminalResize(kernel, requestResizeImpl)

// 模板同名绑定（域返回值解构，template 零改动）
const { terminalTheme, fontSize, themeSelectOptions, fontSizeSelectOptions } = settings
const { bgImageUrl, bgOpacity, containerBgColor } = settings
const { isUserScrolling, clearTerminal, refreshTerminal, scrollToBottomManual } = scroll
const { rendererDecision } = renderer
const {
  showRendererOverrideModal,
  rendererOverrideTarget,
  confirmRendererOverride,
  cancelRendererOverride,
} = resize

const sessionId = computed(() => props.session?.id || '')

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

// ==================== 生命周期编排状态（仅本组件使用） ====================

let imeGuard: LinuxImeGuard | null = null
let resizeObserver: ResizeObserver | null = null
let resizeDebouncer: TerminalResizeDebouncer | null = null
let windowVisibleHandler: (() => void) | null = null
let hasSelection = false
let wheelHandler: ((e: WheelEvent) => void) | null = null
// ==================== 输出拉取（票 04：撤宿主 Channel 桥，改经插件 WASM 原语轮询） ====================
//
// 宿主 Channel 桥（caps.output.attachSink）已撤：输出改为前端定时经插件命令面
// `session.output.pull` 拉取（插件 Rust 后端 → `host-session.output-ring-fetch` 原语，
// WIT `list<u8>` 二进制直传不 JSON 化）。游标（nextOffset）前端自持——慢消费只损失
// 自己的 ring 历史（`truncated` 时清屏重锚），宿主环绝不回传背压。

/** 快档轮询间隔（ms）：活跃输出期经此节奏拉取 */
const OUTPUT_PULL_INTERVAL_MS = 100
/** 单 tick 最多连续拉批数（每批 ≤16 KiB）：输出风暴时一次拿净积压，避免每 tick 只挪
 *  16 KiB 的拖尾；批数封顶防单 tick 长占主线程 */
const OUTPUT_PULL_MAX_BATCHES = 8
/** 连续空闲（追平）次数达到该值后降为慢档轮询 */
const OUTPUT_IDLE_THRESHOLD = 5
/** 慢档轮询间隔（ms）：空闲期省 invoke 往返；有数据立即回到快档 */
const OUTPUT_IDLE_INTERVAL_MS = 500

/** 写入管线 sink（attachSource 返回值；轮询回调写入目标） */
let outputSink: TerminalOutputSink | null = null
/** 拉取游标（= 下一批的 fromOffset；追平后停在产出端） */
let outputCursor = 0
let pullTimer: ReturnType<typeof setInterval> | null = null
let pullInFlight = false
/** 连续追平计数（空闲退避依据） */
let idleStreak = 0

/** 拉一轮：单 tick 内最多连续拉 OUTPUT_PULL_MAX_BATCHES 批，返回是否仍有余量 */
async function pullOnce(): Promise<boolean> {
  if (pullInFlight || !props.session?.id || !outputSink) return false
  pullInFlight = true
  try {
    for (let i = 0; i < OUTPUT_PULL_MAX_BATCHES; i++) {
      const res: unknown = await context.commands.execute('session.output.pull', {
        sessionId: props.session.id,
        fromOffset: outputCursor,
      })
      // null = 游标已追平产出端（宿主 output-ring-fetch Ok(None)）
      if (res === null || res === undefined) return false
      const r = res as { data?: number[]; nextOffset?: number; truncated?: boolean }
      const next = typeof r.nextOffset === 'number' ? r.nextOffset : outputCursor
      if (r.truncated) {
        // resync：游标落后于环驻留起点（中间字节已被淘汰）→ 清屏重锚后从现存段起播
        outputSink.onReset()
        outputSink.onTruncated(outputCursor)
      }
      outputCursor = next
      if (Array.isArray(r.data) && r.data.length > 0) {
        outputSink.onData({ data: new Uint8Array(r.data) })
      }
      // 空段（含 truncated 但无驻留字节）= 追平；有数据但不满批 → 下批大概率追平，
      // 提前让出（少一次 invoke 往返）
      if (!r.data || r.data.length === 0) return false
    }
    return true
  } finally {
    pullInFlight = false
  }
}

async function pullTick() {
  const hadMore = await pullOnce()
  idleStreak = hadMore ? 0 : idleStreak + 1
  // 自适应间隔：连续空闲后降为慢档（省 invoke），任一 tick 有数据立即回快档
  if (idleStreak === OUTPUT_IDLE_THRESHOLD && pullTimer) {
    clearInterval(pullTimer)
    pullTimer = setInterval(() => void pullTick(), OUTPUT_IDLE_INTERVAL_MS)
  }
}

function stopOutputPull() {
  if (pullTimer) {
    clearInterval(pullTimer)
    pullTimer = null
  }
  pullInFlight = false
  idleStreak = 0
}

/** 接入输出源（票 04：插件 WASM 原语轮询拉取 → 写入管线）；会话停止/卸载时断开 */
function attachOutputSource() {
  if (!props.session?.id) return
  stopOutputPull()
  pipeline.resetTruncatedNotified()
  outputCursor = 0
  outputSink = pipeline.attachSource()
  pipeline.armReplayRefresh()
  // 先立即拉一轮（历史回放），再进入自适应轮询
  void pullOnce()
  pullTimer = setInterval(() => void pullTick(), OUTPUT_PULL_INTERVAL_MS)
}

function detachOutputSource() {
  stopOutputPull()
  outputSink = null
}

function initTerminal() {
  if (!terminalHostRef.value) return

  const terminal = new Terminal({
    // 字体与尺寸
    fontSize: settings.effectiveFontSize.value,
    // Linux 用系统等宽字体栈（优先 Ubuntu Mono/DejaVu Sans Mono 等系统自带等宽字体），
    // 其余平台保持 VS Code 终端默认字体栈（Windows 11 自带 Cascadia Mono）不变
    fontFamily: kernel.isLinux.value ? LINUX_FONT_STACK : DEFAULT_FONT_STACK,
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
    // 与 VS Code 终端对齐的选项（spec D-5 / ticket 04）：
    // scrollOnEraseInDisplay —— PuTTY 式清屏：ED 清屏序列擦除内容进入
    // scrollback 而非只清视口（默认 false 时全屏 TUI 清屏后内容错乱残留）
    scrollOnEraseInDisplay: true,
    // windowOptions —— 应答 DA1/DSM 能力查询，老式终端不因探测超时降级
    windowOptions: {
      getWinSizePixels: true,
      getCellSizePixels: true,
      getWinSizeChars: true,
    },
    // 双击选词分隔符 = VS Code 默认（实测 terminalConfiguration.ts:503），
    // 在 xterm 默认（ ()[]{}\',"` ）基础上补齐 box-drawing ─ 与中文引号
    // ‘’“”，路径/URL 双击选中不截断。字面量含反引号与单引号，故外层用
    // 单引号时反斜杠作转义前缀
    wordSeparator: ' ()[]{}\',"`─‘’“”|',
    // Tab 制表宽度与最低对比度对齐 VS Code 默认值（xterm 默认即 8 / 1，
    // 显式声明防默认值漂移）；滚动灵敏度保持 VS Code 相同的 1 / 5
    tabStopWidth: 8,
    minimumContrastRatio: 1,
    scrollSensitivity: 1,
    fastScrollSensitivity: 5,
    // 主题
    theme: kernel.callbacks.getTheme(),
    // 透明度与渲染器由 decideRenderer 统一决策（spec D-1/D-2，route 默认 'B'）：
    // 仅背景图开启时透明（让图片透出）；其余场景关闭透明，避免 WebGL alpha 帧
    // 缓冲滚动不清帧导致的残影/行入侵；背景图场景同时强制 DOM 渲染器，透明
    // 天然正确。allowTransparency 在渲染器创建时一次性生效，运行时的透明度
    // 状态变化由 rebuildRenderer（dispose + 重建）处理，不能只改 options
    allowTransparency: renderer.rendererDecision.value.allowTransparency,
    allowProposedApi: true,
  })
  kernel.terminalRef.value = terminal

  const fitAddon = new FitAddon()
  kernel.fitAddonRef.value = fitAddon
  terminal.loadAddon(fitAddon)
  terminal.loadAddon(new WebLinksAddon())

  // Unicode 11 字符宽度计算：启用后 emoji / box-drawing（╔═╗║╚╝）等
  // 列宽按 Unicode 11 表计算，避免默认 Unicode 5 宽度表导致的光标漂移与重影
  // （TUI 应用如 opencode / vim 边框尤其明显）。移动端已加载，桌面端对齐。
  const unicode11 = new Unicode11Addon()
  terminal.loadAddon(unicode11)
  terminal.unicode.activeVersion = '11'

  terminal.open(terminalHostRef.value)

  // 渲染器初始化：按 decideRenderer 决策加载 WebGL（或保持 DOM）并同步
  // DOM 层光标状态（DOM 渲染器下 webglAddon 为 null，后续分支天然跳过）
  renderer.initRenderer(terminal)

  // Linux WebKitGTK IME 防护（仅 Linux；Windows/macOS 走 xterm 原生路径不变）：
  //   1) 关闭 keydown(229) 遗留差值补发路径（_handleAnyTextareaChanges）
  //   2) 组合窗口内精确载荷去重（TerminalImeStateMachine）
  //   3) 组合提交后清空 textarea——xterm 从不清空且 _compositionPosition.start
  //      只在 compositionstart 更新，WebKitGTK 偶发丢失该事件时 finalize 会把
  //      value.substring(旧起点) = 上一轮已提交文本 + 本轮文本 整体发出，即
  //      「按空格提交中文后随机重复之前输入的字符」的根因；清空后起点恒为 0，
  //      拼接型重复不再产生。细节见 utils/terminalLinuxImeGuard.ts。
  if (kernel.isLinux.value) {
    imeGuard = attachLinuxImeGuard(terminal)
  }

  // 移除光标：用 DECTCEM 隐藏序列（\x1b[?25l）在 buffer 层隐藏光标，
  // WebGL 与 DOM 渲染器均不再绘制（TUI 程序主动发送 \x1b[?25h 时除外）
  terminal.write('\x1b[?25l')

  renderer.applyDprFit()
  resize.syncTerminalSize()

  // PTY 尺寸同步：xterm 内部 resize（含 fit 触发）时同步到后端会话
  // （经正统渲染端裁决：本端非正统时服务端返回需要确认，由弹窗处理）
  terminal.onResize(({ cols, rows }) => {
    if (props.session) {
      resize.requestResize(cols, rows)
    }
  })

  // ResizeObserver — 分层防抖（垂直立即 / 水平 100ms 合并，flush 保证最终尺寸必达），
  // 避免拖窗时每帧整屏 reflow；仅当 cols/rows 实际变化时同步 PTY（见 applyResize）。
  // 小缓冲（<200 行，VS Code StartDebouncingThreshold）连宽度变化也立即应用：
  // 新终端/输出少的 shell reflow 便宜，拖窗时网格即时反应不滞后；
  // buffer 行数不可得时保守防抖（terminal 未就绪场景）
  resizeDebouncer = new TerminalResizeDebouncer({
    onApply: () => resize.applyResize(),
    getBufferLength: () =>
      kernel.terminalRef.value ? kernel.terminalRef.value.buffer.active.length : null,
    // 窗口不可见（最小化/切后台）时挂起 resize 应用，恢复可见时 flush 一次性兑现
    // （spec D-6，对齐 VS Code runWhenWindowIdle 分支；模块内不读 document 的
    // Seam A 约定由注入满足，与下方 windowVisibleHandler 的 flush 语义一致）
    isVisible: () => document.visibilityState === 'visible' && !document.hidden,
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
  renderer.watchDprChanges()

  // 滚动状态：xterm onScroll API（比 DOM addEventListener 更可靠，
  // 不会因 xterm 内部 DOM 重建而丢失监听）；仅更新"是否在底部"状态，
  // 重绘完全交给 xterm 渲染循环，不做手动 refresh 补丁
  scroll.setScrollDisposable(
    terminal.onScroll(() => {
      const t = kernel.terminalRef.value
      if (!t) return
      const buffer = t.buffer.active
      const viewportBottom = buffer.viewportY + t.rows
      kernel.isUserScrolling.value = viewportBottom < buffer.length - 1
    }),
  )

  // 选区状态跟踪：有选区时 Ctrl+C 复制（VS Code 终端行为），不发送 SIGINT
  terminal.onSelectionChange(() => {
    hasSelection = !!kernel.terminalRef.value?.getSelection()
  })

  // Ctrl+滚轮缩放字号（VS Code 终端行为）；passive:false 才能阻止默认滚动
  wheelHandler = (e: WheelEvent) => {
    if (!e.ctrlKey) return
    e.preventDefault()
    const idx = TERMINAL_FONT_SIZES.indexOf(settings.fontSize.value)
    const next = Math.min(
      TERMINAL_FONT_SIZES.length - 1,
      Math.max(0, idx < 0 ? 0 : idx + (e.deltaY < 0 ? 1 : -1)),
    )
    settings.fontSize.value = TERMINAL_FONT_SIZES[next]
  }
  terminalHostRef.value.addEventListener('wheel', wheelHandler, { passive: false })

  // 键盘输入
  terminal.onData((data: string) => {
    if (!props.session) return

    // Linux WebKitGTK IME 双发去重：同一组合文本被 xterm 两条路径重复发出时，
    // 丢弃重复载荷（仅 Linux 启用；Windows/macOS 不受影响）
    if (imeGuard && !imeGuard.shouldForward(data)) {
      return
    }

    // 有选区时 Ctrl+C 仅复制（VS Code 终端行为），不向 PTY 发送中断
    if (data === '\x03' && hasSelection) {
      const sel = kernel.terminalRef.value?.getSelection()
      if (sel) {
        navigator.clipboard?.writeText(sel).catch(() => {})
      }
      return
    }

    // 票 08：输入经**本插件命令通道**写入（宿主不再有会话输入命令面）。
    // 提交行重建 + 任务域观察 + `host-pty.write` 全在本插件 WASM 内完成
    // （与互调 api `session-input` 同一实现）；身份令牌 + 激活门由
    // `plugin_invoke` 通道保证。保持迁移前的「尽力投递」语义（不 await、
    // 不阻塞 xterm 渲染管线）。
    void context.commands.execute('session.input', {
      sessionId: props.session.id,
      data,
    })
  })
}

// 会话状态变化：停止/出错时断开输出流；重新运行时恢复订阅
watch(
  () => props.session?.status,
  (status) => {
    if (!sessionId.value) return
    if (status === 'stopped' || status === 'error') {
      detachOutputSource()
    } else if (status === 'running' || status === 'starting') {
      attachOutputSource()
    }
  },
)

onMounted(async () => {
  await nextTick()

  // 确定运行平台（非 Tauri 环境 dev-shell 抛错 → 回退非 Linux）
  try {
    kernel.isLinux.value = platform() === 'linux'
  } catch {
    kernel.isLinux.value = false
  }

  // Linux：等待系统字体加载完成再初始化终端，避免 WebKitGTK 在字体未就绪时
  // 用回退字体测量字符尺寸，导致字符间距过大/模糊；带超时兜底不阻塞首帧。
  // Windows/macOS 保持原有行为（字体即装即用，无需等待）。
  if (kernel.isLinux.value && typeof document !== 'undefined' && 'fonts' in document) {
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
  await settings.resolveBgImageUrl()
  const terminal = kernel.terminalRef.value
  if (terminal) {
    terminal.options.theme = kernel.callbacks.getTheme()
  }

  // Linux：首帧后重测字符尺寸 + 全量重绘（见 scheduleInitialFontRemeasure），
  // 消除 WebKitGTK 首次 measure 用回退字体指标导致的模糊
  renderer.scheduleInitialFontRemeasure()

  // 渲染链路诊断（排查模糊/回退问题时日志可见 renderer 与 DPR）
  console.info(
    `[terminal-session] renderer=${kernel.webglAddonRef.value ? 'webgl' : 'dom'} ` +
      `dpr=${window.devicePixelRatio} fontSize=${settings.fontSize.value} ` +
      `isLinux=${kernel.isLinux.value}`,
  )

  // 输出流：会话 running/starting 时接入（历史回放 + 实时推送同通道流式到达）
  if (props.session?.status === 'running' || props.session?.status === 'starting') {
    attachOutputSource()
  }

  kernel.terminalRef.value?.focus()
})

onUnmounted(() => {
  // 断开输出源轮询（停止拉取与重连）
  detachOutputSource()

  // 清理 xterm onScroll 监听与待处理的滚动 rAF
  scroll.disposeScroll()

  // 清理 Ctrl+滚轮缩放监听
  if (wheelHandler && terminalHostRef.value) {
    terminalHostRef.value.removeEventListener('wheel', wheelHandler)
    wheelHandler = null
  }

  // 清理写入队列（未 flush 的数据仍存于服务端环形，重开窗口可恢复）与回放补刷定时器
  pipeline.dispose()

  // 清理 WebGL atlas 预热迭代（rAF 句柄非 0 时取消）与 DPR 监听
  renderer.disposeRenderer()

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

  // 清理设置保存定时器与外部订阅
  settings.disposeSettingsSync()

  if (resizeObserver) {
    resizeObserver.disconnect()
    resizeObserver = null
  }

  // 清理 Linux IME 防护（拆除监听与未决清空定时器，textarea 随 xterm 一并销毁）
  imeGuard?.dispose()
  imeGuard = null

  if (kernel.terminalRef.value) {
    kernel.terminalRef.value.dispose()
    kernel.terminalRef.value = null
    kernel.webglAddonRef.value = null
  }
})

// ==================== Expose ====================

/** 暴露给父组件：终端窗口模式下外层 header 需要访问的响应式状态和方法 */
defineExpose({
  fontSize: settings.fontSize,
  terminalTheme: settings.terminalTheme,
  themeNames: TERMINAL_THEME_NAMES,
  isUserScrolling: scroll.isUserScrolling,
  clearTerminal: scroll.clearTerminal,
  refreshTerminal: scroll.refreshTerminal,
  scrollToBottomManual: scroll.scrollToBottomManual,
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
   终端区域的定位层，位于背景图片层之上、渲染画布之下。

   透明仅随 terminal-transparent 类生效（镜像 xterm 6.1 的 allow-transparency
   类机制在 6.0 结构上的实现，spec D-3）：背景图开启（allowTransparency=true，
   类绑定于模板容器）时置透明让图片透出；非透明时**不覆盖**，继承 xterm.css 的
   #000——不再像旧版那样把"透明模式才该透明"变成"永远透明"（无条件透明是第二条
   残影通路的放大器）。选择器带 .xterm 前缀，优先级高于 xterm.css 的
   `.xterm .xterm-viewport`，不依赖样式表加载顺序。 */
:deep(.terminal-transparent .xterm-viewport) {
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
