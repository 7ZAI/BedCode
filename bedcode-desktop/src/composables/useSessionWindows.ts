import { shallowRef } from 'vue'
import i18n from '@/locales'
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { getCurrentWindow, PhysicalPosition } from '@tauri-apps/api/window'
import { emit, listen, emitTo, type UnlistenFn } from '@tauri-apps/api/event'
import type { SessionInfo } from '@/composables/useDesktopCommands'

// Re-export from model
import type { TerminalWindowState } from './model'
export type { TerminalWindowState }


const SNAP_THRESHOLD = 15  // 贴靠阈值（像素）



// ==================== 单例模式 ====================
// 模块级别的状态，确保所有组件共享同一个实例
const windows = shallowRef<Map<string, TerminalWindowState>>(new Map())

// 跟踪正在关闭的窗口，防止重复调用
const closingWindows = new Set<string>()

// 存储事件监听器（模块级别，只初始化一次）
let unlistenMoved: UnlistenFn | null = null
let unlistenResized: UnlistenFn | null = null
let isListenersInitialized = false

/**
 * 初始化主窗口事件监听（只执行一次）
 */
async function initMainWindowListeners() {
  if (isListenersInitialized) return

  const mainWindow = getCurrentWindow()

  // 监听主窗口移动
  unlistenMoved = await mainWindow.onMoved(async (event) => {
    const mainPos = event.payload
    // 发送主窗口位置给所有终端窗口
    await emit('main-window-moved', {
      x: mainPos.x,
      y: mainPos.y,
      width: 0, // PhysicalPosition 没有 width/height
      height: 0
    })
  })

  // 监听主窗口大小变化
  unlistenResized = await mainWindow.onResized(async (event) => {
    const mainSize = event.payload
    await emit('main-window-resized', {
      width: mainSize.width,
      height: mainSize.height
    })
  })

  isListenersInitialized = true
}

/**
 * 终端窗口管理器
 *
 * 管理每个会话对应的终端窗口的创建、聚焦、关闭、贴靠
 * 使用单例模式，确保所有组件共享状态
 */
export function useSessionWindows() {
  // 初始化事件监听（只执行一次）
  initMainWindowListeners()

  /**
   * 为会话创建或聚焦终端窗口
   *
   * 返回 Promise：新窗口时在窗口 show 完成后 resolve（页面就绪事件或 4s 兜底），
   * 创建失败时 reject；已有窗口时直接聚焦立即返回（调用方据此决定是否显示 loading）
   */
  async function openTerminalWindow(session: SessionInfo): Promise<void> {
    // 检查是否已有窗口
    const existingState = windows.value.get(session.id)
    if (existingState) {
      // 窗口已存在，聚焦它
      try {
        await existingState.window.setFocus()
        return
      } catch (e) {
        // 窗口可能已关闭，移除引用
        console.log('[useSessionWindows] Window focus failed, removing reference:', e)
        windows.value.delete(session.id)
      }
    }

    // 获取主窗口
    // Tauri 的 innerSize/outerSize/outerPosition 返回物理像素，
    // 而 WebviewWindow 创建参数（width/height/x/y）需要逻辑像素，
    // 高分屏（缩放 > 1）下不转换会导致终端窗口被放大 scaleFactor 倍
    const mainWindow = getCurrentWindow()
    const scaleFactor = await mainWindow.scaleFactor()
    const mainPosition = (await mainWindow.outerPosition()).toLogical(scaleFactor)
    const mainSize = (await mainWindow.outerSize()).toLogical(scaleFactor)
    const mainInnerSize = (await mainWindow.innerSize()).toLogical(scaleFactor)

    console.log('[useSessionWindows] Main window position (logical):', mainPosition)
    console.log('[useSessionWindows] Main window size (outerSize, logical):', mainSize)
    console.log('[useSessionWindows] Main window size (innerSize, logical):', mainInnerSize)

    // 计算终端窗口位置（紧贴主窗口右侧）
    // 使用 innerSize 确保与主窗口内容区高度一致
    const terminalWidth = Math.floor(mainInnerSize.width * 0.6)
    const terminalHeight = mainInnerSize.height

    console.log('[useSessionWindows] Terminal window size - width:', terminalWidth, 'height:', terminalHeight)

    // 计算新窗口位置，确保不超过屏幕边界（window.screen 为逻辑像素）
    let terminalX = mainPosition.x + mainSize.width
    const screenWidth = window.screen.width

    // 如果窗口会超出屏幕右侧，放到主窗口左侧
    if (terminalX + terminalWidth > screenWidth) {
      terminalX = mainPosition.x - terminalWidth
    }

    // 如果左侧也超出（屏幕太窄），则居中显示
    if (terminalX < 0) {
      terminalX = Math.floor((screenWidth - terminalWidth) / 2)
    }

    const windowLabel = `terminal-${session.id}`

    // 创建终端窗口 - 使用独立的 terminal.html 页面
    // 先隐藏创建，等页面内容就绪后再显示，避免加载过程中的闪屏
    const terminalWindow = new WebviewWindow(windowLabel, {
      url: `/terminal-window/${session.id}`,
      title: i18n.global.t('common.misc.terminalTitle', { name: session.name }),
      width: terminalWidth,
      height: terminalHeight,
      x: terminalX,
      y: mainPosition.y,
      resizable: true,
      decorations: false,
      alwaysOnTop: false,
      skipTaskbar: false,
      center: false,
      visible: false,
      backgroundColor: '#111827',
    })

    // 就绪 Promise：窗口 show 完成后 resolve（调用方据此关闭 loading），
    // 创建失败时 reject 让调用方提示错误
    let resolveReady: (() => void) | null = null
    let rejectReady: ((e: unknown) => void) | null = null
    const ready = new Promise<void>((resolve, reject) => {
      resolveReady = resolve
      rejectReady = reject
    })

    let unlistenReady: UnlistenFn | null = null
    let readyTimeout: number | null = null
    let readyShown = false

    /**
     * 显示终端窗口并通知页面播放显现动画
     */
    async function showTerminalWindow() {
      if (readyShown) return
      readyShown = true
      if (unlistenReady) {
        unlistenReady()
        unlistenReady = null
      }
      if (readyTimeout !== null) {
        window.clearTimeout(readyTimeout)
        readyTimeout = null
      }
      try {
        // show 成功即视为就绪：焦点与动画是锦上添花，失败不影响窗口展示
        await terminalWindow.show()
        resolveReady?.()
        try {
          await terminalWindow.setFocus()
        } catch (e) {
          console.error('[useSessionWindows] Focus error after show:', e)
        }
        // 通知终端页面播放显现动画
        try {
          await emitTo(windowLabel, 'terminal-show', { sessionId: session.id })
        } catch (e) {
          console.error('[useSessionWindows] Emit terminal-show error:', e)
        }
      } catch (e) {
        rejectReady?.(e)
      }
    }

    // 兜底：页面未能发出就绪事件时，超时后仍然显示窗口
    readyTimeout = window.setTimeout(() => {
      showTerminalWindow()
    }, 4000)

    // 页面内容就绪后立即显示（并清除兜底超时）
    unlistenReady = await listen<{ sessionId: string }>('terminal-ready', (event) => {
      if (event.payload.sessionId !== session.id) return
      showTerminalWindow()
    })

    const cleanup = () => {
      if (unlistenReady) {
        unlistenReady()
        unlistenReady = null
      }
      if (readyTimeout !== null) {
        window.clearTimeout(readyTimeout)
        readyTimeout = null
      }
    }

    // 监听窗口关闭事件
    terminalWindow.once('tauri://close-requested', () => {
      cleanup()
      windows.value.delete(session.id)
    })

    // 存储窗口状态
    windows.value.set(session.id, {
      window: terminalWindow,
      isSnapped: false,
      snapDirection: null,
      lastPosition: { x: terminalX, y: mainPosition.y }
    })
    console.log('[useSessionWindows] Window stored, keys:', Array.from(windows.value.keys()))

    // 监听窗口创建失败
    terminalWindow.once('tauri://error', (e) => {
      cleanup()
      console.error('[useSessionWindows] Window creation error:', e)
      windows.value.delete(session.id)
      rejectReady?.(e)
    })

    // 等待窗口就绪（就绪事件或 4s 兜底超时）
    await ready
  }

  /**
   * 关闭指定会话的终端窗口
   * @param sessionId - 会话 ID
   */
  async function closeTerminalWindow(sessionId: string) {
    // 防止重复调用
    if (closingWindows.has(sessionId)) {
      console.log('[useSessionWindows] Window already closing, skipping:', sessionId)
      return
    }

    console.log('[useSessionWindows] closeTerminalWindow called, sessionId:', sessionId)
    console.log('[useSessionWindows] windows.value keys:', Array.from(windows.value.keys()))

    // 标记为正在关闭
    closingWindows.add(sessionId)

    try {
      // 使用 getByLabel 检查窗口是否仍然存在
      const windowLabel = `terminal-${sessionId}`
      console.log('[useSessionWindows] Checking window with label:', windowLabel)

      const window = await WebviewWindow.getByLabel(windowLabel)
      console.log('[useSessionWindows] getByLabel result:', window)
      console.log('[useSessionWindows] window type:', window ? typeof window : 'null')

      if (window) {
        console.log('[useSessionWindows] Window exists, closing...')
        console.log('[useSessionWindows] window.label:', window.label)
        try {
          await window.close()
          console.log('[useSessionWindows] Close request sent')
        } catch (e) {
          console.error('[useSessionWindows] Close error:', e)
        }
      } else {
        console.log('[useSessionWindows] Window already closed or never existed')
      }

      // 从本地状态中移除
      windows.value.delete(sessionId)
    } catch (e) {
      console.error('[useSessionWindows] Error checking window:', e)
      windows.value.delete(sessionId)
    } finally {
      // 移除正在关闭的标记
      closingWindows.delete(sessionId)
    }
  }

  /**
   * 关闭所有终端窗口
   */
  async function closeAllTerminalWindows() {
    for (const [sessionId, state] of windows.value) {
      try {
        await state.window.close()
      } catch (e) {
        console.error('[useSessionWindows] Close window error:', e)
      }
    }
    windows.value.clear()
  }

  /**
   * 检查指定会话是否有打开的终端窗口
   */
  function hasTerminalWindow(sessionId: string): boolean {
    return windows.value.has(sessionId)
  }

  /**
   * 更新终端窗口的贴靠状态
   */
  async function updateWindowSnapState(sessionId: string, isSnapped: boolean, snapDirection: 'left' | 'right' | null) {
    const state = windows.value.get(sessionId)
    if (state) {
      state.isSnapped = isSnapped
      state.snapDirection = snapDirection
    }
  }

  /**
   * 获取终端窗口的当前位置
   */
  async function getTerminalWindowPosition(sessionId: string): Promise<{ x: number; y: number; width: number; height: number } | null> {
    const state = windows.value.get(sessionId)
    if (!state) return null

    try {
      const position = await state.window.outerPosition()
      const size = await state.window.outerSize()
      return {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height
      }
    } catch (e) {
      return null
    }
  }

  /**
   * 设置终端窗口位置
   */
  async function setTerminalWindowPosition(sessionId: string, x: number, y: number) {
    const state = windows.value.get(sessionId)
    if (!state) return

    try {
      await state.window.setPosition(new PhysicalPosition(x, y))
    } catch (e) {
      console.error('[useSessionWindows] Set position error:', e)
    }
  }

  return {
    windows,
    openTerminalWindow,
    closeTerminalWindow,
    closeAllTerminalWindows,
    hasTerminalWindow,
    updateWindowSnapState,
    getTerminalWindowPosition,
    setTerminalWindowPosition,
  }
}
