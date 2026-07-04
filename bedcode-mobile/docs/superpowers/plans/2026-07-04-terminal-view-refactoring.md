# TerminalView Refactoring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce TerminalView.vue from 2248 lines to ~550 lines by extracting header, settings modal, confirm modal, scroll composable, and CSS into separate files.

**Architecture:** Extract 3 child components (TerminalHeader, TerminalSettingsModal, TerminalConfirmModal), 1 composable (useTerminalScroll), and 1 CSS file (styles/terminal.css). TerminalView becomes an orchestrator that owns the terminal instance and wires components together.

**Tech Stack:** Vue 3 + TypeScript + TailwindCSS + @xterm/xterm

---

## File Structure

| Action | File | Responsibility |
|--------|------|---------------|
| Create | `src/components/TerminalHeader.vue` | Header bar with toolbar buttons and overflow menu |
| Create | `src/components/TerminalSettingsModal.vue` | Terminal settings modal (font, theme, quick bar, toolbar) |
| Create | `src/components/TerminalConfirmModal.vue` | Generic confirm dialog |
| Create | `src/composables/useTerminalScroll.ts` | Touch scroll, inertia, scrollbar, selection mode |
| Create | `src/styles/terminal.css` | Terminal/xterm/scrollbar/selection styles |
| Modify | `src/views/TerminalView.vue` | Reduce to orchestrator (~550 lines) |

---

### Task 1: Create styles/terminal.css

**Files:**
- Create: `src/styles/terminal.css`

Extract all terminal-related CSS from TerminalView.vue's `<style scoped>` into an unscoped CSS file. Remove `:deep()` wrappers since the file is unscoped.

- [ ] **Step 1: Create the terminal.css file**

```css
/**
 * Terminal Styles - 终端视图布局、xterm 覆盖、滚动条、选择模式样式
 *
 * 从 TerminalView.vue 提取，供终端视图及相关组件共用
 */

/* Terminal View Layout */
.terminal-view {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--mobile-terminal-bg);
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 1;
  overflow: hidden;
  /* padding 由 JS 动态设置（安全区域 + 键盘高度），添加过渡保证平滑避让 */
  transition: padding 0.25s cubic-bezier(0.4, 0, 0.2, 1);
}

/* Loading Overlay */
.loading-overlay {
  position: fixed;
  inset: 0;
  z-index: 50;
  background: var(--mobile-terminal-bg);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 1rem;
}

.loading-spinner {
  width: 32px;
  height: 32px;
  border: 3px solid var(--mobile-border);
  border-top-color: var(--mobile-accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.loading-text {
  font-size: 0.875rem;
  color: var(--mobile-text-muted);
  margin: 0;
}

/* Loading fade transition */
.loading-fade-enter-active,
.loading-fade-leave-active {
  transition: opacity 0.3s ease;
}

.loading-fade-enter-from,
.loading-fade-leave-to {
  opacity: 0;
}

/* Main Content Area */
.main-content {
  flex: 1;
  min-height: 0;
  position: relative;
  overflow: hidden;
}

/* Sidebar overlay */
.sidebar-overlay {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  z-index: 20;
  box-shadow: -4px 0 16px rgba(0, 0, 0, 0.3);
  transition: transform 0.25s cubic-bezier(0.4, 0, 0.2, 1);
}

.sidebar-hidden {
  transform: translateX(100%);
  pointer-events: none;
}

.sidebar-backdrop {
  position: absolute;
  inset: 0;
  z-index: 15;
}

/* Terminal Area */
.terminal-output-area {
  position: absolute;
  inset: 0;
  overflow: hidden;
  background: var(--mobile-terminal-bg);
}

/* 触摸滚动容器 */
.terminal-scroll-container {
  position: relative;
  width: 100%;
  height: 100%;
  overflow: hidden;
  touch-action: none;
}

/* 自定义滚动条轨道 */
.scrollbar-track {
  position: absolute;
  top: 4px;
  right: 2px;
  bottom: 4px;
  width: 4px;
  z-index: 5;
  pointer-events: none;
}

/* 自定义滚动条滑块 */
.scrollbar-thumb {
  position: absolute;
  left: 0;
  right: 0;
  min-height: 20px;
  border-radius: 2px;
  background: rgba(160, 160, 180, 0.3);
  opacity: 0;
  transition: opacity 0.25s ease, background 0.15s ease;
  pointer-events: none;
}

.scrollbar-thumb.visible {
  opacity: 1;
  background: rgba(0, 212, 255, 0.4);
}

/* xterm 容器 */
.xterm-container {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  overflow: hidden;
}

/* xterm 核心样式 */
.xterm {
  touch-action: none;
  user-select: none;
  -webkit-user-select: none;
  scrollbar-width: none;
}

.xterm::-webkit-scrollbar {
  display: none;
  width: 0;
}

.xterm-screen {
  touch-action: none;
  user-select: none;
  -webkit-user-select: none;
  scrollbar-width: none;
}

.xterm-screen::-webkit-scrollbar {
  display: none;
  width: 0;
}

/* 选择模式：允许选中 */
.selection-mode .xterm {
  user-select: text;
  -webkit-user-select: text;
}

.selection-mode .xterm-screen {
  user-select: text;
  -webkit-user-select: text;
}

/* 选择模式：高亮 xterm 区域 */
.selection-mode .xterm-container {
  outline: 2px solid rgba(0, 212, 255, 0.3);
  outline-offset: -2px;
  border-radius: 2px;
}

/* 选择模式操作栏 */
.selection-action-bar {
  position: absolute;
  bottom: 12px;
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  gap: 0.5rem;
  padding: 0.5rem 0.75rem;
  background: var(--mobile-bg-secondary);
  border: 1px solid var(--mobile-border);
  border-radius: 0.75rem;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
  z-index: 10;
}

.selection-action-btn {
  padding: 0.375rem 0.875rem;
  border-radius: 0.375rem;
  background: var(--mobile-accent);
  border: none;
  color: var(--mobile-text-on-accent);
  font-size: 0.8125rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
  white-space: nowrap;
}

.selection-action-btn.cancel {
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-secondary);
}

.selection-action-btn:active {
  opacity: 0.8;
}

/* 选择栏过渡动画 */
.selection-bar-enter-active,
.selection-bar-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}

.selection-bar-enter-from,
.selection-bar-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(8px);
}

/* 禁用 xterm-viewport 原生滚动 */
.xterm-viewport {
  overflow-y: hidden !important;
  touch-action: none !important;
  pointer-events: none !important;
  scrollbar-width: none !important;
}

.xterm-viewport::-webkit-scrollbar {
  display: none !important;
  width: 0 !important;
}

/* 禁用 xterm-scroll-area 滚动条 */
.xterm-scroll-area {
  scrollbar-width: none;
}

.xterm-scroll-area::-webkit-scrollbar {
  display: none;
  width: 0;
}
```

- [ ] **Step 2: Commit**

```bash
git add src/styles/terminal.css
git commit -m "refactor(mobile): extract terminal styles to dedicated CSS file"
```

---

### Task 2: Create useTerminalScroll composable

**Files:**
- Create: `src/composables/useTerminalScroll.ts`

Extract all touch scroll, inertia, scrollbar, and selection mode logic from TerminalView.vue into a composable. The composable receives `terminalRef` and `scrollContainerRef` as parameters — it does not own them.

- [ ] **Step 1: Create the composable file**

```typescript
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
      const touch = e.touches[0]
      longPressStartPos.x = touch.clientX
      longPressStartPos.y = touch.clientY

      if (terminalRef.value?.element && cellHeight.value > 0) {
        const screen = terminalRef.value.element.querySelector('.xterm-screen') as HTMLElement
        if (screen) {
          const rect = screen.getBoundingClientRect()
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

  function enableGpuHint() {
    if (!terminalRef.value?.element) return
    const screen = terminalRef.value.element.querySelector('.xterm-screen') as HTMLElement
    if (screen) {
      screen.style.willChange = 'transform'
    }
  }

  function disableGpuHint() {
    if (!terminalRef.value?.element) return
    const screen = terminalRef.value.element.querySelector('.xterm-screen') as HTMLElement
    if (screen) {
      screen.style.willChange = 'auto'
    }
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

  function enterSelectionMode() {
    isSelectionMode.value = true
    hasSelection.value = false
    selectLineAtTouchPos(longPressStartPos.x, longPressStartPos.y)
    startSelectionPoll()
  }

  function selectLineAtTouchPos(clientX: number, clientY: number) {
    if (!terminalRef.value?.element || cellHeight.value <= 0) return

    const screen = terminalRef.value.element.querySelector('.xterm-screen') as HTMLElement
    if (!screen) return

    const rect = screen.getBoundingClientRect()
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
  }

  function extendSelectionToTouch(touch: Touch) {
    if (!terminalRef.value?.element || cellHeight.value <= 0) return

    const screen = terminalRef.value.element.querySelector('.xterm-screen') as HTMLElement
    if (!screen) return

    const rect = screen.getBoundingClientRect()
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
  }

  function exitSelectionMode() {
    isSelectionMode.value = false
    hasSelection.value = false

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
    stopSelectionPoll()
  }

  return {
    // State
    currentLine,
    isSelectionMode,
    hasSelection,
    scrollbarVisible,
    scrollbarThumbStyle,
    xtermContainerStyle,
    shortcutsPanelHeight,
    isUserScrolling,
    cellHeight,

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
```

- [ ] **Step 2: Commit**

```bash
git add src/composables/useTerminalScroll.ts
git commit -m "refactor(mobile): extract terminal scroll and selection logic to composable"
```

---

### Task 3: Create TerminalConfirmModal component

**Files:**
- Create: `src/components/TerminalConfirmModal.vue`

Generic confirm dialog extracted from the clear-screen confirm modal in TerminalView.vue.

- [ ] **Step 1: Create the component**

```vue
<template>
  <div v-if="visible" class="confirm-modal-overlay mobile-ui" @click.self="$emit('cancel')">
    <div class="confirm-modal" :style="safeAreaStyle">
      <p class="confirm-text">{{ message }}</p>
      <div class="confirm-buttons">
        <button class="confirm-btn cancel" @click.stop="$emit('cancel')">{{ t('common.button.cancel') }}</button>
        <button class="confirm-btn confirm" @click.stop="$emit('confirm')">{{ t('common.button.confirm') }}</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 终端确认弹窗 - 通用确认对话框
 */
defineOptions({ name: 'TerminalConfirmModal' })

import { useI18n } from 'vue-i18n'

const { t } = useI18n()

defineProps<{
  visible: boolean
  message: string
  safeAreaStyle: Record<string, string>
}>()

defineEmits<{
  confirm: []
  cancel: []
}>()
</script>

<style scoped>
.confirm-modal-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: var(--mobile-overlay-heavy);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 50;
  padding: 1rem;
}

.confirm-modal {
  background: var(--mobile-bg-secondary);
  border-radius: 1rem;
  padding: 1.5rem;
  width: 100%;
  max-width: 300px;
  text-align: center;
}

.confirm-text {
  font-size: 1rem;
  color: var(--mobile-text-primary);
  margin: 0 0 1.25rem;
}

.confirm-buttons {
  display: flex;
  gap: 0.75rem;
}

.confirm-btn {
  flex: 1;
  padding: 0.75rem;
  border-radius: 0.5rem;
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.confirm-btn.cancel {
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-muted);
}

.confirm-btn.cancel:hover {
  background: var(--mobile-bg-hover);
  color: var(--mobile-text-primary);
}

.confirm-btn.confirm {
  background: #ef4444;
  border: none;
  color: #ffffff;
}

.confirm-btn.confirm:hover {
  background: #dc2626;
}
</style>
```

- [ ] **Step 2: Commit**

```bash
git add src/components/TerminalConfirmModal.vue
git commit -m "refactor(mobile): extract confirm modal to TerminalConfirmModal component"
```

---

### Task 4: Create TerminalSettingsModal component

**Files:**
- Create: `src/components/TerminalSettingsModal.vue`

Extract the settings modal (font size, theme, quick bar, toolbar config) from TerminalView.vue. All `temp*` state is managed locally; only emitted on confirm.

- [ ] **Step 1: Create the component**

```vue
<template>
  <div v-if="visible" class="settings-modal-overlay mobile-ui" @click.self="$emit('cancel')">
    <div class="settings-modal" :style="safeAreaStyle">
      <div class="settings-header">
        <h2>{{ t('mobile.terminal.terminalSettings') }}</h2>
        <button class="close-btn" @click.stop="$emit('cancel')">
          <svg width="24" height="24" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <div class="settings-content">
        <!-- Font Size -->
        <div class="settings-section">
          <label class="settings-label">{{ t('mobile.terminal.fontSize') }}</label>
          <div class="font-size-control">
            <button class="size-btn" @click.stop="tempFontSize--" :disabled="tempFontSize <= 10">-</button>
            <span class="size-value">{{ tempFontSize }}px</span>
            <button class="size-btn" @click.stop="tempFontSize++" :disabled="tempFontSize >= 24">+</button>
          </div>
        </div>

        <!-- Theme -->
        <div class="settings-section">
          <label class="settings-label">{{ t('mobile.terminal.theme') }}</label>
          <div class="theme-grid">
            <button
              v-for="(theme, name) in TERMINAL_THEMES"
              :key="name"
              class="theme-btn"
              :class="{ active: tempTheme === name }"
              @click.stop="tempTheme = name"
            >
              <span class="theme-preview" :style="getThemePreviewStyle(name)">Aa</span>
              <span class="theme-name">{{ resolveThemeLabel(theme.label, t) }}</span>
            </button>
          </div>
        </div>

        <!-- Quick Bar Count -->
        <div class="settings-section">
          <label class="settings-label">{{ t('mobile.terminal.shortcutCount') }}</label>
          <div class="font-size-control">
            <button class="size-btn" @click.stop="tempQuickBarCount--" :disabled="tempQuickBarCount <= 3">-</button>
            <span class="size-value">{{ tempQuickBarCount }}</span>
            <button class="size-btn" @click.stop="tempQuickBarCount++" :disabled="tempQuickBarCount >= 10">+</button>
          </div>
        </div>

        <!-- Header Toolbar Items -->
        <div class="settings-section">
          <label class="settings-label">{{ t('mobile.terminal.persistentToolbar') }}</label>
          <p class="settings-hint">{{ t('mobile.terminal.persistentToolbar') }}</p>
          <div class="toolbar-toggle-grid">
            <button
              v-for="item in allToolbarItems"
              :key="item.key"
              class="toolbar-toggle-btn"
              :class="{ active: tempToolbarItems.includes(item.key) }"
              @click.stop="toggleToolbarItem(item.key)"
            >
              <span>{{ item.label }}</span>
            </button>
          </div>
        </div>
      </div>

      <!-- Settings Footer -->
      <div class="settings-footer">
        <button class="settings-footer-btn cancel" @click.stop="$emit('cancel')">{{ t('common.button.cancel') }}</button>
        <button class="settings-footer-btn confirm" @click.stop="handleConfirm">{{ t('common.button.confirm') }}</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 终端设置弹窗 - 字体大小、主题、快捷栏数量、工具栏配置
 *
 * 所有编辑中的状态 (temp*) 在组件内部管理，确认时通过 emit 传出
 */
defineOptions({ name: 'TerminalSettingsModal' })

import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useTheme } from '@/composables/useTheme'
import { TERMINAL_THEMES, resolveThemeLabel } from '@/config/terminalThemes'

export interface TerminalSettings {
  fontSize: number
  theme: string
  isThemeUserSet: boolean
  quickBarCount: number
  toolbarItems: string[]
}

export interface ToolbarItemConfig {
  key: string
  label: string
  icon: string
}

const props = defineProps<{
  visible: boolean
  fontSize: number
  theme: string
  isThemeUserSet: boolean
  quickBarCount: number
  toolbarItems: string[]
  allToolbarItems: ToolbarItemConfig[]
  safeAreaStyle: Record<string, string>
}>()

const emit = defineEmits<{
  confirm: [settings: TerminalSettings]
  cancel: []
}>()

const { t } = useI18n()
const { isSystemDark } = useTheme()

const tempFontSize = ref(props.fontSize)
const tempTheme = ref<string>(props.isThemeUserSet ? props.theme : 'system')
const tempQuickBarCount = ref(props.quickBarCount)
const tempToolbarItems = ref<string[]>([...props.toolbarItems])

// 打开时同步 props 到临时状态
watch(() => props.visible, (visible) => {
  if (visible) {
    tempFontSize.value = props.fontSize
    tempTheme.value = props.isThemeUserSet ? props.theme : 'system'
    tempQuickBarCount.value = props.quickBarCount
    tempToolbarItems.value = [...props.toolbarItems]
  }
})

function getThemePreviewStyle(themeName: string): { background: string; color: string } {
  if (themeName === 'system') {
    const resolved = isSystemDark.value ? 'dark' : 'light'
    const th = TERMINAL_THEMES[resolved]
    return { background: th.background, color: th.foreground }
  }
  const th = TERMINAL_THEMES[themeName]
  return { background: th.background, color: th.foreground }
}

function toggleToolbarItem(key: string) {
  const idx = tempToolbarItems.value.indexOf(key)
  if (idx >= 0) {
    tempToolbarItems.value.splice(idx, 1)
  } else {
    tempToolbarItems.value.push(key)
  }
}

function handleConfirm() {
  let resolvedTheme: string
  let isThemeUserSet: boolean

  if (tempTheme.value === 'system') {
    resolvedTheme = isSystemDark.value ? 'dark' : 'light'
    isThemeUserSet = false
  } else {
    resolvedTheme = tempTheme.value
    isThemeUserSet = true
  }

  emit('confirm', {
    fontSize: tempFontSize.value,
    theme: resolvedTheme,
    isThemeUserSet,
    quickBarCount: tempQuickBarCount.value,
    toolbarItems: tempToolbarItems.value,
  })
}
</script>

<style scoped>
.settings-modal-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: var(--mobile-overlay-heavy);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 50;
  padding: 1rem;
}

.settings-modal {
  background: var(--mobile-bg-secondary);
  border-radius: 1rem;
  width: 100%;
  max-width: 360px;
  max-height: 80vh;
  overflow-y: auto;
}

.settings-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.5rem 1rem;
  border-bottom: 1px solid var(--mobile-border);
}

.settings-header h2 {
  font-size: 1rem;
  font-weight: 600;
  color: var(--mobile-text-primary);
  margin: 0;
}

.close-btn {
  padding: 0.25rem;
  background: none;
  border: none;
  color: var(--mobile-text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}

.close-btn:hover {
  color: var(--mobile-text-primary);
}

.settings-content {
  padding: 1rem;
}

.settings-section {
  margin-bottom: 1.5rem;
}

.settings-section:last-child {
  margin-bottom: 0;
}

.settings-label {
  display: block;
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--mobile-text-muted);
  margin-bottom: 0.75rem;
}

.font-size-control {
  display: flex;
  align-items: center;
  gap: 1rem;
}

.size-btn {
  width: 40px;
  height: 40px;
  border-radius: 0.5rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-primary);
  font-size: 1.25rem;
  cursor: pointer;
  transition: all 0.2s ease;
}

.size-btn:hover:not(:disabled) {
  background: var(--mobile-bg-hover);
}

.size-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.size-value {
  flex: 1;
  text-align: center;
  font-size: 1.125rem;
  font-weight: 500;
  color: var(--mobile-text-primary);
}

.theme-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 0.5rem;
}

.theme-btn {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.375rem;
  padding: 0.75rem 0.5rem;
  border-radius: 0.5rem;
  background: var(--mobile-bg-elevated);
  border: 2px solid transparent;
  cursor: pointer;
  transition: all 0.2s ease;
}

.theme-btn:hover {
  background: var(--mobile-bg-hover);
}

.theme-btn.active {
  border-color: var(--mobile-accent);
  background: var(--mobile-accent-muted);
  box-shadow: 0 0 12px rgba(0, 212, 255, 0.3);
}

.theme-preview {
  width: 100%;
  padding: 0.5rem;
  border-radius: 0.375rem;
  text-align: center;
  font-size: 0.875rem;
  font-weight: 600;
}

.theme-name {
  font-size: 0.75rem;
  color: var(--mobile-text-muted);
}

.theme-btn.active .theme-name {
  color: var(--mobile-accent);
  font-weight: 600;
}

.settings-hint {
  font-size: 0.75rem;
  color: var(--mobile-text-muted);
  margin: 0 0 0.75rem;
}

.toolbar-toggle-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 0.5rem;
}

.toolbar-toggle-btn {
  padding: 0.5rem;
  border-radius: 0.5rem;
  background: var(--mobile-bg-elevated);
  border: 2px solid transparent;
  color: var(--mobile-text-muted);
  font-size: 0.8rem;
  cursor: pointer;
  transition: all 0.2s ease;
  text-align: center;
}

.toolbar-toggle-btn:hover {
  background: var(--mobile-bg-hover);
}

.toolbar-toggle-btn.active {
  border-color: var(--mobile-accent);
  background: var(--mobile-accent-muted);
  color: var(--mobile-accent);
  font-weight: 600;
}

.settings-footer {
  display: flex;
  gap: 0.75rem;
  padding: 0.75rem 1rem;
  border-top: 1px solid var(--mobile-border);
}

.settings-footer-btn {
  flex: 1;
  padding: 0.75rem;
  border-radius: 0.5rem;
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.settings-footer-btn.cancel {
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-muted);
}

.settings-footer-btn.cancel:hover {
  background: var(--mobile-bg-hover);
  color: var(--mobile-text-primary);
}

.settings-footer-btn.confirm {
  background: var(--mobile-accent);
  border: none;
  color: var(--mobile-text-on-accent);
}

.settings-footer-btn.confirm:hover {
  background: #00b8e6;
}
</style>
```

- [ ] **Step 2: Commit**

```bash
git add src/components/TerminalSettingsModal.vue
git commit -m "refactor(mobile): extract settings modal to TerminalSettingsModal component"
```

---

### Task 5: Create TerminalHeader component

**Files:**
- Create: `src/components/TerminalHeader.vue`

Extract the header bar with back button, session name, toolbar items, and overflow menu. Header emits generic `action` events — it doesn't know about modals.

- [ ] **Step 1: Create the component**

```vue
<template>
  <header class="header">
    <button class="back-btn" @click="$emit('back')">
      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
      </svg>
    </button>
    <div class="header-title-area">
      <h1 class="header-title">{{ sessionName }}</h1>
    </div>
    <!-- 常驻工具按钮 -->
    <template v-for="item in visibleItems" :key="item.key">
      <button v-if="item.key === 'task'" class="task-btn" @click="$emit('action', 'task')" :title="t('mobile.terminal.pendingTasks')">
        <svg viewBox="0 0 24 24" class="w-5 h-5" fill="currentColor">
          <path d="M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 16H5V5h14v14zM17.99 9l-1.41-1.42-6.59 6.59-2.58-2.57-1.42 1.41 4 3.99z"/>
        </svg>
      </button>
      <button v-else-if="item.key === 'shortcut'" class="tool-btn" @click="$emit('action', 'shortcut')" :title="t('mobile.shortcutConfig.title')">
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16M8 6v12M16 6v12" />
        </svg>
      </button>
      <button v-else-if="item.key === 'clear'" class="tool-btn" @click="$emit('action', 'clear')" :title="t('mobile.terminal.clearScreen')">
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
        </svg>
      </button>
      <button v-else-if="item.key === 'refresh'" class="tool-btn" @click="$emit('action', 'refresh')" :title="t('mobile.terminal.refreshFormat')">
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
      </button>
      <button v-else-if="item.key === 'settings'" class="tool-btn" @click="$emit('action', 'settings')" :title="t('mobile.terminal.settings')">
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
        </svg>
      </button>
      <button v-else-if="item.key === 'folder'" class="folder-btn" :class="{ active: showSidebar }" @click="$emit('action', 'folder')" :title="t('mobile.terminal.files')">
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
        </svg>
      </button>
    </template>
    <!-- 溢出菜单按钮 -->
    <div v-if="overflowItems.length > 0" class="overflow-menu-wrapper">
      <button class="overflow-btn" :class="{ active: showOverflowMenu }" @click.stop="showOverflowMenu = !showOverflowMenu" :title="t('mobile.terminal.moreTools')">
        <svg class="w-5 h-5" viewBox="0 0 24 24" fill="currentColor">
          <circle cx="12" cy="5" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="12" cy="19" r="2"/>
        </svg>
      </button>
      <transition name="overflow-menu">
        <div v-if="showOverflowMenu" class="overflow-menu" @click.stop>
          <button v-if="isOverflowItem('task')" class="overflow-menu-item" @click="emitAction('task')">
            <svg viewBox="0 0 24 24" class="w-[18px] h-[18px]" fill="currentColor"><path d="M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 16H5V5h14v14zM17.99 9l-1.41-1.42-6.59 6.59-2.58-2.57-1.42 1.41 4 3.99z"/></svg>
            <span>{{ t('mobile.terminal.pendingTasks') }}</span>
          </button>
          <button v-if="isOverflowItem('shortcut')" class="overflow-menu-item" @click="emitAction('shortcut')">
            <svg class="w-[18px] h-[18px]" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16M8 6v12M16 6v12"/></svg>
            <span>{{ t('mobile.shortcutConfig.title') }}</span>
          </button>
          <button v-if="isOverflowItem('clear')" class="overflow-menu-item" @click="emitAction('clear')">
            <svg class="w-[18px] h-[18px]" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/></svg>
            <span>{{ t('mobile.terminal.clearScreen') }}</span>
          </button>
          <button v-if="isOverflowItem('refresh')" class="overflow-menu-item" @click="emitAction('refresh')">
            <svg class="w-[18px] h-[18px]" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg>
            <span>{{ t('mobile.terminal.refreshFormat') }}</span>
          </button>
          <button v-if="isOverflowItem('settings')" class="overflow-menu-item" @click="emitAction('settings')">
            <svg class="w-[18px] h-[18px]" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"/><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/></svg>
            <span>{{ t('mobile.terminal.settings') }}</span>
          </button>
          <button v-if="isOverflowItem('folder')" class="overflow-menu-item" :class="{ active: showSidebar }" @click="emitAction('folder')">
            <svg class="w-[18px] h-[18px]" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"/></svg>
            <span>{{ t('mobile.terminal.files') }}</span>
          </button>
        </div>
      </transition>
    </div>
    <!-- 点击溢出菜单外部关闭 -->
    <div v-if="showOverflowMenu" class="overflow-backdrop" @click="showOverflowMenu = false"></div>
  </header>
</template>

<script setup lang="ts">
/**
 * 终端页头部 - 返回按钮、会话名、工具栏、溢出菜单
 *
 * 工具栏按钮通过 emit('action', key) 通知父组件，不管理弹窗状态
 */
defineOptions({ name: 'TerminalHeader' })

import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { ToolbarItemConfig } from '@/components/TerminalSettingsModal.vue'

const props = defineProps<{
  sessionName: string
  visibleItems: ToolbarItemConfig[]
  allItems: ToolbarItemConfig[]
  showSidebar: boolean
}>()

const emit = defineEmits<{
  back: []
  action: [key: string]
}>()

const { t } = useI18n()
const showOverflowMenu = ref(false)

const overflowItems = computed(() => {
  const visibleKeys = new Set(props.visibleItems.map(item => item.key))
  return props.allItems.filter(item => !visibleKeys.has(item.key))
})

function isOverflowItem(key: string): boolean {
  return overflowItems.value.some(item => item.key === key)
}

function emitAction(key: string) {
  showOverflowMenu.value = false
  emit('action', key)
}
</script>

<style scoped>
.header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.75rem 1rem;
  background: var(--mobile-terminal-header);
  backdrop-filter: blur(20px);
  border-bottom: 1px solid var(--mobile-border);
  flex-shrink: 0;
  position: relative;
  z-index: 25;
}

.back-btn {
  padding: 0.5rem;
  margin-left: -0.5rem;
  color: var(--mobile-text-secondary);
  background: none;
  border: none;
  cursor: pointer;
  transition: color 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.back-btn:hover {
  color: var(--accent, #00d4ff);
}

.header-title-area {
  flex: 1;
  min-width: 0;
}

.header-title {
  font-size: 1rem;
  font-weight: 600;
  color: var(--mobile-text-primary);
  margin: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.tool-btn,
.task-btn,
.folder-btn,
.overflow-btn {
  padding: 0.5rem;
  border-radius: 0.5rem;
  background: var(--mobile-bg-elevated);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-secondary);
  cursor: pointer;
  transition: all 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
}

.tool-btn:hover,
.task-btn:hover,
.folder-btn:hover,
.overflow-btn:hover {
  border-color: rgba(0, 212, 255, 0.3);
}

.task-btn {
  position: relative;
}

.folder-btn.active {
  color: var(--mobile-accent);
  border-color: var(--mobile-border-active);
  background: var(--mobile-accent-muted);
}

.overflow-menu-wrapper {
  position: relative;
}

.overflow-btn.active {
  color: var(--mobile-accent);
  border-color: var(--mobile-border-active);
  background: var(--mobile-accent-muted);
}

.overflow-menu {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  min-width: 160px;
  background: var(--mobile-bg-secondary);
  border: 1px solid var(--mobile-border);
  border-radius: 0.75rem;
  padding: 0.375rem;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
  z-index: 30;
}

.overflow-menu-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  width: 100%;
  padding: 0.625rem 0.75rem;
  border-radius: 0.5rem;
  background: none;
  border: none;
  color: var(--mobile-text-primary);
  font-size: 0.875rem;
  cursor: pointer;
  transition: background 0.15s ease;
  text-align: left;
}

.overflow-menu-item:hover {
  background: var(--mobile-bg-hover);
}

.overflow-menu-item.active {
  color: var(--mobile-accent);
}

.overflow-backdrop {
  position: fixed;
  inset: 0;
  z-index: 29;
}

.overflow-menu-enter-active,
.overflow-menu-leave-active {
  transition: opacity 0.15s ease, transform 0.15s ease;
}

.overflow-menu-enter-from,
.overflow-menu-leave-to {
  opacity: 0;
  transform: translateY(-4px) scale(0.95);
}
</style>
```

- [ ] **Step 2: Commit**

```bash
git add src/components/TerminalHeader.vue
git commit -m "refactor(mobile): extract header toolbar to TerminalHeader component"
```

---

### Task 6: Rewrite TerminalView.vue as orchestrator

**Files:**
- Modify: `src/views/TerminalView.vue` (full rewrite)

Replace the 2248-line monolith with a ~550-line orchestrator that imports the extracted components, composable, and CSS file. All functionality and visual behavior must remain identical.

- [ ] **Step 1: Rewrite TerminalView.vue**

The new file imports `TerminalHeader`, `TerminalSettingsModal`, `TerminalConfirmModal`, `useTerminalScroll`, and `terminal.css`. It owns the terminal instance, manages modal visibility, and wires components together.

Key changes from original:
- Template uses `<TerminalHeader>`, `<TerminalSettingsModal>`, `<TerminalConfirmModal>` instead of inline markup
- Touch scroll/selection logic delegated to `useTerminalScroll`
- CSS moved to `styles/terminal.css` (imported)
- `applySettings` now applies theme via `terminalRef.value.options.theme` and calls `scroll.applySettings()`
- `clearTerminal` resets `scroll.currentLine` and `scroll.isUserScrolling`
- `disposeTerminal` calls `scroll.dispose()` instead of inline cleanup

```vue
<template>
  <div
    class="terminal-view"
    :style="terminalViewStyle"
  >
    <!-- Loading Overlay -->
    <transition name="loading-fade">
      <div v-if="!isTerminalReady" class="loading-overlay">
        <div class="loading-spinner"></div>
        <p class="loading-text">{{ t('mobile.terminal.preparing') }}</p>
      </div>
    </transition>

    <!-- Header -->
    <TerminalHeader
      :session-name="sessionName"
      :visible-items="visibleToolbarItems"
      :all-items="ALL_TOOLBAR_ITEMS"
      :show-sidebar="showSidebar"
      @back="handleBack"
      @action="handleToolbarAction"
    />

    <!-- Main Content: Terminal + Sidebar overlay -->
    <div class="main-content">
      <div class="terminal-output-area">
        <div
          ref="scrollContainer"
          class="terminal-scroll-container"
          :class="{ 'selection-mode': scroll.isSelectionMode }"
        >
          <div
            ref="xtermContainer"
            class="xterm-container"
            :style="scroll.xtermContainerStyle"
          ></div>
          <div class="scrollbar-track">
            <div
              class="scrollbar-thumb"
              :class="{ visible: scroll.scrollbarVisible }"
              :style="scroll.scrollbarThumbStyle"
            ></div>
          </div>
          <transition name="selection-bar">
            <div v-if="scroll.isSelectionMode && scroll.hasSelection" class="selection-action-bar">
              <button class="selection-action-btn" @click="scroll.copySelection">
                {{ t('common.button.copy') }}
              </button>
              <button class="selection-action-btn" @click="scroll.selectAllText">
                {{ t('mobile.terminal.selectAll') }}
              </button>
              <button class="selection-action-btn cancel" @click="scroll.exitSelectionMode">
                {{ t('common.button.cancel') }}
              </button>
            </div>
          </transition>
        </div>
      </div>

      <FileSidebar
        class="sidebar-overlay"
        :class="{ 'sidebar-hidden': !showSidebar }"
        :session-id="sessionId"
        @long-press="handleLongPress"
      />

      <div v-if="showSidebar" class="sidebar-backdrop" @click="showSidebar = false"></div>
    </div>

    <!-- Input Bar -->
    <TerminalInputBar
      :disabled="!isSessionActive"
      :is-connected="isConnected"
      :placeholder="inputPlaceholder"
      :is-landscape="isLandscape"
      @submit="handleInputSubmit"
      @execute="handleInputExecute"
      @special-key="handleSpecialKey"
      @shortcuts-panel-toggle="scroll.handleShortcutsPanelToggle"
    />

    <!-- Settings Modal -->
    <TerminalSettingsModal
      :visible="showSettings"
      :font-size="terminalSettings.fontSize"
      :theme="terminalSettings.theme"
      :is-theme-user-set="terminalSettings.isThemeUserSet"
      :quick-bar-count="assistStore.settings.quickBarCount"
      :toolbar-items="assistStore.settings.headerToolbarItems || ['folder']"
      :all-toolbar-items="ALL_TOOLBAR_ITEMS"
      :safe-area-style="settingsModalStyle"
      @confirm="handleSettingsConfirm"
      @cancel="showSettings = false"
    />

    <!-- Clear Confirm Modal -->
    <TerminalConfirmModal
      :visible="showClearConfirm"
      :message="t('mobile.terminal.clearScreen') + '?'"
      :safe-area-style="confirmModalStyle"
      @confirm="clearTerminal"
      @cancel="showClearConfirm = false"
    />
  </div>

  <!-- Task Picker -->
  <TaskPickerModal
    v-if="showTaskPicker"
    :tasks="presetTasks"
    :session-id="sessionId"
    @confirm="onTaskConfirm"
    @close="showTaskPicker = false"
  />

  <!-- Shortcut Config -->
  <ShortcutConfigModal :visible="showShortcutConfig" @close="showShortcutConfig = false" />
</template>

<script setup lang="ts">
/**
 * 终端视图 - 显示 PTY 输出和输入栏
 * 支持多会话切换和 ANSI 渲染
 */
defineOptions({ name: 'TerminalView' })

import { ref, computed, inject, type Ref, onMounted, onUnmounted, watch, nextTick } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import '@xterm/xterm/css/xterm.css'
import '@/styles/terminal.css'
import { useMobileConnection } from '@/composables/useMobileConnection'
import { useTerminalBuffer } from '@/composables/useTerminalBuffer'
import { wsResizeTerminal } from '@/composables/useMobileCommands'
import { httpSendSessionInput } from '@/composables/useHttpApi'
import { useOrientation } from '@/composables/useOrientation'
import { useTheme } from '@/composables/useTheme'
import { useSettingsStore } from '@/stores/settings'
import { useInputAssistantStore } from '@/stores/inputAssistant'
import { useTerminalScroll } from '@/composables/useTerminalScroll'
import TerminalHeader from '@/components/TerminalHeader.vue'
import TerminalSettingsModal from '@/components/TerminalSettingsModal.vue'
import type { ToolbarItemConfig, TerminalSettings } from '@/components/TerminalSettingsModal.vue'
import TerminalConfirmModal from '@/components/TerminalConfirmModal.vue'
import TerminalInputBar from '@/components/TerminalInputBar.vue'
import FileSidebar from '@/components/FileSidebar.vue'
import TaskPickerModal from '@/components/TaskPickerModal.vue'
import ShortcutConfigModal from '@/components/ShortcutConfigModal.vue'
import { useToast } from '@/composables/useToast'
import { writeClipboardText } from '@/utils/clipboard'
import { usePresetTasks, executeTask } from '@/composables/usePresetTasks'
import { TERMINAL_THEMES } from '@/config/terminalThemes'
import type { PresetTask } from '@/composables/model'

// ==================== Props & Route ====================

const router = useRouter()
const route = useRoute()
const { t } = useI18n()
const connection = useMobileConnection()
const toast = useToast()
const { isLandscape } = useOrientation()
const { isSystemDark } = useTheme()
const { store: bufferStore, writeBufferHistoryToTerminal, registerRealtimeHandler, unregisterRealtimeHandler, subscribeSession, unsubscribeSession, handleDisconnect, handleReconnect, handleSessionStopped } = useTerminalBuffer()
const settingsStore = useSettingsStore()
const assistStore = useInputAssistantStore()
const sessionId = computed(() => route.params.id as string)

// 安全区域从 App.vue inject
const safeArea = inject<Ref<{ top: number; bottom: number }>>('safeArea')!
const keyboardInfo = inject<Ref<{ keyboardHeight: number; isVisible: boolean }>>('keyboardInfo')!

// ==================== Task Picker ====================

const { tasks: presetTasks } = usePresetTasks()
const showTaskPicker = ref(false)

// ==================== Header Toolbar Config ====================

const ALL_TOOLBAR_ITEMS: ToolbarItemConfig[] = [
  { key: 'task', label: 'task', icon: 'task' },
  { key: 'shortcut', label: 'shortcut', icon: 'shortcut' },
  { key: 'clear', label: 'clear', icon: 'clear' },
  { key: 'refresh', label: 'refresh', icon: 'refresh' },
  { key: 'settings', label: 'settings', icon: 'settings' },
  { key: 'folder', label: 'folder', icon: 'folder' },
]

const visibleToolbarItems = computed(() => {
  const items = assistStore.settings.headerToolbarItems || ['folder']
  return ALL_TOOLBAR_ITEMS.filter(item => items.includes(item.key))
})

// ==================== State ====================

const xtermContainer = ref<HTMLDivElement | null>(null)
const scrollContainer = ref<HTMLDivElement | null>(null)
const isTerminalReady = ref(false)
const terminalRef = ref<Terminal | null>(null)
const fitAddonRef = ref<FitAddon | null>(null)
const resizeObserverRef = ref<ResizeObserver | null>(null)

const showSettings = ref(false)
const showClearConfirm = ref(false)
const showSidebar = ref(false)
const showShortcutConfig = ref(false)

const terminalSettings = ref({
  fontSize: assistStore.settings.terminalFontSize,
  theme: assistStore.settings.terminalTheme
    ?? (settingsStore.settings.ui.theme === 'system'
      ? (isSystemDark.value ? 'dark' : 'light')
      : settingsStore.settings.ui.theme) as string,
  isThemeUserSet: assistStore.settings.isTerminalThemeUserSet,
})

// ==================== Terminal Scroll ====================

const scroll = useTerminalScroll(terminalRef, scrollContainer)

// ==================== Computed ====================

const isConnected = computed(() =>
  connection.connectionStatus.value === 'connected' ||
  connection.connectionStatus.value === 'paired'
)

const session = computed(() =>
  connection.activeSessions.value.find(s => s.id === sessionId.value)
)

const sessionName = computed(() => session.value?.name || sessionId.value || t('desktop.terminal.title'))

const isSessionActive = computed(() => (session.value?.status || 'stopped') === 'running')

const inputPlaceholder = computed(() => {
  if (!isConnected.value) return t('mobile.input.disconnected') + '...'
  if (!isSessionActive.value) return t('mobile.connection.connectFailed')
  return t('mobile.input.commandPlaceholder')
})

const safeAreaTop = computed(() => safeArea.value.top || 0)
const keyboardHeight = computed(() => keyboardInfo.value.keyboardHeight || 0)

const terminalViewStyle = computed(() => ({
  paddingTop: `${safeAreaTop.value}px`,
  paddingBottom: keyboardHeight.value > 0 ? `${keyboardHeight.value}px` : '0px',
}))

const settingsModalStyle = computed(() => ({
  paddingTop: `${safeArea.value.top}px`,
  paddingBottom: `${safeArea.value.bottom}px`,
}))

const confirmModalStyle = computed(() => ({
  paddingTop: `${safeArea.value.top}px`,
  paddingBottom: `${safeArea.value.bottom}px`,
}))

// ==================== Watchers ====================

watch(() => keyboardInfo.value.keyboardHeight, () => {
  setTimeout(() => scroll.fitTerminal(fitAddonRef.value), 300)
})

watch(() => settingsStore.settings.ui.theme, (uiTheme) => {
  if (terminalSettings.value.isThemeUserSet) return
  const resolved = uiTheme === 'system'
    ? (isSystemDark.value ? 'dark' : 'light')
    : uiTheme
  if (terminalSettings.value.theme !== resolved) {
    terminalSettings.value.theme = resolved as string
    applyTerminalTheme()
  }
})

watch(isSystemDark, () => {
  if (terminalSettings.value.isThemeUserSet) return
  if (settingsStore.settings.ui.theme !== 'system') return
  terminalSettings.value.theme = isSystemDark.value ? 'dark' : 'light'
  applyTerminalTheme()
})

// ==================== Terminal Setup ====================

async function initTerminal() {
  if (!xtermContainer.value) return

  const theme = TERMINAL_THEMES[terminalSettings.value.theme]
  const term = new Terminal({
    theme: theme,
    fontFamily: '"Courier New", Courier, "Lucida Console", monospace',
    fontSize: terminalSettings.value.fontSize,
    lineHeight: 1.2,
    cursorBlink: true,
    cursorStyle: 'block',
    allowProposedApi: true,
    scrollback: 5000,
    convertEol: true,
    disableStdin: true,
    scrollSensitivity: 0.8,
  })

  terminalRef.value = term
  term.open(xtermContainer.value)

  const addon = new FitAddon()
  fitAddonRef.value = addon
  term.loadAddon(addon)
  term.loadAddon(new WebLinksAddon())

  // WebGL renderer — 后台加载
  try {
    const { WebglAddon } = await import('@xterm/addon-webgl')
    const webglAddon = new WebglAddon()
    term.loadAddon(webglAddon)
    webglAddon.onContextLoss(() => {})
  } catch {
    // WebGL 不可用时回退到 canvas 渲染器
  }

  writeBufferHistoryToTerminal(sessionId.value, term)
  registerRealtimeHandler(sessionId.value, term)

  setTimeout(() => {
    scroll.fitTerminal(fitAddonRef.value)
    scroll.setupViewportScroll()
  }, 100)

  const observer = new ResizeObserver(() => {
    requestAnimationFrame(() => scroll.fitTerminal(fitAddonRef.value))
  })
  resizeObserverRef.value = observer
  observer.observe(xtermContainer.value)

  window.addEventListener('resize', handleWindowResize)

  term.onResize(({ cols, rows }) => {
    if (isConnected.value && isSessionActive.value && sessionId.value) {
      wsResizeTerminal(sessionId.value, cols, rows).catch((e: Error) => {
        console.warn('[TerminalView] Resize failed:', e)
      })
    }
  })
}

function handleWindowResize() {
  setTimeout(() => scroll.fitTerminal(fitAddonRef.value), 100)
}

function disposeTerminal() {
  if (resizeObserverRef.value) {
    resizeObserverRef.value.disconnect()
    resizeObserverRef.value = null
  }
  window.removeEventListener('resize', handleWindowResize)

  if (sessionId.value) {
    unregisterRealtimeHandler(sessionId.value)
  }

  scroll.dispose()

  if (terminalRef.value) {
    terminalRef.value.dispose()
    terminalRef.value = null
    fitAddonRef.value = null
  }
  isTerminalReady.value = false
}

function applyTerminalTheme() {
  if (!terminalRef.value) return
  const theme = TERMINAL_THEMES[terminalSettings.value.theme]
  terminalRef.value.options.theme = theme
  scroll.fitTerminal(fitAddonRef.value)
}

// ==================== Input Handlers ====================

function handleInputSubmit(text: string) {
  if (!terminalRef.value) return
  if (isConnected.value && isSessionActive.value) {
    httpSendSessionInput(sessionId.value, text).then(result => {
      if (result.code !== 0) {
        console.error('[TerminalView] Send input failed:', result.message)
        toast.error(t('mobile.connection.connectFailed'))
      }
    })
  }
}

async function handleInputExecute(text: string) {
  if (!terminalRef.value) return
  if (isConnected.value && isSessionActive.value) {
    const result = await httpSendSessionInput(sessionId.value, text, 'enter')
    if (result.code !== 0) {
      console.error('[TerminalView] Send input failed:', result.message)
      toast.error(t('mobile.connection.connectFailed'))
    }
  }
}

function handleSpecialKey(key: string) {
  if (isConnected.value && isSessionActive.value) {
    httpSendSessionInput(sessionId.value, '', key).then(result => {
      if (result.code !== 0) {
        console.error('[TerminalView] Send special key failed:', result.message)
      }
    })
  }
}

// ==================== Toolbar Actions ====================

function handleToolbarAction(key: string) {
  switch (key) {
    case 'task': showTaskPicker.value = true; break
    case 'shortcut': showShortcutConfig.value = true; break
    case 'clear': showClearConfirm.value = true; break
    case 'refresh': refreshTerminal(); break
    case 'settings': showSettings.value = true; break
    case 'folder': showSidebar.value = !showSidebar.value; break
  }
}

// ==================== Settings ====================

function handleSettingsConfirm(settings: TerminalSettings) {
  terminalSettings.value.fontSize = settings.fontSize
  terminalSettings.value.theme = settings.theme
  terminalSettings.value.isThemeUserSet = settings.isThemeUserSet

  assistStore.saveSettings({
    quickBarCount: settings.quickBarCount,
    headerToolbarItems: settings.toolbarItems,
    terminalFontSize: terminalSettings.value.fontSize,
    terminalTheme: terminalSettings.value.isThemeUserSet ? terminalSettings.value.theme : null,
    isTerminalThemeUserSet: terminalSettings.value.isThemeUserSet,
  })

  applyTerminalTheme()
  scroll.applySettings(settings.theme, settings.fontSize, fitAddonRef.value)
  showSettings.value = false
}

// ==================== Clear Terminal ====================

function clearTerminal() {
  if (!terminalRef.value) return
  terminalRef.value.clear()
  scroll.currentLine.value = 0
  scroll.isUserScrolling.value = false
  showClearConfirm.value = false
}

// ==================== Refresh Terminal ====================

function refreshTerminal() {
  if (!fitAddonRef.value || !terminalRef.value) return

  fitAddonRef.value.fit()
  if (isConnected.value && isSessionActive.value) {
    wsResizeTerminal(sessionId.value, terminalRef.value.cols, terminalRef.value.rows).then(() => {
      toast.success(t('mobile.terminal.refreshed'))
    }).catch((e: Error) => {
      console.warn('[TerminalView] Refresh resize failed:', e)
      toast.error(t('mobile.terminal.refreshFailed'))
    })
  } else {
    toast.success(t('mobile.terminal.refreshed'))
  }
}

// ==================== Misc Handlers ====================

async function handleLongPress(name: string, path: string) {
  try {
    await writeClipboardText(path)
    toast.success(t('mobile.file.copied', { path }))
  } catch {
    toast.error(t('mobile.file.copyFailed'))
  }
}

function handleBack() {
  router.back()
}

async function onTaskConfirm(tasks: PresetTask[]) {
  showTaskPicker.value = false
  if (!isConnected.value || !isSessionActive.value) {
    toast.error(t('mobile.connection.connectFailed'))
    return
  }
  for (const task of tasks) {
    try {
      await executeTask(task, sessionId.value)
    } catch {
      toast.error(t('mobile.toolbox.sendFailed'))
      break
    }
  }
}

// ==================== Lifecycle ====================

onMounted(async () => {
  await nextTick()
  initTerminal()

  if (isSessionActive.value && isConnected.value) {
    await subscribeSession(sessionId.value)
  }

  isTerminalReady.value = true
})

onUnmounted(async () => {
  disposeTerminal()

  if (!isSessionActive.value) {
    await unsubscribeSession(sessionId.value)
  }
})

watch(isSessionActive, async (active, prevActive) => {
  if (!sessionId.value) return
  if (active && !prevActive) {
    await subscribeSession(sessionId.value)
  } else if (!active && prevActive) {
    await handleSessionStopped(sessionId.value)
  }
})

watch(isConnected, async (connected) => {
  if (!sessionId.value) return
  if (!connected) {
    handleDisconnect()
  } else if (connected && isSessionActive.value) {
    await subscribeSession(sessionId.value)
  }
})
</script>
```

- [ ] **Step 2: Verify the build compiles**

Run: `cd bedcode-mobile && npx vue-tsc --noEmit 2>&1 | head -30`
Expected: No type errors related to TerminalView or extracted components.

- [ ] **Step 3: Commit**

```bash
git add src/views/TerminalView.vue
git commit -m "refactor(mobile): rewrite TerminalView as orchestrator using extracted components"
```

---

### Task 7: Update component exports and code-map

**Files:**
- Modify: `src/components/index.ts` — add new component exports
- Modify: `docs/code-map.md` — update component list

- [ ] **Step 1: Add new components to index.ts**

Read `src/components/index.ts` and add exports for `TerminalHeader`, `TerminalSettingsModal`, `TerminalConfirmModal`.

- [ ] **Step 2: Update code-map.md**

Add the 3 new components to the component list in `docs/code-map.md` and add `useTerminalScroll` to the composable table.

- [ ] **Step 3: Commit**

```bash
git add src/components/index.ts docs/code-map.md
git commit -m "docs: update code-map and exports for TerminalView refactoring"
```

---

### Task 8: Final validation

- [ ] **Step 1: Build check**

Run: `cd bedcode-mobile && npx vue-tsc --noEmit`
Expected: No type errors.

- [ ] **Step 2: Line count check**

Run: `wc -l bedcode-mobile/src/views/TerminalView.vue bedcode-mobile/src/components/TerminalHeader.vue bedcode-mobile/src/components/TerminalSettingsModal.vue bedcode-mobile/src/components/TerminalConfirmModal.vue bedcode-mobile/src/composables/useTerminalScroll.ts bedcode-mobile/src/styles/terminal.css`

Expected:
- TerminalView.vue: ~500-600 lines
- Each new file matches its estimate
- Total lines ≈ original (no code lost or duplicated)

- [ ] **Step 3: Manual test on device**

Run: `cd bedcode-mobile && npm run tauri:android:dev`

Verify:
1. Terminal renders with session output
2. Touch scrolling + inertia works
3. Long press → selection mode → copy/select all/cancel works
4. Settings modal opens, font/theme/quick bar/toolbar config saves
5. Clear screen confirm dialog works
6. Header toolbar buttons work
7. Overflow menu works
8. Sidebar open/close works
9. Keyboard avoid + safe area works

- [ ] **Step 4: Final commit if any fixes needed**
