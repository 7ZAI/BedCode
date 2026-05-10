import { shallowRef } from 'vue'
import { WebviewWindow, getCurrentWindow } from '@tauri-apps/api/webviewWindow'
import type { SessionInfo } from '@/composables/useTauri'

/**
 * 终端窗口管理器
 *
 * 管理每个会话对应的终端窗口的创建、聚焦、关闭
 */
export function useSessionWindows() {
  // 使用 shallowRef 存储 WebviewWindow 实例，避免深度响应
  const windows = shallowRef<Map<string, WebviewWindow>>(new Map())

  /**
   * 为会话创建或聚焦终端窗口
   */
  async function openTerminalWindow(session: SessionInfo) {
    // 检查是否已有窗口
    const existingWindow = windows.value.get(session.id)
    if (existingWindow) {
      // 窗口已存在，聚焦它
      try {
        await existingWindow.setFocus()
        return
      } catch (e) {
        // 窗口可能已关闭，移除引用
        console.log('[useSessionWindows] Window focus failed, removing reference:', e)
        windows.value.delete(session.id)
      }
    }

    // 获取主窗口
    const mainWindow = getCurrentWindow()
    const mainPosition = await mainWindow.outerPosition()
    const mainSize = await mainWindow.outerSize()

    // 计算终端窗口位置（贴靠主窗口右侧）
    const terminalWidth = Math.floor(mainSize.width * 0.5)
    const terminalHeight = mainSize.height

    // 创建终端窗口
    const terminalWindow = new WebviewWindow(`terminal-${session.id}`, {
      url: `/terminal-window/${session.id}`,
      title: `终端 - ${session.name}`,
      width: terminalWidth,
      height: terminalHeight,
      x: mainPosition.x + mainSize.width,
      y: mainPosition.y,
      resizable: true,
      decorations: false,
      alwaysOnTop: false,
      skipTaskbar: false,
      center: false,
    })

    // 监听窗口关闭事件
    terminalWindow.once('tauri://close-requested', () => {
      windows.value.delete(session.id)
    })

    // 存储窗口引用
    windows.value.set(session.id, terminalWindow)

    // 监听窗口创建失败
    terminalWindow.once('tauri://error', (e) => {
      console.error('[useSessionWindows] Window creation error:', e)
      windows.value.delete(session.id)
    })
  }

  /**
   * 关闭指定会话的终端窗口
   */
  async function closeTerminalWindow(sessionId: string) {
    const window = windows.value.get(sessionId)
    if (window) {
      try {
        await window.close()
      } catch (e) {
        console.error('[useSessionWindows] Close window error:', e)
      }
      windows.value.delete(sessionId)
    }
  }

  /**
   * 关闭所有终端窗口
   */
  async function closeAllTerminalWindows() {
    for (const [sessionId, window] of windows.value) {
      try {
        await window.close()
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

  return {
    windows,
    openTerminalWindow,
    closeTerminalWindow,
    closeAllTerminalWindows,
    hasTerminalWindow,
  }
}