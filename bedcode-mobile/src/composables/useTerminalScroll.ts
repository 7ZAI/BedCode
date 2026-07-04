/**
 * useTerminalScroll - 终端触摸滚动 + 选择模式 composable
 *
 * 封装 xterm 终端的触摸滚动（含惯性）、自定义滚动条、长按选择模式等逻辑。
 * 不拥有 Terminal/FitAddon 实例，通过参数接收 ref。
 */

import { ref, reactive, computed, nextTick, type Ref } from 'vue'
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

  const xtermContainerStyle = computed(() => {
    if (shortcutsPanelHeight.value <= 0) return {}
    return {
      transform: `translateY(-${shortcutsPanelHeight.value}px)`,
      transition: 'transform 0.25s cubic-bezier(0.4, 0, 0.2, 1)',
    }
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

  function scrollToBottom() {
    if (!terminalRef.value) return

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
    isUserScrolling.value = false
  }

  function syncViewportToLine(line: number) {
    if (!terminalRef.value) return

    const bufferLength = terminalRef.value.buffer.active.length
    const rows = terminalRef.value.rows
    const maxLine = Math.max(0, bufferLength - rows)

    const clampedLine = Math.max(0, Math.min(line, maxLine))
    currentLine.value = clampedLine
    pendingScrollLine = clampedLine

    if (!pendingScrollRaf) {
      pendingScrollRaf = requestAnimationFrame(() => {
        pendingScrollRaf = 0
        if (terminalRef.value && pendingScrollLine >= 0) {
          terminalRef.value.scrollToLine(pendingScrollLine)
          pendingScrollLine = -1
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
    const lineData = terminalRef.value.buffer.active.getLine(bufferLine)
    const lineLength = lineData?.length ?? 0

    selectionStartLine = bufferLine
    selectionStartCol = col

    const endCol = lineLength > 0 ? lineLength - 1 : 0
    terminalRef.value.select(0, bufferLine, endCol + 1)
    hasSelection.value = true
    updateSelectionViewportRange()
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

    cellHeight.value = computeCellHeight()

    if (scrollContainerRef.value) {
      scrollContainerRef.value.addEventListener('touchstart', onTouchStart, { passive: true, capture: true })
      scrollContainerRef.value.addEventListener('touchmove', onTouchMove, { passive: true, capture: true })
      scrollContainerRef.value.addEventListener('touchend', onTouchEnd, { capture: true })
    }

    terminalRef.value.onLineFeed(() => {
      if (!isUserScrolling.value) {
        nextTick(() => scrollToBottom())
      }
    })

    terminalRef.value.onScroll((viewportY: number) => {
      currentLine.value = viewportY
    })

    terminalRef.value.onResize(() => {
      cellHeight.value = computeCellHeight()
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
