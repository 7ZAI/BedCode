/**
 * Mobile Global Terminal Manager
 *
 * 全局离屏 xterm.js 实例管理器
 * 会话创建时就创建隐藏实例，确保进入终端页面时数据已在缓冲区中
 */

import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import '@xterm/xterm/css/xterm.css'

// 离屏终端实例存储
interface TerminalInstance {
  terminal: Terminal
  fitAddon: FitAddon
  outputBuffer: string[]  // 原始输出数据，用于同步到新窗口
}

// 全局实例存储：sessionId -> TerminalInstance
const terminalInstances = new Map<string, TerminalInstance>()

// 全局输出监听器
let globalUnlisten: UnlistenFn | null = null

// 是否已初始化
let initialized = false

// 深色主题（移动端默认深色）
const DARK_THEME = {
  background: '#1a1a2e',
  foreground: '#e0e0e0',
  cursor: '#ffffff',
  cursorAccent: '#1a1a2e',
  selectionBackground: '#4a4a6a',
  black: '#000000',
  red: '#ff5555',
  green: '#50fa7b',
  yellow: '#f1fa8c',
  blue: '#bd93f9',
  magenta: '#ff79c6',
  cyan: '#8be9fd',
  white: '#bbbbbb',
  brightBlack: '#555555',
  brightRed: '#ff5555',
  brightGreen: '#50fa7b',
  brightYellow: '#f1fa8c',
  brightBlue: '#bd93f9',
  brightMagenta: '#ff79c6',
  brightCyan: '#8be9fd',
  brightWhite: '#ffffff',
}

// 浅色主题
const LIGHT_THEME = {
  background: '#ffffff',
  foreground: '#333333',
  cursor: '#000000',
  cursorAccent: '#ffffff',
  selectionBackground: '#b4d7ff',
  black: '#000000',
  red: '#cd3131',
  green: '#0dbc79',
  yellow: '#e5e510',
  blue: '#2472c8',
  magenta: '#bc3fbc',
  cyan: '#11a8cd',
  white: '#e5e5e5',
  brightBlack: '#666666',
  brightRed: '#f14c4c',
  brightGreen: '#23d18b',
  brightYellow: '#f5f543',
  brightBlue: '#3b8eea',
  brightMagenta: '#d670d6',
  brightCyan: '#29b8db',
  brightWhite: '#ffffff',
}

/**
 * 获取当前主题
 */
function getCurrentTheme(): 'light' | 'dark' {
  return document.documentElement.classList.contains('dark') ? 'dark' : 'light'
}

/**
 * 创建离屏 xterm.js 实例（不挂载到 DOM）
 */
export function createHiddenTerminal(sessionId: string): Terminal {
  // 如果已存在，直接返回
  const existing = terminalInstances.get(sessionId)
  if (existing) {
    return existing.terminal
  }

  // 创建新的 xterm 实例
  const terminal = new Terminal({
    fontSize: 14,
    fontFamily: '"SF Mono", "Menlo", "Monaco", "Courier New", monospace',
    theme: getCurrentTheme() === 'dark' ? DARK_THEME : LIGHT_THEME,
    cursorBlink: true,
    cursorStyle: 'block',
    scrollback: 10000,  // 移动端缓存 10000 行历史
    allowProposedApi: true,
    convertEol: true,
  })

  const fitAddon = new FitAddon()
  terminal.loadAddon(fitAddon)

  terminalInstances.set(sessionId, {
    terminal,
    fitAddon,
    outputBuffer: [],
  })

  console.log('[MobileGlobalTerminal] Created hidden terminal for session:', sessionId)
  return terminal
}

/**
 * 获取会话的离屏 xterm 实例
 */
export function getTerminal(sessionId: string): Terminal | null {
  return terminalInstances.get(sessionId)?.terminal || null
}

/**
 * 获取会话的 FitAddon
 */
export function getFitAddon(sessionId: string): FitAddon | null {
  return terminalInstances.get(sessionId)?.fitAddon || null
}

/**
 * 获取会话的输出缓冲区（用于同步）
 */
export function getOutputBuffer(sessionId: string): string {
  const instance = terminalInstances.get(sessionId)
  if (!instance) return ''
  return instance.outputBuffer.join('')
}

/**
 * 清除输出缓冲区
 */
export function clearOutputBuffer(sessionId: string): void {
  const instance = terminalInstances.get(sessionId)
  if (instance) {
    instance.outputBuffer = []
  }
}

/**
 * 检查会话是否有离屏实例
 */
export function hasTerminal(sessionId: string): boolean {
  return terminalInstances.has(sessionId)
}

/**
 * 销毁会话的离屏 xterm 实例
 */
export function destroyTerminal(sessionId: string): void {
  const instance = terminalInstances.get(sessionId)
  if (instance) {
    instance.terminal.dispose()
    terminalInstances.delete(sessionId)
    console.log('[MobileGlobalTerminal] Destroyed terminal for session:', sessionId)
  }
}

/**
 * 销毁所有离屏实例
 */
export function destroyAllTerminals(): void {
  for (const [sessionId, instance] of terminalInstances) {
    instance.terminal.dispose()
    console.log('[MobileGlobalTerminal] Destroyed terminal for session:', sessionId)
  }
  terminalInstances.clear()
}

/**
 * 将离屏实例挂载到 DOM 容器
 * @param sessionId 会话 ID
 * @param container DOM 容器元素
 * @returns 是否成功挂载
 */
export function mountTerminal(sessionId: string, container: HTMLElement): boolean {
  const instance = terminalInstances.get(sessionId)
  if (!instance) {
    console.warn('[MobileGlobalTerminal] No terminal instance for session:', sessionId)
    return false
  }

  // 检查是否已经挂载到其他容器
  if (instance.terminal.element) {
    // xterm.js 已经有 element，需要从旧容器移动到新容器
    const oldContainer = instance.terminal.element.parentElement
    if (oldContainer && oldContainer !== container) {
      oldContainer.removeChild(instance.terminal.element)
      container.appendChild(instance.terminal.element)
      console.log('[MobileGlobalTerminal] Moved terminal to new container:', sessionId)
    }
  } else {
    // 首次挂载
    instance.terminal.open(container)
    console.log('[MobileGlobalTerminal] Mounted terminal:', sessionId)
  }

  // 调整尺寸
  try {
    instance.fitAddon.fit()
  } catch (e) {
    console.warn('[MobileGlobalTerminal] Fit failed:', e)
  }

  // 更新主题
  instance.terminal.options.theme = getCurrentTheme() === 'dark' ? DARK_THEME : LIGHT_THEME

  // 滚动到底部
  instance.terminal.scrollToBottom()

  return true
}

/**
 * 从 DOM 卸载终端（保留实例和数据）
 * 注意：xterm.js 不支持真正的"卸载"，此函数只是将 element 从容器移除
 * 实际上我们依赖 KeepAlive 保持组件状态，不调用此函数
 */
export function unmountTerminal(sessionId: string): void {
  const instance = terminalInstances.get(sessionId)
  if (!instance) return

  // xterm.js 的 element 仍然存在，但可以被移动到其他容器
  // 这里不做任何操作，让 KeepAlive 保持状态
  console.log('[MobileGlobalTerminal] Keeping terminal instance:', sessionId)
}

/**
 * 清空终端显示
 */
export function clearTerminal(sessionId: string): void {
  const instance = terminalInstances.get(sessionId)
  if (instance) {
    instance.terminal.clear()
    instance.outputBuffer = []
  }
}

/**
 * 初始化全局 PTY 输出监听器
 * 在应用启动时调用，监听所有会话的输出
 */
export async function initGlobalTerminalManager(): Promise<void> {
  if (initialized) {
    console.log('[MobileGlobalTerminal] Already initialized')
    return
  }
  initialized = true

  // 监听 ws_output 事件
  globalUnlisten = await listen<{ session_id: string; data: string; index: number }>('ws_output', (event) => {
    const { session_id, data } = event.payload

    const instance = terminalInstances.get(session_id)
    if (instance) {
      // 写入 xterm 实例
      instance.terminal.write(data)
      // 同时保存到缓冲区
      instance.outputBuffer.push(data)
    }
  })

  console.log('[MobileGlobalTerminal] Initialized global output listener')

  // 监听主题变化，更新所有实例
  const observer = new MutationObserver(() => {
    const theme = getCurrentTheme() === 'dark' ? DARK_THEME : LIGHT_THEME
    for (const [, instance] of terminalInstances) {
      instance.terminal.options.theme = theme
    }
  })
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['class'],
  })
}

/**
 * 清理全局监听器
 */
export function cleanupGlobalTerminalManager(): void {
  if (globalUnlisten) {
    globalUnlisten()
    globalUnlisten = null
  }
  destroyAllTerminals()
  initialized = false
  console.log('[MobileGlobalTerminal] Cleanup completed')
}

/**
 * Composable: 使用全局终端管理器
 */
export function useGlobalTerminal(sessionId: string) {
  return {
    terminal: getTerminal(sessionId),
    fitAddon: getFitAddon(sessionId),
    outputBuffer: getOutputBuffer(sessionId),
    hasTerminal: hasTerminal(sessionId),
    create: () => createHiddenTerminal(sessionId),
    mount: (container: HTMLElement) => mountTerminal(sessionId, container),
    unmount: () => unmountTerminal(sessionId),
    clear: () => clearTerminal(sessionId),
    clearBuffer: () => clearOutputBuffer(sessionId),
    destroy: () => destroyTerminal(sessionId),
  }
}
