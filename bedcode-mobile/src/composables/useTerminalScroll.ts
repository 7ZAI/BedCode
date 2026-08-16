/**
 * useTerminalScroll - 终端触摸滚动 + 选择模式 composable
 *
 * 封装 xterm 终端的触摸滚动（含惯性）、自定义滚动条、长按选择模式等逻辑。
 * 不拥有 Terminal/FitAddon 实例，通过参数接收 ref。
 *
 * 滚动状态对齐桌面端 TerminalPreview："是否在底部"由 onScroll 按位置推导
 * （位置即状态，无容差猜测）；触摸按下期间以 touchActive 锁定输出自动跟随
 * （VS Code 终端滚动锁行为：手指按住时不被新输出拉回底部）。
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

/**
 * 触摸滚动行为注入（TUI 兼容）：TUI 模式下手势提交改为发送 SGR 滚轮事件
 */
export interface TuiScrollCompat {
  /** TUI 模式门控（useTuiCompat.isTuiMode） */
  isTuiMode: Ref<boolean>
  /** 提交滚轮事件：deltaLines > 0 向下查看（手指上滑），col/row 为 1-based 终端格坐标 */
  sendWheel(deltaLines: number, col: number, row: number): void
}

export function useTerminalScroll(
  terminalRef: Ref<Terminal | null>,
  scrollContainerRef: Ref<HTMLDivElement | null>,
  tuiCompat?: TuiScrollCompat,
) {
  const toast = useToast()

  // ==================== Scroll State ====================

  const currentLine = ref(0)
  const cellHeight = ref(0)
  const isUserScrolling = ref(false)
  /** 触摸滚动锁：手指按下期间暂停输出自动跟随，抬起后由位置推导恢复 */
  const touchActive = ref(false)
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
    lastX: 0,
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
  // 渲染帧同步：确保 scrollToLine 只在 xterm 渲染完成后执行
  // WebGL 渲染器双缓冲在渲染未完成时切换 viewport 会导致新旧帧同时可见
  let renderSyncRaf = 0
  // 输出自动跟随标志：scrollToBottom 的程序性追赶滚动（xterm 同步 fire
  // onScroll）在 onScroll 推导中跳过——回放流式写入时滚动常追不上最新行，
  // 若按位置推导会把「程序性滚动未追到位」误判为「用户上滚」，导致
  // isUserScrolling 被置 true 后自动跟随永久锁死（进入终端停在历史中间）
  let autoFollowScroll = false

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

  /** 是否已滚动到缓冲区底部（对齐桌面端推导公式） */
  function isAtBottom(): boolean {
    if (!terminalRef.value) return true
    const buffer = terminalRef.value.buffer.active
    return buffer.viewportY + terminalRef.value.rows >= buffer.length - 1
  }

  /**
   * 自动跟随输出滚动到底部（VSCode 式）
   *
   * - 触摸滚动期间忽略：用户已接管滚动，不被新输出拉回底部
   * - rAF 节流：同帧多次调用只滚一次，输出持续增长时更新目标行
   */
  function scrollToBottom() {
    if (!terminalRef.value || isUserScrolling.value || touchActive.value) return

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
        autoFollowScroll = true
        terminalRef.value.scrollToLine(target)
      }
    })
  }

  /** 用户点击"回到底部"：强制滚到底并恢复自动跟随（不受触摸状态影响） */
  function scrollToBottomManual() {
    if (!terminalRef.value) return

    // 打断进行中的惯性滑行（否则本次滚动会被平滑动画接管）
    cancelGlide()

    // 显式复位底部状态与滚动锁：已在底部时 scrollToLine 不触发 onScroll，
    // 推导路径不会执行，需手动复位
    isUserScrolling.value = false
    touchActive.value = false
    const bufferLength = terminalRef.value.buffer.active.length
    const rows = terminalRef.value.rows
    const targetLine = Math.max(0, bufferLength - rows)

    if (pendingScrollRaf) {
      cancelAnimationFrame(pendingScrollRaf)
      pendingScrollRaf = 0
    }
    pendingScrollLine = -1
    currentLine.value = targetLine
    autoFollowScroll = false
    terminalRef.value.scrollToLine(targetLine)
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

  /**
   * 触摸位置 → 终端格坐标（1-based，clamp 到 cols/rows）。
   * 供 TUI 模式滚轮序列使用；非 TUI 模式不受影响
   */
  function touchToCell(clientX: number, clientY: number): { col: number; row: number } {
    const term = terminalRef.value
    if (!term?.element || cellHeight.value <= 0) return { col: 1, row: 1 }
    const viewport = term.element.querySelector('.xterm-viewport') as HTMLElement
    if (!viewport) return { col: 1, row: 1 }
    const rect = viewport.getBoundingClientRect()
    const cellWidth = term.cols > 0 ? rect.width / term.cols : 8
    const col = Math.max(1, Math.min(Math.floor((clientX - rect.left) / cellWidth) + 1, term.cols))
    const row = Math.max(1, Math.min(Math.floor((clientY - rect.top) / cellHeight.value) + 1, term.rows))
    return { col, row }
  }

  /** 手势提交：TUI 模式转滚轮事件，否则滚动 xterm 缓冲区 */
  function commitGesture(deltaLines: number, clientX: number, clientY: number) {
    if (tuiCompat?.isTuiMode.value) {
      const cell = touchToCell(clientX, clientY)
      tuiCompat.sendWheel(deltaLines, cell.col, cell.row)
      return
    }
    isUserScrolling.value = true
    syncViewportToLine(currentLine.value + deltaLines)
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
    // 手指按下立即打断进行中的惯性滑行（恢复即时滚动）
    cancelGlide()

    const touch = e.touches[0]
    touchState.startY = touch.clientY
    touchState.startLine = currentLine.value
    touchState.lastY = touch.clientY
    touchState.lastX = touch.clientX
    touchState.lastTime = Date.now()
    touchState.velocity = 0
    touchState.fractionalLine = 0

    // 触摸即锁定滚动：暂停输出自动跟随，避免手势被新输出拉回底部；
    // 底部判定仍由 onScroll 按位置推导，手指抬起后自动恢复
    touchActive.value = true
    // 用户接管滚动：清除可能残留的自动跟随标志（scrollToLine 未触发
    // onScroll 的边界场景），保证后续推导从干净状态开始
    autoFollowScroll = false

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
    touchState.lastX = touch.clientX
    touchState.lastTime = Date.now()

    const rawLines = -deltaY / cellHeight.value
    const totalLines = rawLines + touchState.fractionalLine
    const linesDelta = Math.trunc(totalLines)

    if (linesDelta === 0) {
      touchState.fractionalLine = totalLines
      return
    }

    touchState.fractionalLine = totalLines - linesDelta
    // TUI 模式下不设置 isUserScrolling（滚动条隐藏、底部按钮不显示）
    commitGesture(linesDelta, touch.clientX, touch.clientY)
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
      touchActive.value = false
      disableGpuHint()
      return
    }

    // 手指抬起解除滚动锁：是否停在底部由 onScroll 按位置推导
    touchActive.value = false

    // 先应用最后一次拖动的挂起滚动位置（syncViewportToLine 的 rAF 可能尚未
    // 触发），并取消挂起帧——否则惯性开始后旧帧会把滑行目标拉回拖动终点
    if (pendingScrollRaf) {
      cancelAnimationFrame(pendingScrollRaf)
      pendingScrollRaf = 0
    }
    if (pendingScrollLine >= 0) {
      const lastLine = pendingScrollLine
      pendingScrollLine = -1
      terminalRef.value.scrollToLine(lastLine)
      currentLine.value = lastLine
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

  // 惯性滑行参数（非 TUI 模式）：甩动后单次 xterm 平滑滚动（smoothScrollDuration）
  // 代替逐帧步进，消除低速度尾段的「走走停停」顿感
  const INERTIA_GLIDE_MIN_MS = 160
  const INERTIA_GLIDE_MAX_MS = 320
  const INERTIA_GLIDE_MS_PER_LINE = 8
  /** 等效摩擦投影：总行程 = v·16ms/(1−0.95)，与旧逐帧衰减的总距离一致 */
  const INERTIA_PROJECT_FACTOR = 16 / (1 - 0.95)

  let glideTimer: ReturnType<typeof setTimeout> | null = null

  /** 终止进行中的惯性滑行：恢复即时滚动（xterm 动画由 duration 置 0 后的
   * scrollToLine 走 setScrollPositionNow 取消） */
  function cancelGlide() {
    if (glideTimer) {
      clearTimeout(glideTimer)
      glideTimer = null
    }
    const term = terminalRef.value
    if (!term || !('options' in term)) return
    if ((term.options.smoothScrollDuration ?? 0) > 0) {
      term.options.smoothScrollDuration = 0
      term.scrollToLine(term.buffer.active.viewportY)
    }
  }

  function startInertia() {
    const term = terminalRef.value
    if (!term || Math.abs(touchState.velocity) < 0.02) {
      // 是否停在底部由 onScroll 按位置推导，无需手动判定
      disableGpuHint()
      return
    }

    // TUI 模式：保持逐帧滚轮事件惯性（应用侧每事件渲染一行，天然平滑）
    if (tuiCompat?.isTuiMode.value) {
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
          // 惯性结束：是否停在底部由 onScroll 按位置推导
          disableGpuHint()
          return
        }

        const pixelsPerFrame = touchState.velocity * 16
        const rawLines = -pixelsPerFrame / cellHeight.value
        const totalLines = rawLines + touchState.fractionalLine
        const linesPerFrame = Math.trunc(totalLines)

        if (linesPerFrame !== 0) {
          touchState.fractionalLine = totalLines - linesPerFrame
          // TUI 模式下惯性转为滚轮事件（坐标用最后触摸位置），
          // 否则滚动 xterm 缓冲区（syncViewportToLine 内部 rAF 节流每帧至多一次）
          commitGesture(linesPerFrame, touchState.lastX, touchState.lastY)
        } else {
          touchState.fractionalLine = totalLines
        }

        touchState.inertiaRafId = requestAnimationFrame(step)
      }

      touchState.inertiaRafId = requestAnimationFrame(step)
      return
    }

    // 非 TUI：投影惯性行程 → 单次平滑滑行到目标行（不再逐帧步进）
    const projectedPx = touchState.velocity * INERTIA_PROJECT_FACTOR
    const lines = -projectedPx / cellHeight.value
    if (Math.abs(lines) < 0.5) {
      disableGpuHint()
      return
    }

    const bufferLength = term.buffer.active.length
    const maxLine = Math.max(0, bufferLength - term.rows)
    const targetLine = Math.max(0, Math.min(Math.round(currentLine.value + lines), maxLine))
    if (targetLine === currentLine.value) {
      disableGpuHint()
      return
    }

    // 滑行期间暂停输出自动跟随；结束位置由 onScroll 按位置推导恢复
    isUserScrolling.value = true
    touchState.fractionalLine = 0
    // 取消挂起的拖动滚动帧，避免滑行开始后被旧帧拉回拖动终点
    if (pendingScrollRaf) {
      cancelAnimationFrame(pendingScrollRaf)
      pendingScrollRaf = 0
    }
    pendingScrollLine = -1

    // 行程越远滑行越久（8ms/行），限制在 160~320ms 内
    const duration = Math.max(INERTIA_GLIDE_MIN_MS, Math.min(
      INERTIA_GLIDE_MAX_MS,
      Math.round(Math.abs(lines) * INERTIA_GLIDE_MS_PER_LINE),
    ))
    term.options.smoothScrollDuration = duration
    term.scrollToLine(targetLine)
    showScrollbar()

    // 滑行结束后恢复即时滚动（触摸接管/输出跟随不受平滑动画干扰）
    if (glideTimer) clearTimeout(glideTimer)
    glideTimer = setTimeout(() => {
      glideTimer = null
      if (!terminalRef.value) return
      terminalRef.value.options.smoothScrollDuration = 0
    }, duration + 60)
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
      // 输出自动跟随的追赶滚动：本次滚动由 scrollToBottom 发起（非用户
      // 主动），跳过位置推导，避免回放/输出追赶被误判为离开底部
      if (autoFollowScroll) {
        autoFollowScroll = false
        return
      }
      // 对齐桌面端：由滚动位置推导是否处于底部（位置即状态），
      // 在底部时输出自动跟随，向上滚动后停止跟随
      const buffer = terminalRef.value!.buffer.active
      const viewportBottom = buffer.viewportY + terminalRef.value!.rows
      isUserScrolling.value = viewportBottom < buffer.length - 1
      // 不做额外全量重绘：xterm scrollLines 已自带 refresh(0, rows-1)，
      // 再加一次 DOM 渲染器下的全量重绘是纯开销（每帧双倍 canvas 绘制，卡顿源）
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
      // 原始尺寸 fit：采用 FitAddon 计算的尺寸，宽度/高度不做增减
      // （与 TerminalView.fitWithMargin 保持一致）
      fitAddon.fit()
    } catch (e) {
      console.warn('[useTerminalScroll] fit failed:', e)
    }
  }

  function handleShortcutsPanelToggle(height: number) {
    if (height > 0) {
      // 仅当终端停在底部时上移内容，露出被面板遮挡的当前行
      if (isAtBottom()) {
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
    touchActive.value = false
    scrollbarVisible.value = false

    // 终止惯性滑行并复位即时滚动
    cancelGlide()

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
    if (renderSyncRaf) {
      cancelAnimationFrame(renderSyncRaf)
      renderSyncRaf = 0
    }
    pendingScrollLine = -1
    autoFollowScroll = false

    if (scrollContainerRef.value) {
      scrollContainerRef.value.removeEventListener('touchstart', onTouchStart, { passive: true, capture: true } as EventListenerOptions)
      scrollContainerRef.value.removeEventListener('touchmove', onTouchMove, { passive: true, capture: true } as EventListenerOptions)
      scrollContainerRef.value.removeEventListener('touchend', onTouchEnd, { capture: true })
    }

    currentLine.value = 0
    cellHeight.value = 0
    touchState.lastX = 0
    touchState.lastY = 0

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
