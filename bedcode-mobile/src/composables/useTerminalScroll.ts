/**
 * useTerminalScroll - 终端触摸滚动 + 选择模式 composable
 *
 * 封装 xterm 终端的触摸滚动（含惯性）、自定义滚动条、长按选择模式等逻辑。
 * 不拥有 Terminal/FitAddon 实例，通过参数接收 ref。
 */

import { ref, reactive, computed, nextTick, watch, type Ref } from 'vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { writeClipboardText } from '@/utils/clipboard'
import { useToast } from '@/composables/useToast'
import i18n from '@/locales'

/// 长按阈值（毫秒）
const LONG_PRESS_DURATION = 500
/// 长按移动容差（像素）
const LONG_PRESS_MOVE_THRESHOLD = 10

export function useTerminalScroll(
  terminalRef: Ref<Terminal | null>,
  scrollContainerRef: Ref<HTMLDivElement | null>,
) {
  const toast = useToast()

  // ==================== Scroll State ====================

  const currentLine = ref(0)
  const cellHeight = ref(0)
  const isUserScrolling = ref(false)
  const scrollbarVisible = ref(false)
  const isSelectionMode = ref(false)
  const hasSelection = ref(false)
  /** 选择模式下手指是否已抬起（框选完成后才显示操作栏） */
  const selectionTouchEnded = ref(false)
  const shortcutsPanelHeight = ref(0)

  const touchState = reactive({
    hideTimer: null as ReturnType<typeof setTimeout> | null,
    inertiaRafId: 0,
    startY: 0,
    startLine: 0,
    lastY: 0,
    lastTime: 0,
    velocity: 0,
    fractionalLine: 0,
  })

  // 长按检测
  const longPressTimer = ref<ReturnType<typeof setTimeout> | null>(null)
  const longPressStartPos = reactive({ x: 0, y: 0 })
  /** 长按触发时的客户端坐标，供 UI 定位弹窗 */
  const longPressTriggerPos = reactive({ x: 0, y: 0 })
  /** 选区在 viewport 中的可视行范围（相对于 scrollContainer），供 UI 避让选区定位 */
  const selectionViewportRange = reactive({ topRow: 0, bottomRow: 0 })
  let selectionStartLine = 0
  let selectionStartCol = 0
  let selectionPollRaf = 0

  // rAF 节流滚动
  let pendingScrollRaf = 0
  let pendingScrollLine = -1
  // 滚动后强制重绘可见区：清除 WebGL 渲染器滚动遗留的重影纹理行（每帧至多一次）
  let pendingScrollRefreshRaf = 0
  // 渲染帧同步：确保 scrollToLine 只在 xterm 渲染完成后执行
  // WebGL 渲染器双缓冲在渲染未完成时切换 viewport 会导致新旧帧同时可见
  let renderSyncRaf = 0

  // ==================== Computed ====================

  const scrollbarThumbStyle = computed(() => {
    if (!terminalRef.value) return { top: '0%', height: '0%' }

    const bufferLength = terminalRef.value.buffer.active.length
    const rows = terminalRef.value.rows
    if (bufferLength <= 0 || rows <= 0) return { top: '0%', height: '100%' }

    const scrollableLines = bufferLength - rows
    if (scrollableLines <= 0) return { top: '0%', height: '100%' }

    const thumbRatio = rows / bufferLength
    const thumbHeight = Math.max(0.08, Math.min(0.8, thumbRatio))

    const scrollRatio = currentLine.value / scrollableLines
    const top = scrollRatio * (1 - thumbHeight)

    return {
      top: `${(top * 100).toFixed(1)}%`,
      height: `${(thumbHeight * 100).toFixed(1)}%`,
    }
  })

  // xterm-container 的 transition 只在面板高度动画期间启用
  // 持续开启会导致 xterm-container 被 GPU 提升为合成层
  // 触摸滚动时 WebGL canvas 在合成层上更新不同步，产生重影
  const xtermTransitionActive = ref(false)
  let xtermTransitionTimer: ReturnType<typeof setTimeout> | null = null

  const xtermContainerStyle = computed(() => {
    const height = shortcutsPanelHeight.value
    return {
      transform: `translateY(-${height}px)`,
      transition: xtermTransitionActive.value ? 'transform 0.25s cubic-bezier(0.4, 0, 0.2, 1)' : 'none',
    }
  })

  // 监听面板高度变化，临时启用 transition，动画结束后移除
  watch(shortcutsPanelHeight, () => {
    xtermTransitionActive.value = true
    if (xtermTransitionTimer) clearTimeout(xtermTransitionTimer)
    xtermTransitionTimer = setTimeout(() => {
      xtermTransitionActive.value = false
      xtermTransitionTimer = null
    }, 300)
  })

  // ==================== Scroll Helpers ====================

  function computeCellHeight(): number {
    if (!terminalRef.value?.element) return 0
    const viewport = terminalRef.value.element.querySelector('.xterm-viewport') as HTMLElement
    if (viewport && terminalRef.value.rows > 0) {
      return viewport.clientHeight / terminalRef.value.rows
    }
    return 0
  }

  function isScrolledToBottom(): boolean {
    if (!scrollContainerRef.value || !terminalRef.value) return true
    const maxLine = terminalRef.value.buffer.active.length - terminalRef.value.rows
    return currentLine.value >= maxLine - 2
  }

  /**
   * 自动跟随输出滚动到底部（VSCode 式）
   *
   * - 触摸滚动期间忽略：用户已接管滚动，不被新输出拉回底部
   * - rAF 节流：同帧多次调用只滚一次，输出持续增长时更新目标行
   */
  function scrollToBottom() {
    if (!terminalRef.value || isUserScrolling.value) return

    const bufferLength = terminalRef.value.buffer.active.length
    const rows = terminalRef.value.rows
    const targetLine = Math.max(0, bufferLength - rows)

    if (pendingScrollRaf) {
      // 同帧已挂起滚动：输出可能已增长，只更新目标行
      pendingScrollLine = targetLine
      return
    }

    pendingScrollLine = targetLine
    currentLine.value = targetLine
    pendingScrollRaf = requestAnimationFrame(() => {
      pendingScrollRaf = 0
      const target = pendingScrollLine
      pendingScrollLine = -1
      // 执行时复查：触摸已接管则放弃自动滚动
      if (terminalRef.value && target >= 0 && !isUserScrolling.value) {
        terminalRef.value.scrollToLine(target)
        scheduleScrollRefresh()
      }
    })
  }

  /** 用户点击"回到底部"：强制滚到底并恢复自动跟随（不受触摸状态影响） */
  function scrollToBottomManual() {
    if (!terminalRef.value) return

    isUserScrolling.value = false
    const bufferLength = terminalRef.value.buffer.active.length
    const rows = terminalRef.value.rows
    const targetLine = Math.max(0, bufferLength - rows)

    if (pendingScrollRaf) {
      cancelAnimationFrame(pendingScrollRaf)
      pendingScrollRaf = 0
    }
    pendingScrollLine = -1
    currentLine.value = targetLine
    terminalRef.value.scrollToLine(targetLine)
    scheduleScrollRefresh()
  }

  function syncViewportToLine(line: number) {
    if (!terminalRef.value) return

    const bufferLength = terminalRef.value.buffer.active.length
    const rows = terminalRef.value.rows
    const maxLine = Math.max(0, bufferLength - rows)

    const clampedLine = Math.max(0, Math.min(line, maxLine))
    currentLine.value = clampedLine
    pendingScrollLine = clampedLine

    // 渲染帧同步调度：
    // WebGL 渲染器使用双缓冲，scrollToLine 同步修改 buffer ydisp 但渲染异步执行
    // 如果在渲染未完成时再次 scrollToLine，新旧帧内容会同时可见（重影）
    // 使用 rAF 节流确保每帧最多执行一次 scrollToLine，
    // 并在 scrollToLine 后等待渲染完成再允许下一次滚动
    // 配合 terminal.smoothScrollDuration = 0 关闭补间动画，避免多帧重叠
    if (!pendingScrollRaf) {
      pendingScrollRaf = requestAnimationFrame(() => {
        pendingScrollRaf = 0
        if (terminalRef.value && pendingScrollLine >= 0) {
          const targetLine = pendingScrollLine
          pendingScrollLine = -1
          terminalRef.value.scrollToLine(targetLine)
        }
      })
    }

    showScrollbar()
  }

  function showScrollbar() {
    scrollbarVisible.value = true
    if (touchState.hideTimer) {
      clearTimeout(touchState.hideTimer)
    }
    touchState.hideTimer = setTimeout(() => {
      scrollbarVisible.value = false
    }, 1200)
  }

  /** 滚动后强制重绘可见区：清除 WebGL 渲染器滚动遗留的重影纹理行（每帧至多一次） */
  function scheduleScrollRefresh() {
    if (!terminalRef.value || pendingScrollRefreshRaf) return
    pendingScrollRefreshRaf = requestAnimationFrame(() => {
      pendingScrollRefreshRaf = 0
      if (terminalRef.value) {
        terminalRef.value.refresh(0, terminalRef.value.rows - 1)
      }
    })
  }

  // ==================== Touch Handlers ====================

  function onTouchStart(e: TouchEvent) {
    if (isSelectionMode.value) {
      // 手指按下时隐藏操作栏，等重新抬起后再显示
      selectionTouchEnded.value = false
      const touch = e.touches[0]
      longPressStartPos.x = touch.clientX
      longPressStartPos.y = touch.clientY

      if (terminalRef.value?.element && cellHeight.value > 0) {
        const viewport = terminalRef.value.element.querySelector('.xterm-viewport') as HTMLElement
        if (viewport) {
          const rect = viewport.getBoundingClientRect()
          const relY = touch.clientY - rect.top
          const relX = touch.clientX - rect.left
          const visibleRow = Math.max(0, Math.min(Math.floor(relY / cellHeight.value), terminalRef.value.rows - 1))
          const cellWidth = terminalRef.value.cols > 0 ? rect.width / terminalRef.value.cols : 8
          const col = Math.max(0, Math.min(Math.floor(relX / cellWidth), terminalRef.value.cols - 1))
          const bufferLine = terminalRef.value.buffer.active.viewportY + visibleRow
          selectionStartLine = bufferLine
          selectionStartCol = col
        }
      }
      return
    }

    if (touchState.inertiaRafId) {
      cancelAnimationFrame(touchState.inertiaRafId)
      touchState.inertiaRafId = 0
    }

    const touch = e.touches[0]
    touchState.startY = touch.clientY
    touchState.startLine = currentLine.value
    touchState.lastY = touch.clientY
    touchState.lastTime = Date.now()
    touchState.velocity = 0
    touchState.fractionalLine = 0

    // 触摸即接管滚动：暂停输出自动跟随，避免手势被新输出拉回底部
    isUserScrolling.value = true

    enableGpuHint()

    longPressStartPos.x = touch.clientX
    longPressStartPos.y = touch.clientY
    if (longPressTimer.value) clearTimeout(longPressTimer.value)
    longPressTimer.value = setTimeout(() => {
      longPressTimer.value = null
      enterSelectionMode()
    }, LONG_PRESS_DURATION)
  }

  function onTouchMove(e: TouchEvent) {
    if (isSelectionMode.value) {
      extendSelectionToTouch(e.touches[0])
      return
    }

    if (longPressTimer.value) {
      const touch = e.touches[0]
      const dx = Math.abs(touch.clientX - longPressStartPos.x)
      const dy = Math.abs(touch.clientY - longPressStartPos.y)
      if (dx > LONG_PRESS_MOVE_THRESHOLD || dy > LONG_PRESS_MOVE_THRESHOLD) {
        clearTimeout(longPressTimer.value)
        longPressTimer.value = null
      }
    }

    if (!terminalRef.value || cellHeight.value <= 0) return

    const touch = e.touches[0]
    const deltaY = touch.clientY - touchState.lastY
    const deltaTime = Date.now() - touchState.lastTime

    if (deltaTime > 0) {
      touchState.velocity = deltaY / deltaTime
    }

    touchState.lastY = touch.clientY
    touchState.lastTime = Date.now()

    const rawLines = -deltaY / cellHeight.value
    const totalLines = rawLines + touchState.fractionalLine
    const linesDelta = Math.trunc(totalLines)

    if (linesDelta === 0) {
      touchState.fractionalLine = totalLines
      return
    }

    touchState.fractionalLine = totalLines - linesDelta
    const newLine = currentLine.value + linesDelta
    isUserScrolling.value = true
    syncViewportToLine(newLine)
  }

  function onTouchEnd() {
    if (longPressTimer.value) {
      clearTimeout(longPressTimer.value)
      longPressTimer.value = null
    }

    if (isSelectionMode.value) {
      if (!terminalRef.value?.hasSelection()) {
        exitSelectionMode()
      } else {
        // 框选完成，手指抬起，可以显示操作栏
        selectionTouchEnded.value = true
      }
      return
    }

    if (!terminalRef.value || cellHeight.value <= 0) {
      disableGpuHint()
      return
    }

    startInertia()
  }

  // ==================== GPU Hint ====================
  // 不再对 .xterm-screen 设置 will-change: transform
  // 持续开启会导致 xterm scrollToLine 时新旧帧同时可见（重影）
  // 行级滚动无需亚像素渲染，xterm 内部渲染器已足够高效

  function enableGpuHint() {
    // no-op: will-change 会导致 xterm 滚动重影
  }

  function disableGpuHint() {
    // no-op: will-change 会导致 xterm 滚动重影
  }

  // ==================== Inertia Scroll ====================

  function startInertia() {
    if (Math.abs(touchState.velocity) < 0.02) {
      if (isScrolledToBottom()) {
        isUserScrolling.value = false
      }
      disableGpuHint()
      return
    }

    const friction = 0.95

    function step() {
      if (!terminalRef.value || cellHeight.value <= 0) {
        touchState.inertiaRafId = 0
        disableGpuHint()
        return
      }

      touchState.velocity *= friction
      if (Math.abs(touchState.velocity) < 0.005) {
        touchState.inertiaRafId = 0
        touchState.fractionalLine = 0
        if (isScrolledToBottom()) {
          isUserScrolling.value = false
        }
        disableGpuHint()
        return
      }

      const pixelsPerFrame = touchState.velocity * 16
      const rawLines = -pixelsPerFrame / cellHeight.value
      const totalLines = rawLines + touchState.fractionalLine
      const linesPerFrame = Math.trunc(totalLines)

      if (linesPerFrame !== 0) {
        touchState.fractionalLine = totalLines - linesPerFrame
        // 直接更新滚动目标行，不立即调用 scrollToLine
        // syncViewportToLine 内部的 rAF 节流确保每帧最多执行一次 scrollToLine
        syncViewportToLine(currentLine.value + linesPerFrame)
      } else {
        touchState.fractionalLine = totalLines
      }

      touchState.inertiaRafId = requestAnimationFrame(step)
    }

    touchState.inertiaRafId = requestAnimationFrame(step)
  }

  // ==================== Selection Mode ====================

  /** 更新选区可视行范围（相对于 viewport 的行号） */
  function updateSelectionViewportRange() {
    if (!terminalRef.value) return
    const sel = terminalRef.value.getSelectionPosition()
    if (!sel) return
    const viewportY = terminalRef.value.buffer.active.viewportY
    selectionViewportRange.topRow = sel.start.y - viewportY
    selectionViewportRange.bottomRow = sel.end.y - viewportY
  }

  function enterSelectionMode() {
    isSelectionMode.value = true
    hasSelection.value = false
    selectLineAtTouchPos(longPressStartPos.x, longPressStartPos.y)
    // 记录长按触发位置，供弹窗定位
    longPressTriggerPos.x = longPressStartPos.x
    longPressTriggerPos.y = longPressStartPos.y
    startSelectionPoll()
  }

  function selectLineAtTouchPos(clientX: number, clientY: number) {
    if (!terminalRef.value?.element || cellHeight.value <= 0) return

    const viewport = terminalRef.value.element.querySelector('.xterm-viewport') as HTMLElement
    if (!viewport) return

    const rect = viewport.getBoundingClientRect()
    const relY = clientY - rect.top
    const relX = clientX - rect.left

    const visibleRow = Math.max(0, Math.min(Math.floor(relY / cellHeight.value), terminalRef.value.rows - 1))
    const cellWidth = terminalRef.value.cols > 0 ? rect.width / terminalRef.value.cols : 8
    const col = Math.max(0, Math.min(Math.floor(relX / cellWidth), terminalRef.value.cols - 1))

    const bufferLine = terminalRef.value.buffer.active.viewportY + visibleRow

    // 仅记录锚点，不立即选中：长按误触后直接抬起会自动退出选择模式
    // （hasSelection=false），恢复滚动；拖动手指时才形成选区
    selectionStartLine = bufferLine
    selectionStartCol = col
  }

  function extendSelectionToTouch(touch: Touch) {
    if (!terminalRef.value?.element || cellHeight.value <= 0) return

    const viewport = terminalRef.value.element.querySelector('.xterm-viewport') as HTMLElement
    if (!viewport) return

    const rect = viewport.getBoundingClientRect()
    const relY = touch.clientY - rect.top
    const relX = touch.clientX - rect.left

    const visibleRow = Math.max(0, Math.min(Math.floor(relY / cellHeight.value), terminalRef.value.rows - 1))
    const cellWidth = terminalRef.value.cols > 0 ? rect.width / terminalRef.value.cols : 8
    const endCol = Math.max(0, Math.min(Math.floor(relX / cellWidth), terminalRef.value.cols - 1))

    const bufferLine = terminalRef.value.buffer.active.viewportY + visibleRow

    const startLine = selectionStartLine
    const startCol = selectionStartCol

    if (startLine === bufferLine) {
      const left = Math.min(startCol, endCol)
      const right = Math.max(startCol, endCol)
      terminalRef.value.select(left, startLine, right - left + 1)
    } else if (bufferLine > startLine) {
      const startLineLength = terminalRef.value.buffer.active.getLine(startLine)?.length ?? 0
      const colSpan = startLineLength - startCol
      let totalSpan = colSpan
      for (let i = startLine + 1; i < bufferLine; i++) {
        totalSpan += terminalRef.value.buffer.active.getLine(i)?.length ?? 0
      }
      totalSpan += endCol + 1
      terminalRef.value.select(startCol, startLine, totalSpan)
    } else {
      const endLineLength = terminalRef.value.buffer.active.getLine(bufferLine)?.length ?? 0
      const colSpan = endLineLength - endCol
      let totalSpan = colSpan
      for (let i = bufferLine + 1; i < startLine; i++) {
        totalSpan += terminalRef.value.buffer.active.getLine(i)?.length ?? 0
      }
      totalSpan += startCol + 1
      terminalRef.value.select(endCol, bufferLine, totalSpan)
    }

    hasSelection.value = true
    updateSelectionViewportRange()
  }

  function exitSelectionMode() {
    isSelectionMode.value = false
    hasSelection.value = false
    selectionTouchEnded.value = false

    if (terminalRef.value) {
      terminalRef.value.clearSelection()
    }

    stopSelectionPoll()
  }

  function startSelectionPoll() {
    stopSelectionPoll()
    function poll() {
      if (!isSelectionMode.value) return
      hasSelection.value = terminalRef.value?.hasSelection() ?? false
      selectionPollRaf = requestAnimationFrame(poll)
    }
    selectionPollRaf = requestAnimationFrame(poll)
  }

  function stopSelectionPoll() {
    if (selectionPollRaf) {
      cancelAnimationFrame(selectionPollRaf)
      selectionPollRaf = 0
    }
  }

  async function copySelection() {
    const text = terminalRef.value?.getSelection()
    if (!text) return

    try {
      await writeClipboardText(text)
      toast.success(i18n.global.t('mobile.terminal.copied'))
    } catch {
      toast.error(i18n.global.t('mobile.terminal.copyFailed'))
    }
    exitSelectionMode()
  }

  function selectAllText() {
    if (!terminalRef.value) return
    terminalRef.value.selectAll()
    hasSelection.value = true
    updateSelectionViewportRange()
  }

  // ==================== Viewport Scroll Setup ====================

  function setupViewportScroll() {
    if (!terminalRef.value?.element) return

    const viewport = terminalRef.value.element.querySelector('.xterm-viewport') as HTMLElement
    if (viewport) {
      viewport.style.overflowY = 'hidden'
      viewport.style.touchAction = 'none'
      viewport.style.pointerEvents = 'none'
    }

    // 禁用 xterm-scrollable-element 的触摸和指针事件
    // xterm 新版本使用 SmoothScrollableElement 管理 viewport 滚动
    // 移动端由自定义触摸滚动接管，必须禁用 xterm 内部的触摸交互
    const scrollableElement = terminalRef.value.element.querySelector('.xterm-scrollable-element') as HTMLElement
    if (scrollableElement) {
      scrollableElement.style.touchAction = 'none'
      scrollableElement.style.pointerEvents = 'none'
    }

    // 布局未就绪时 clientHeight 可能为 0，不覆盖旧值避免滚动永久失效
    const h = computeCellHeight()
    if (h > 0) cellHeight.value = h

    if (scrollContainerRef.value) {
      scrollContainerRef.value.addEventListener('touchstart', onTouchStart, { passive: true, capture: true })
      scrollContainerRef.value.addEventListener('touchmove', onTouchMove, { passive: true, capture: true })
      scrollContainerRef.value.addEventListener('touchend', onTouchEnd, { capture: true })
    }

    // 输出自动跟随：内部已做触摸接管检查 + rAF 节流，
    // 渲染期间滚动由 xterm 渲染服务统一提交，不与输出渲染竞争
    terminalRef.value.onLineFeed(() => {
      scrollToBottom()
    })

    terminalRef.value.onScroll((viewportY: number) => {
      currentLine.value = viewportY
      scheduleScrollRefresh()
    })

    terminalRef.value.onResize(() => {
      // clientHeight 为 0 的中间态不覆盖旧值，避免滚动永久失效
      const h = computeCellHeight()
      if (h > 0) cellHeight.value = h
    })

    nextTick(() => scrollToBottom())
  }

  // ==================== Public Methods ====================

  function fitTerminal(fitAddon: FitAddon | null) {
    if (!fitAddon || !terminalRef.value) return
    try {
      fitAddon.fit()
    } catch (e) {
      console.warn('[useTerminalScroll] fit failed:', e)
    }
  }

  function handleShortcutsPanelToggle(height: number) {
    if (height > 0) {
      if (isScrolledToBottom()) {
        shortcutsPanelHeight.value = height
      }
    } else {
      shortcutsPanelHeight.value = 0
    }
  }

  function applySettings(theme: string, fontSize: number, fitAddon: FitAddon | null) {
    if (!terminalRef.value) return

    // 单独设置每个属性，避免覆盖整个 options 对象
    terminalRef.value.options.fontSize = fontSize

    // 重新 fit 终端
    setTimeout(() => fitTerminal(fitAddon), 50)
  }

  // ==================== Dispose ====================

  function dispose() {
    isUserScrolling.value = false
    scrollbarVisible.value = false

    if (xtermTransitionTimer) {
      clearTimeout(xtermTransitionTimer)
      xtermTransitionTimer = null
    }

    if (touchState.hideTimer) {
      clearTimeout(touchState.hideTimer)
      touchState.hideTimer = null
    }
    if (touchState.inertiaRafId) {
      cancelAnimationFrame(touchState.inertiaRafId)
      touchState.inertiaRafId = 0
    }
    if (pendingScrollRaf) {
      cancelAnimationFrame(pendingScrollRaf)
      pendingScrollRaf = 0
    }
    if (pendingScrollRefreshRaf) {
      cancelAnimationFrame(pendingScrollRefreshRaf)
      pendingScrollRefreshRaf = 0
    }
    if (renderSyncRaf) {
      cancelAnimationFrame(renderSyncRaf)
      renderSyncRaf = 0
    }
    pendingScrollLine = -1

    if (scrollContainerRef.value) {
      scrollContainerRef.value.removeEventListener('touchstart', onTouchStart, { passive: true, capture: true } as EventListenerOptions)
      scrollContainerRef.value.removeEventListener('touchmove', onTouchMove, { passive: true, capture: true } as EventListenerOptions)
      scrollContainerRef.value.removeEventListener('touchend', onTouchEnd, { capture: true })
    }

    currentLine.value = 0
    cellHeight.value = 0

    if (longPressTimer.value) {
      clearTimeout(longPressTimer.value)
      longPressTimer.value = null
    }
    isSelectionMode.value = false
    hasSelection.value = false
    selectionTouchEnded.value = false
    stopSelectionPoll()
  }

  return {
    // State
    currentLine,
    isSelectionMode,
    hasSelection,
    selectionTouchEnded,
    scrollbarVisible,
    scrollbarThumbStyle,
    xtermContainerStyle,
    shortcutsPanelHeight,
    isUserScrolling,
    cellHeight,
    longPressTriggerPos,
    selectionViewportRange,

    // Methods
    scrollToBottom,
    scrollToBottomManual,
    fitTerminal,
    setupViewportScroll,
    exitSelectionMode,
    copySelection,
    selectAllText,
    handleShortcutsPanelToggle,
    applySettings,

    // Lifecycle
    dispose,
  }
}
