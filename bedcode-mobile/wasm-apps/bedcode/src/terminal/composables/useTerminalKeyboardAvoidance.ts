/**
 * 终端键盘避让域（TerminalView 拆分产物，移动端特有）
 *
 * 双通道键盘检测，兼容不同 Android WebView 实现：
 * - 通道 1 (visualViewport)：部分 WebView 在键盘弹出时 visualViewport.height 缩小，
 *   通过 resize/scroll 事件检测，计算 fullLayoutHeight - viewportHeight 得到偏移量
 * - 通道 2 (插件 keyboardHeight)：部分 WebView 的 visualViewport 不触发事件，
 *   通过 tauri-plugin-edge-to-edge 的 safeAreaChanged 事件获取插件报告的键盘高度
 *
 * 避让语义（2026-10-01 由 resize 改为 lift）：**向上平移**——把 `.movable-area`
 * （终端显示区 + 输入栏）整体 `translateY(-keyboardOffset)`，与快捷键面板弹出
 * 避让（useTerminalScroll 的 xtermContainerStyle）同一手法：
 *   - 终端网格**不动**：cols/rows/PTY 尺寸全部不变 ⇒ TUI 不重画、缓冲区不回流、
 *     没有「键盘弹出瞬间整屏重排 + 重新发一次 resize」的开销
 *   - 原本被键盘遮住的下半部分被整体移出可视区，输入栏底边落在键盘上沿
 *   - 代价：终端顶部 keyboardOffset 高度的一段被 `.movable-clip`（overflow:hidden）
 *     裁在 Header 之下——「显示最后若干行」而非「显示全部行的前若干行」。
 *     横屏 + 键盘时可见行数会明显变少（见 CSS 内 .movable-clip 注释）。
 * 不做根容器高度收缩：那是旧的 resize 语义，已按需求注释保留在文件末尾
 * 「已停用：resize 语义避让」段，恢复时把 terminalViewStyle 的 height 改回去即可。
 *
 * ⚠ lift 成立的前提：布局视口（ICB）在键盘弹出时**不变**（AndroidManifest
 * windowSoftInputMode=adjustNothing，键盘浮在 WebView 之上），因此
 * `100vh` 恒等于屏幕全高，平移 keyboardOffset 恰好让输入栏底边贴上键盘上沿。
 * 若某个 WebView 真按 index.html 的 `interactive-widget=resizes-content` 收缩了
 * ICB，则布局本身已避让键盘，再平移一次就是双重补偿（输入条悬空）——那种设备上
 * 本式的偏移会明显大于真实键盘高度（vvOffset = 冻结基准 − 已收缩的 innerHeight），
 * 见到这个现象先查 ICB 是否收缩，而不是加补偿系数。
 *
 * 另含页面 pan 守卫（阻断整页被拖向键盘、标题/输入条滑出）。
 */
import { computed, ref, watch, type Ref } from 'vue'
import { attachViewportPanGuard, type ViewportPanGuard } from './useViewportPanGuard'

/**
 * 「键盘可见」判定阈值（px）：低于该值的偏移视为无键盘。
 * 该值使 keyboardOffset 只取 0 或 >10 的离散值，因此「从可见归零」等价于
 * 「prev > 10 且 now <= 10」（原实现拆成两个 watcher，条件等价，此处合并为一次回调）。
 */
export const KEYBOARD_VISIBLE_THRESHOLD = 10

export interface TerminalKeyboardAvoidanceOptions {
  /** 根容器（.terminal-view）模板 ref：承载安全区 padding 与 pan 守卫 */
  rootRef: Ref<HTMLElement | null>
  /** 顶部安全区高度（px） */
  safeAreaTop: () => number
  /** 终端画布底色：网格贴合后顶部余量/行尾余量区显示的是容器底色，须与画布同色 */
  canvasBackground: () => string
  /** 选择模式取色框颜色（描边）：跟随当前终端主题，避免浅色主题下配色突兀 */
  selectionFrame: () => string
  /**
   * 键盘收起（偏移从可见归零）时回调：Android 返回键/下拉手势收起键盘时 WebView
   * 的输入框仍保有焦点，须主动退出编辑态（blur + 收缩单行 + 关补全弹层）；同时
   * 终端显示区还原，需滚回最新行（键盘弹出期间用户可能已上翻）。
   */
  onKeyboardHide: () => void
}

export function useTerminalKeyboardAvoidance(options: TerminalKeyboardAvoidanceOptions) {
  /** 通道 1 基准：无键盘时的布局视口高度 */
  const fullLayoutHeight = ref(window.innerHeight)
  const viewportHeight = ref(window.visualViewport?.height ?? window.innerHeight)
  /** 通道 2：插件上报的键盘高度 */
  const pluginKeyboardHeight = ref(0)
  /** 侧边栏设置面板输入框聚焦时，禁用键盘避让 */
  const settingsInputFocused = ref(false)
  let panGuard: ViewportPanGuard | null = null

  /**
   * 最终键盘偏移量：visualViewport 优先（逐帧跟踪真实遮挡高度），插件高度兜底
   * （部分 WebView 的 vv 不触发事件）。不用 Math.max：插件在键盘动画 onStart 即
   * 上报最终高度，取大值会让偏移在动画开始瞬间跳到终态——输入条先于键盘到位，
   * 底部短暂露出背景空隙；vv 可用时它就是当前真实遮挡量。
   */
  const keyboardOffset = computed(() => {
    if (settingsInputFocused.value) return 0
    const vvOffset = fullLayoutHeight.value - viewportHeight.value
    if (vvOffset > KEYBOARD_VISIBLE_THRESHOLD) return vvOffset
    return pluginKeyboardHeight.value > KEYBOARD_VISIBLE_THRESHOLD ? pluginKeyboardHeight.value : 0
  })

  /** 通道 1 回调：visualViewport resize/scroll */
  function handleVisualViewportChange() {
    const vv = window.visualViewport
    if (!vv) return
    // 无键盘时更新基准高度（键盘弹出期间基准必须冻结，否则偏移恒为 0）
    if (!keyboardOffset.value) {
      fullLayoutHeight.value = window.innerHeight
    }
    viewportHeight.value = vv.height
  }

  /** 通道 2 回调：插件 safeAreaChanged 事件 */
  function handlePluginSafeAreaChange(e: Event) {
    const detail = (e as CustomEvent).detail as {
      keyboardHeight: number
      keyboardVisible: boolean
    }
    pluginKeyboardHeight.value = detail.keyboardVisible ? detail.keyboardHeight : 0
  }

  /**
   * terminal-view 只负责安全区域与主题变量（画布底色 / 选区描边）：
   * 键盘避让不再改这里的高度——避让由 movable-area 的 transform 承担。
   */
  const terminalViewStyle = computed(() => ({
    paddingTop: `${options.safeAreaTop()}px`,
    '--terminal-canvas-bg': options.canvasBackground(),
    '--terminal-selection-frame': options.selectionFrame(),
  }))

  /**
   * 键盘避让（lift）：可移动区域（终端显示区 + 输入栏）整体上移一个键盘高度。
   *
   * - 位移量 = 实测遮挡高度（keyboardOffset），与快捷键面板避让同源同法；
   * - 归零时返回空对象（不留 `transform: translateY(0)` 残值），让元素回到
   *   未变换状态，避免合成层保留；
   * - 不加 transition：visualViewport 的 resize 事件在键盘动画期间逐帧触发，
   *   平移本身已跟手，再叠 250ms 过渡只会让终端滞后于键盘。
   */
  const movableAreaStyle = computed(() => {
    const offset = keyboardOffset.value
    return offset > 0 ? { transform: `translateY(-${offset}px)` } : {}
  })

  // 键盘收起统一回调（详见 KEYBOARD_VISIBLE_THRESHOLD 注释）
  watch(keyboardOffset, (offset, prev) => {
    if ((prev ?? 0) > KEYBOARD_VISIBLE_THRESHOLD && offset <= KEYBOARD_VISIBLE_THRESHOLD) {
      options.onKeyboardHide()
    }
  })

  /** 事件监听与 pan 守卫挂载（onMounted） */
  function attach() {
    if (window.visualViewport) {
      window.visualViewport.addEventListener('resize', handleVisualViewportChange)
      window.visualViewport.addEventListener('scroll', handleVisualViewportChange)
    }
    window.addEventListener('safeAreaChanged', handlePluginSafeAreaChange as EventListener)
    if (options.rootRef.value) {
      panGuard = attachViewportPanGuard(options.rootRef.value)
    }
  }

  /** 事件监听与 pan 守卫卸载（onUnmounted） */
  function dispose() {
    if (window.visualViewport) {
      window.visualViewport.removeEventListener('resize', handleVisualViewportChange)
      window.visualViewport.removeEventListener('scroll', handleVisualViewportChange)
    }
    window.removeEventListener('safeAreaChanged', handlePluginSafeAreaChange as EventListener)
    panGuard?.dispose()
    panGuard = null
  }

  /** 侧边栏设置面板输入框聚焦/失焦：聚焦期间禁用键盘避让 */
  function setSettingsInputFocused(focused: boolean) {
    settingsInputFocused.value = focused
  }

  return {
    keyboardOffset,
    terminalViewStyle,
    movableAreaStyle,
    attach,
    dispose,
    setSettingsInputFocused,
  }
}

/* ==================== 已停用：resize 语义避让（2026-10-01 停用，注释保留） ====================
 *
 * 原实现（终端显示区随根容器高度收缩 → ResizeObserver 重新 fit → 行数实时重算并
 * 同步 PTY）。停用原因：键盘每弹/收一次都要重排整块画布并重发一次 PTY resize，
 * TUI 会整屏重画；且行数变化本身让「输入条上方那几行」内容跳动。
 * 现改为 movable-area 的 translateY 上移（见 movableAreaStyle），网格尺寸恒定。
 *
 * 恢复方式（若真机证明 lift 的「顶部被裁」不可接受）：
 *   1) terminalViewStyle 加回下面这行 height（与 padding-top 同时存在时 bottom
 *      会被忽略，必须用 height 而不是 bottom——CSS over-constrained）：
 *        height: keyboardOffset.value > 0 ? `calc(100vh - ${keyboardOffset.value}px)` : '100vh',
 *   2) TerminalView 删掉 .movable-area 上的 :style="movableAreaStyle"；
 *   3) 快捷键面板仍走 xterm 容器 transform，两者互不影响。
 *
 *   const terminalViewStyle = computed(() => ({
 *     paddingTop: `${options.safeAreaTop()}px`,
 *     '--terminal-canvas-bg': options.canvasBackground(),
 *     '--terminal-selection-frame': options.selectionFrame(),
 *     height: keyboardOffset.value > 0 ? `calc(100vh - ${keyboardOffset.value}px)` : '100vh',
 *   }))
 * ======================================================================================= */
