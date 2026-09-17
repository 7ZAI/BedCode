/**
 * 终端键盘避让域（TerminalView 拆分产物，移动端特有）
 *
 * 双通道键盘检测，兼容不同 Android WebView 实现：
 * - 通道 1 (visualViewport)：部分 WebView 在键盘弹出时 visualViewport.height 缩小，
 *   通过 resize/scroll 事件检测，计算 fullLayoutHeight - viewportHeight 得到偏移量
 * - 通道 2 (插件 keyboardHeight)：部分 WebView 的 visualViewport 不触发事件，
 *   通过 tauri-plugin-edge-to-edge 的 safeAreaChanged 事件获取插件报告的键盘高度
 *
 * 避让语义：`terminal-view` 根容器高度收缩（height = 100vh - keyboardOffset），
 * 终端显示区随 flex 收缩 → ResizeObserver 重新 fit → 行数实时重算并同步 PTY。
 * 不做 transform/padding 抬升：布局视口与可视区等高，WebView 的聚焦呈现
 * （visual viewport pan）无空间触发，避免双重补偿把输入条悬空。
 *
 * 另含页面 pan 守卫（阻断整页被拖向键盘、标题/输入条滑出）。
 */
import { computed, ref, watch, type Ref } from 'vue'
import { attachViewportPanGuard, type ViewportPanGuard } from '@/composables/useViewportPanGuard'

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
  /**
   * 键盘收起（偏移从可见归零）时回调：Android 返回键/下拉手势收起键盘时 WebView
   * 的输入框仍保有焦点，须主动退出编辑态（blur + 收缩单行 + 关补全弹层）；同时
   * 终端显示区高度还原、行数增多，需滚回最新行（键盘弹出期间用户可能已上翻）。
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
   * terminal-view 负责安全区域 + 键盘避让：键盘弹出时收缩根容器高度
   * （height = 100vh - keyboardOffset）。不能只写 bottom——CSS 里 height:100vh
   * 与 top 同时存在时 bottom 被忽略（over-constrained），收缩不生效。
   */
  const terminalViewStyle = computed(() => ({
    paddingTop: `${options.safeAreaTop()}px`,
    '--terminal-canvas-bg': options.canvasBackground(),
    height: keyboardOffset.value > 0 ? `calc(100vh - ${keyboardOffset.value}px)` : '100vh',
  }))

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
    attach,
    dispose,
    setSettingsInputFocused,
  }
}
