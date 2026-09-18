/**
 * 终端显示域（TerminalView 拆分产物）：xterm 实例装配/销毁 + 主题 + 画面恢复
 *
 * 一块职责：**终端「看起来对不对」**——四组内容同源（都围绕 xterm 实例与画布）：
 *
 * 1. **实例装配与销毁**：构造选项（移动端专用项见 initTerminal 注释）、addon 装配、
 *    实时写入管线接线、TUI 挂钩、触摸滚动接管、首帧 fit 收敛、ResizeObserver
 *    （分层防抖）、DPR 变化监听、onResize → PTY 同步；卸载按相反顺序释放
 * 2. **主题**：字号/主题设置值、'system' → dark/light 解析、应用主题后重排，
 *    以及「用户未手动指定时跟随系统主题」的联动
 * 3. **画面恢复**：清屏、手动刷新（渲染层 + 数据层双管）、合成层强制重绘
 * 4. **选择操作栏定位**：长按选择后浮出「复制/全选/取消」的避让定位（纯派生）
 *
 * 主题解析必须在显示域内完成：xterm 只接受可解析颜色，把 `var()` 串传进去会落回
 * 内置默认色；同一份解析结果还要供容器底色（`--terminal-canvas-bg`，键盘避让域
 * 消费）使用——网格贴合后的顶部/行尾余量区显示的是容器底色，与画布不同色会露色带。
 */
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import type { ITheme } from '@xterm/xterm'
import { computed, ref, watch, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { logger } from '@/utils/frontendLogger'
import { useToast } from '@/composables/useToast'
import { useTheme } from '@/composables/useTheme'
import { useSettingsStore } from '@/stores/settings'
import { useInputAssistantStore } from '@/stores/inputAssistant'
import { useTerminalBufferStore } from '@/stores/terminalBuffer'
import { isMockSession } from '@/composables/useMockTerminal'
import { resolveTerminalTheme } from '@/config/terminalThemes'
import { FONT_FAMILY, TERMINAL_LINE_HEIGHT, TERMINAL_SCROLLBAR_GUTTER_PX } from '@/utils/terminalMetrics'
import { TerminalResizeDebouncer } from '@/utils/terminalResizeDebouncer'
import { TERMINAL_SCROLLBACK } from '@/utils/terminalScrollback'
import type { TerminalKernelContext } from './terminalKernel'
import type { useTerminalRenderer } from './useTerminalRenderer'
import type { useTerminalResize } from './useTerminalResize'
import type { useTerminalSubscription } from './useTerminalSubscription'

// ==================== 首帧 fit 收敛常量 ====================
/** 连续无变化次数达到该值即视为网格稳定 */
const STABLE_FITS = 3
/** 总重试上限：50ms × 40 ≈ 2s，超时放行遮罩门控（防异常态无限循环） */
const MAX_TOTAL_ATTEMPTS = 40
/** 起始延迟（ms）：等 xterm open 后 DOM/字体测量进入可测状态 */
const INITIAL_FIT_DELAY_MS = 50
/** 重试间隔（ms） */
const FIT_RETRY_INTERVAL_MS = 50

// ==================== 选择操作栏定位常量 ====================
/** 操作栏与选区的间距（px） */
const BAR_MARGIN = 10
/** 操作栏与容器边缘的最小留白（px） */
const EDGE_PADDING = 12
/** 操作栏尺寸估算（px）：定位只用于避让，不追求像素精确 */
const ESTIMATED_BAR_WIDTH = 240
const ESTIMATED_BAR_HEIGHT = 40

/** 当前生效的终端显示设置（模板：设置弹窗绑定值；渲染：xterm 构造选项） */
export interface TerminalDisplaySettings {
  fontSize: number
  /** 主题名：可能是 'system'（由 resolvedTerminalTheme 解析为具体色板） */
  theme: string
  /** 是否由用户手动指定（true 时不再跟随系统主题） */
  isThemeUserSet: boolean
}

export interface TerminalDisplayDeps {
  renderer: ReturnType<typeof useTerminalRenderer>
  resize: ReturnType<typeof useTerminalResize>
  subscription: ReturnType<typeof useTerminalSubscription>
  /** 终端缓冲 store：读取 headTrimmed（本地缓存被 LRU 裁剪的提示） */
  bufferStore: ReturnType<typeof useTerminalBufferStore>
  /** 注册实时写入管线（返回本地缓存回放完成信号，接线加载遮罩门控） */
  registerRealtimeHandler: (
    sessionId: string,
    terminal: Terminal,
    onRawOutput?: (data: Uint8Array) => void,
  ) => { replayDone: Promise<void> }
  unregisterRealtimeHandler: (sessionId: string) => void
  /** 触摸滚动接管（viewport 在 open 后即存在，必须无条件挂载） */
  setupViewportScroll: () => void
  /** TUI 兼容：挂接 onWriteParsed 检测备用屏幕 */
  attachTuiCompat: (term: Terminal) => void
  /** TUI 兼容：输出流嗅探喂入 */
  feedTuiOutput: (data: Uint8Array) => void
  disposeTuiCompat: () => void
  disposeScroll: () => void
  /** 数据层续传重拼接（forceReplay，from = 当前游标） */
  forceReplay: (sessionId: string) => void | Promise<void>
  /** 滚动域当前行号（清屏后复位） */
  currentLine: Ref<number>
  /** 滚动域「用户已上翻」标记（清屏后复位，恢复自动跟随） */
  isUserScrolling: Ref<boolean>
  /** 滚动域实测行高（选择操作栏定位；0 = 未测量） */
  cellHeight: Ref<number>
  /** 滚动域选区行号范围 */
  selectionViewportRange: { topRow: number; bottomRow: number }
  /** 滚动域长按触发点坐标 */
  longPressTriggerPos: { x: number; y: number }
  /** 滚动容器（选择操作栏定位基准） */
  scrollContainerRef: Ref<HTMLElement | null>
  /** 挂载时固定的会话 ID：卸载时 route.params 已失效（undefined） */
  mountedSessionId: string
  /** 加载遮罩开关（销毁时复位，防残影） */
  setReady: (ready: boolean) => void
}

export function useTerminalDisplay(ctx: TerminalKernelContext, deps: TerminalDisplayDeps) {
  const { t } = useI18n()
  const toast = useToast()
  const settingsStore = useSettingsStore()
  const assistStore = useInputAssistantStore()
  const { isSystemDark } = useTheme()

  // ==================== 显示设置与主题 ====================

  const terminalSettings = ref<TerminalDisplaySettings>({
    fontSize: assistStore.settings.terminalFontSize,
    theme:
      assistStore.settings.terminalTheme ??
      (settingsStore.settings.ui.theme === 'system'
        ? isSystemDark.value
          ? 'dark'
          : 'light'
        : (settingsStore.settings.ui.theme as string)),
    isThemeUserSet: assistStore.settings.isTerminalThemeUserSet,
  })

  const resolvedTerminalTheme = computed(() =>
    resolveTerminalTheme(terminalSettings.value.theme, isSystemDark.value),
  )

  /** 应用主题到 xterm（整体替换 theme 对象 + 重排重绘） */
  function applyTerminalTheme() {
    const term = ctx.terminalRef.value
    if (!term) return
    term.options.theme = resolvedTerminalTheme.value
    deps.renderer.fitWithMargin()
  }

  /** 跟随系统主题：仅当用户未手动指定时生效（应用主题显式 dark/light 时也同步） */
  function syncFromSystemTheme() {
    if (terminalSettings.value.isThemeUserSet) return
    const uiTheme = settingsStore.settings.ui.theme
    const next = uiTheme === 'system' ? (isSystemDark.value ? 'dark' : 'light') : uiTheme
    if (terminalSettings.value.theme !== next) terminalSettings.value.theme = next
  }

  watch(() => settingsStore.settings.ui.theme, syncFromSystemTheme)
  watch(isSystemDark, syncFromSystemTheme)

  // ==================== 实例观察者状态 ====================

  const resizeObserverRef = ref<ResizeObserver | null>(null)
  /** ResizeObserver rAF 节流句柄：同一帧内多次回调只喂一次防抖器 */
  let resizeRaf = 0
  /** resize 分层防抖器（对齐桌面端 TerminalPreview）：垂直立即 / 水平 100ms 合并 */
  let resizeDebouncer: TerminalResizeDebouncer | null = null

  // ==================== 实例装配 ====================

  async function initTerminal() {
    const container = ctx.xtermContainerRef.value
    if (!container) return

    // 创建前预测量：直接以适配屏幕的行列值构造，不再经过默认 80x24 阶段
    const initial = deps.renderer.computeInitialSize(terminalSettings.value.fontSize ?? 14)

    const term = new Terminal({
      // 渲染器：默认 DOM（xterm 内置 canvas）；USE_WEBGL_RENDERER 开启时
      // WebGL addon 加载成功后自动接管渲染，失败则保持 DOM
      cols: initial.cols,
      rows: initial.rows,
      fontSize: terminalSettings.value.fontSize,
      fontFamily: FONT_FAMILY,
      // 行高倍率（唯一真源 TERMINAL_LINE_HEIGHT）：小屏 CJK 满屏输出行间呼吸感；
      // 与 measureCellSize/computeGridSize 同源，保证预估网格与渲染口径一致
      lineHeight: TERMINAL_LINE_HEIGHT,
      // 滚动历史行数（与桌面主机服务端事件队列容量对齐）
      scrollback: TERMINAL_SCROLLBACK,
      // 自绘滚动条预留宽（唯一真源 TERMINAL_SCROLLBAR_GUTTER_PX）：xterm 6 内部
      // verticalScrollbarSize 与 FitAddon 可用宽扣除同源取此值（缺省 14px，
      // 原生滚动条已被 CSS 隐藏却仍按 14px 预留 → 右侧固定空白竖条）。
      // 设为自绘指示线足迹后，画布右缘与滚动条零重叠且死区收窄到 6px
      overviewRuler: { width: TERMINAL_SCROLLBAR_GUTTER_PX },
      // 默认即时滚动：关闭平滑滚动，避免滚动动画期间合成器缓存旧帧导致重影；
      // 仅在惯性甩动时由 useTerminalScroll 临时开启（smoothScrollDuration）
      // 做单次平滑滑行，滑行结束立即复位为 0
      smoothScrollDuration: 0,
      // VS Code 风格块光标：移动端保留光标（标记输入落点与 TUI 光标位置），
      // DOM 渲染器自带光标层，无需额外处理
      cursorBlink: true,
      cursorStyle: 'block',
      cursorWidth: 1,
      drawBoldTextInBrightColors: true,
      // 移动端特殊处理：禁用 xterm 原生输入。桌面端键盘输入流（onData → PTY）
      // 无法在移动端复现，输入统一由底部 TerminalInputBar 承担，避免软键盘误弹
      disableStdin: true,
      // 主题（'system' 已解析为具体色板，禁止把 var() 串传给 xterm）
      theme: resolvedTerminalTheme.value as ITheme,
      allowProposedApi: true,
    })

    ctx.terminalRef.value = term

    // 挂载 addon（对齐桌面端顺序：addon 先于 open）
    const fitAddon = new FitAddon()
    ctx.fitAddonRef.value = fitAddon
    term.loadAddon(fitAddon)
    term.loadAddon(new WebLinksAddon())

    // Unicode11 addon（移动端特殊处理）：启用 Unicode 11 字符宽度计算。
    // TUI 应用（opencode 等）大量使用 box-drawing 字符（╔═╗║╚╝）和 emoji，
    // 不加载此 addon 时 xterm 默认字符宽度表为 Unicode 5，部分新字符的列宽计算
    // 错误会导致光标位置漂移、上一个写入的字符部分残留（重影）
    const unicode11 = new Unicode11Addon()
    term.loadAddon(unicode11)
    term.unicode.activeVersion = '11'

    term.open(container)

    // 渲染器初始化：按 USE_WEBGL_RENDERER 决策加载 WebGL（或保持 DOM），
    // WebGL 激活后隐藏 DOM 层光标（保留 WebGL 层光标，避免双光标）
    await deps.renderer.initRenderer(term)

    // 注册实时 handler — 历史分片回放（高水位节流，见 useTerminalBuffer）与
    // 实时推送同通道写入；背压 ack 由 useTerminalBuffer 无条件回发（onWriteParsed
    // 即证明本端在消费，不依赖正统归属，见 composable 注释）
    // 回放完成信号接入加载遮罩门控：末批解析完成后才允许撤遮罩
    const { replayDone } = deps.registerRealtimeHandler(ctx.getSessionId(), term, deps.feedTuiOutput)
    void replayDone.then(() => deps.subscription.markReplayDone())

    // 本地历史缓存曾被头部 LRU 裁剪（超 16MB）：本次回放起点非流首，可能切断
    // 转义序列，提示历史不完整（渲染残留由 composable 的回放静止全量重绘兜底）
    if (deps.bufferStore.getBuffer(ctx.getSessionId())?.headTrimmed) {
      toast.warning(t('mobile.terminal.historyTruncated'))
    }

    // TUI 兼容：挂接 onWriteParsed 检测备用屏幕（与嗅探器构成双条件门控）
    deps.attachTuiCompat(term)

    // 触摸滚动接管 + 首帧校准 fit：
    // - setupViewportScroll 不依赖字体测量（viewport 在 open 后即存在于 DOM），
    //   必须无条件挂载，否则触摸滚动/历史查看永久失效
    // - fitWithMargin 幂等（FitAddon 在字体测量未就绪时无操作），轮询重试直至
    //   校准生效；尺寸变化经 onResize → 串行队列发送（自动合并最新值）
    //
    // 收敛语义（修顶部落差）：不"首次变化即停"，而是连续多次无变化才算稳定。
    // 原因：charMeasure 字体度量就绪晚于首次 fit（初始估值偏大 → 行数偏少），
    // 首次 fit 从默认 80×24 变化即停会锁定偏小网格；容器尺寸此后不变时
    // ResizeObserver 不再触发，顶部空带（标题栏与首行之间）无法自愈。
    // 网格贴底对齐后（.xterm bottom:0），行偏少的缺额全部暴露在顶部。
    // 配合 terminalResizePolicy 行双向即时生效，网格收敛到 floor(容器高/行高)。
    setTimeout(() => {
      deps.setupViewportScroll()
      let stableCount = 0
      let totalAttempts = 0
      // 是否已发生过至少一次成功校准：字体度量未就绪时 fit 是 no-op（无变化），
      // 不能据此提前收敛（会把网格锁死在默认 80×24），必须先有一次真实校准
      let everChanged = false
      const tryInitialFit = () => {
        if (!ctx.terminalRef.value) return
        const changed = deps.renderer.fitWithMargin()
        if (changed) {
          // 校准生效：补发一次实际尺寸（队列合并，防 onResize 门控漏发），
          // 并重置稳定计数——尺寸仍在变化（字体度量未稳），继续收敛
          deps.resize.syncTerminalSizeToHost()
          everChanged = true
          stableCount = 0
        } else {
          stableCount++
          if (everChanged && stableCount >= STABLE_FITS) {
            // 已成功校准过且连续多次 fit 无变化：网格已收敛，放行遮罩门控
            deps.subscription.markFirstFitDone()
            return
          }
        }
        if (++totalAttempts < MAX_TOTAL_ATTEMPTS) {
          setTimeout(tryInitialFit, FIT_RETRY_INTERVAL_MS)
        } else {
          // 收敛超时：放弃继续校准并放行遮罩门控（后续尺寸由 ResizeObserver 兜底）
          deps.subscription.markFirstFitDone()
        }
      }
      tryInitialFit()
    }, INITIAL_FIT_DELAY_MS)

    // ResizeObserver — 接入分层防抖（对齐桌面端 TerminalPreview / VS Code）：
    // 垂直 resize（行数变化）立即应用，仅宽度变化 100ms 防抖合并，避免旋转/键盘
    // 避让触发容器尺寸微调时每帧整屏 reflow；0/非法尺寸（隐藏/过渡中 RO 报 0）被
    // 防抖器忽略，避免把 PTY 缩成 1×1 打乱 shell。小缓冲（<200 行，VS Code
    // StartDebouncingThreshold）连宽度变化也立即应用。flush() 保证防抖窗口内最后
    // 一次尺寸必达。仅 cols/rows 实际变化才同步 PTY（见 applyResize）。
    resizeDebouncer = new TerminalResizeDebouncer({
      onApply: () => deps.resize.applyResize(),
      getBufferLength: () => {
        const current = ctx.terminalRef.value
        return current ? current.buffer.active.length : null
      },
    })
    resizeObserverRef.value = new ResizeObserver((entries) => {
      const rect = entries[0]?.contentRect
      if (rect) {
        // rAF 聚合同一帧的多次回调，再喂给防抖器（采集侧节流，防抖器负责应用侧调度）
        if (resizeRaf) return
        resizeRaf = requestAnimationFrame(() => {
          resizeRaf = 0
          resizeDebouncer?.resize(rect.width, rect.height)
        })
      }
    })
    resizeObserverRef.value.observe(container)

    // DPR 动态变化监听（跨 DPI 旋转 / 系统缩放变化时窗口尺寸可能不变，
    // ResizeObserver 不触发）：matchMedia 只能匹配固定 dppx 值，变化后按新值递归注册
    deps.renderer.watchDprChanges()

    // PTY 尺寸同步：xterm 内部 resize（含 fit 触发）时同步到主机会话。
    // 统一走 HTTP 串行队列（resize 域 queueResize）：HTTP 与 WS 双通道并发会把不同
    // 尺寸的请求乱序送达服务端——fit 前的 80x24 默认值若后到会覆盖实际尺寸，
    // PTY 停在 80x24 → opencode 按 24 行渲染，显示区下半黑（半屏黑）
    term.onResize(({ cols, rows }) => {
      logger.debug(`[TerminalView] onResize: ${cols}x${rows}`)
      deps.resize.queueResize(cols, rows)
    })
  }

  /** 实例销毁：观察者 → 域资源 → 写入管线 → xterm（顺序与装配相反） */
  function disposeTerminal() {
    if (resizeObserverRef.value) {
      resizeObserverRef.value.disconnect()
      resizeObserverRef.value = null
    }
    if (resizeRaf) {
      cancelAnimationFrame(resizeRaf)
      resizeRaf = 0
    }
    // 清理 resize 分层防抖器（丢掉挂起的应用）
    resizeDebouncer?.dispose()
    resizeDebouncer = null
    // 清理渲染器域资源（atlas 预热定时器 + DPR 变化监听）
    deps.renderer.disposeRenderer()

    // 卸载时 route.params 已失效（undefined），须用挂载时固定的会话 ID，
    // 否则 handler 注销被守卫跳过 → 残留闭包引用已 dispose 的 xterm
    if (deps.mountedSessionId) {
      deps.unregisterRealtimeHandler(deps.mountedSessionId)
    }

    deps.disposeTuiCompat()
    deps.disposeScroll()

    if (ctx.terminalRef.value) {
      ctx.terminalRef.value.dispose()
      ctx.terminalRef.value = null
      ctx.fitAddonRef.value = null
    }
    deps.setReady(false)
  }

  // ==================== 画面恢复 ====================

  /**
   * 合成层强制重绘：1px transform 往返抖动，迫使 WebView 合成器重新合成 canvas 层。
   * xterm 渲染管线挂起（脏区跳过等）时 refresh() 不生效，DOM transform 变化能绕过
   * 渲染管线直接触发合成器重绘（实测：键盘避让后黑屏恢复）。
   * 读取当前生效 transform（容器上可能已有 translateY），往返后还原。
   */
  function forceCompositorRepaint() {
    const el = ctx.xtermContainerRef.value
    if (!el) return
    const current = getComputedStyle(el).transform
    el.style.transform = 'translateY(1px)'
    requestAnimationFrame(() => {
      el.style.transform = current
    })
  }

  /** 清屏：仅渲染层（同实例 scrollback 保留在 xterm 内，不影响服务端） */
  function clearTerminal() {
    const term = ctx.terminalRef.value
    if (!term) return
    term.clear()
    deps.currentLine.value = 0
    deps.isUserScrolling.value = false
  }

  /**
   * 手动刷新（工具栏 refresh 入口）：渲染层恢复 + 数据层兜底。
   * 用户显式刷新 = 明确意图：清空「拒绝覆盖尺寸」记录，让尺寸仲裁重跑一遍
   * （否则同尺寸请求被永久抑制，PTY 尺寸再也不会被纠正）。
   */
  async function refreshTerminal() {
    const term = ctx.terminalRef.value
    if (!ctx.fitAddonRef.value || !term) return

    deps.renderer.fitWithMargin()
    // 强制重绘可见区：fit 尺寸不变时不触发重排，渲染残留需要手动刷新
    if (term.rows > 0) {
      term.refresh(0, term.rows - 1)
    }
    forceCompositorRepaint()

    if (ctx.isConnected() && ctx.isSessionActive()) {
      const sid = ctx.getSessionId()
      deps.resize.clearRejectedSize()
      // 统一走串行队列（过滤未校准默认值 + 单通道保序），失败仅 console.warn
      deps.resize.queueResize(term.cols, term.rows)
      // 数据层兜底：渲染层恢复后内容仍缺失（violation 风暴期间帧被拒）时
      // 续传重拼接（forceReplay from=游标——同实例 scrollback 仍在，无需全量）
      if (!isMockSession(sid)) {
        // 订阅信念对账：Rust 幂等订阅不发状态事件，长时间未收事件的会话需主动
        // 拉状态收敛（否则刷新后输入仍可能被 subscribed 门控拒绝）
        await deps.bufferStore.reconcileState(sid)
        await deps.forceReplay(sid)
        await deps.subscription.subscribeWithRetry()
      }
    }
    toast.success(t('mobile.terminal.refreshed'))
  }

  // ==================== 选择操作栏定位 ====================

  /**
   * 长按选择后浮出「复制 / 全选 / 取消」操作栏：优先放选区上方，上方空间不足则放
   * 下方，都不行则按选区偏上/偏下贴最近边缘；水平以长按位置为中心并夹在容器内。
   * 纯派生（依赖容器 rect + 行高 + 选区行号 + 长按坐标），故为 computed。
   */
  const selectionBarStyle = computed(() => {
    const container = deps.scrollContainerRef.value
    if (!container) return {} as Record<string, string>

    const rect = container.getBoundingClientRect()
    let selTop = 0
    let selBottom = 0
    if (deps.cellHeight.value > 0) {
      const rows = ctx.terminalRef.value?.rows ?? 0
      const topRow = Math.max(0, deps.selectionViewportRange.topRow)
      const bottomRow = Math.min(rows || topRow, deps.selectionViewportRange.bottomRow + 1)
      selTop = topRow * deps.cellHeight.value
      selBottom = bottomRow * deps.cellHeight.value
    }

    const relX = deps.longPressTriggerPos.x - rect.left
    const left = Math.max(
      EDGE_PADDING,
      Math.min(relX - ESTIMATED_BAR_WIDTH / 2, rect.width - ESTIMATED_BAR_WIDTH - EDGE_PADDING),
    )

    const aboveTop = selTop - ESTIMATED_BAR_HEIGHT - BAR_MARGIN
    const belowTop = selBottom + BAR_MARGIN
    let top: number
    if (aboveTop >= EDGE_PADDING) {
      top = aboveTop
    } else if (belowTop + ESTIMATED_BAR_HEIGHT <= rect.height - EDGE_PADDING) {
      top = belowTop
    } else if (selTop < rect.height / 2) {
      top = rect.height - ESTIMATED_BAR_HEIGHT - EDGE_PADDING
    } else {
      top = EDGE_PADDING
    }

    return { top: `${top}px`, left: `${left}px` }
  })

  return {
    terminalSettings,
    resolvedTerminalTheme,
    applyTerminalTheme,
    initTerminal,
    disposeTerminal,
    forceCompositorRepaint,
    clearTerminal,
    refreshTerminal,
    selectionBarStyle,
  }
}
