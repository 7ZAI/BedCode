/**
 * 全局终端历史缓存管理器
 *
 * 不监听 PTY 输出，只存储历史数据
 * 组件负责同步写入全局缓存
 */

import { Terminal } from '@xterm/xterm'
import '@xterm/xterm/css/xterm.css'

// 会话历史缓存：sessionId -> 原始输出数据
const sessionHistoryCache = new Map<string, string[]>()

// 隐藏的 xterm 实例（用于解析 ANSI 序列和计算行数）
const hiddenTerminals = new Map<string, Terminal>()

// 行数限制
const MAX_HISTORY_LINES = 50000

// 默认列数（与 Rust 端 TerminalConfig.default_cols 一致）
const DEFAULT_COLS = 120

// 深色主题
const darkTheme = {
  background: '#1a1e2e',
  foreground: '#e0e0e0',
  cursor: '#ffffff',
  cursorAccent: '#1a1e2e',
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

/**
 * 初始化会话的历史缓存（创建隐藏 xterm 实例）
 *
 * @param sessionId - 会话 ID
 * @param cols - 终端列数，应与显示实例一致，避免行数计算偏差导致缓存过早截断
 */
export function initSessionCache(sessionId: string, cols?: number): void {
  if (sessionHistoryCache.has(sessionId)) {
    return
  }

  sessionHistoryCache.set(sessionId, [])

  const effectiveCols = cols || DEFAULT_COLS

  // 创建隐藏的 xterm 实例用于追踪行数
  const terminal = new Terminal({
    fontSize: 14,
    fontFamily: 'Consolas, Monaco, Courier New, monospace',
    theme: darkTheme,
    cursorBlink: false,
    scrollback: MAX_HISTORY_LINES,
    cols: effectiveCols,
    rows: 40,
    allowProposedApi: true,
  })

  // 隐藏容器（不显示在 DOM 中）
  const hiddenContainer = document.createElement('div')
  hiddenContainer.style.position = 'absolute'
  hiddenContainer.style.left = '-9999px'
  hiddenContainer.style.width = '100%'
  hiddenContainer.style.height = '100%'
  document.body.appendChild(hiddenContainer)
  terminal.open(hiddenContainer)

  hiddenTerminals.set(sessionId, terminal)
}

/**
 * 同步隐藏 xterm 实例的列数与显示实例一致
 *
 * 显示终端 resize 时必须调用，否则隐藏实例仍按旧列数计算行数，
 * 导致缓存截断时机与显示实例不一致
 */
export function resizeHiddenTerminal(sessionId: string, cols: number, rows: number): void {
  const terminal = hiddenTerminals.get(sessionId)
  if (terminal && cols > 0 && rows > 0) {
    terminal.resize(cols, rows)
  }
}

/**
 * 追加输出数据到历史缓存
 * 同时写入隐藏的 xterm 实例以追踪行数
 */
export function appendOutput(sessionId: string, data: string): void {
  const cache = sessionHistoryCache.get(sessionId)
  const terminal = hiddenTerminals.get(sessionId)

  if (!cache) {
    console.warn('[TerminalCache] Session cache not initialized:', sessionId)
    return
  }

  // 写入隐藏 xterm 实例
  if (terminal) {
    terminal.write(data)
  }

  // 追加到缓存
  cache.push(data)

  // 行数限制：检查是否超出
  if (terminal) {
    const buffer = terminal.buffer.active
    const totalLines = buffer.length

    if (totalLines > MAX_HISTORY_LINES) {
      // 丢弃旧的缓存数据
      // 计算需要丢弃的行数
      const linesToRemove = totalLines - MAX_HISTORY_LINES

      // 估算：每行约 80 字符，丢弃相应数量的缓存条目
      // 实际上这里简化处理，因为 xterm 已经处理了 scrollback
      // 我们只需要确保缓存不会无限增长
      let removedCount = 0
      while (cache.length > 1 && removedCount < linesToRemove) {
        const removed = cache.shift()
        if (removed) {
          removedCount += (removed.match(/\n/g) || []).length || 1
        }
      }
    }
  }
}

/**
 * 获取会话的历史输出（用于恢复终端显示）
 */
export function getHistoryOutput(sessionId: string): string {
  const cache = sessionHistoryCache.get(sessionId)
  if (!cache) {
    return ''
  }
  return cache.join('')
}

/**
 * 清除会话的历史缓存
 */
export function clearHistoryCache(sessionId: string): void {
  const cache = sessionHistoryCache.get(sessionId)
  if (cache) {
    cache.length = 0
  }

  const terminal = hiddenTerminals.get(sessionId)
  if (terminal) {
    terminal.clear()
  }
}

/**
 * 销毁会话的历史缓存（停止会话时调用）
 */
export function destroySessionCache(sessionId: string): void {
  sessionHistoryCache.delete(sessionId)

  const terminal = hiddenTerminals.get(sessionId)
  if (terminal) {
    terminal.dispose()
    hiddenTerminals.delete(sessionId)
  }
}

/**
 * 检查会话是否有历史缓存
 */
export function hasSessionCache(sessionId: string): boolean {
  return sessionHistoryCache.has(sessionId)
}

/**
 * 清理所有缓存
 */
export function cleanupAllCaches(): void {
  for (const terminal of hiddenTerminals.values()) {
    terminal.dispose()
  }
  hiddenTerminals.clear()
  sessionHistoryCache.clear()
}

/**
 * Composable: 使用终端历史缓存
 */
export function useTerminalHistory(sessionId: string) {
  return {
    init: (cols?: number) => initSessionCache(sessionId, cols),
    append: (data: string) => appendOutput(sessionId, data),
    getHistory: () => getHistoryOutput(sessionId),
    clear: () => clearHistoryCache(sessionId),
    destroy: () => destroySessionCache(sessionId),
    hasCache: () => hasSessionCache(sessionId),
    resize: (cols: number, rows: number) => resizeHiddenTerminal(sessionId, cols, rows),
  }
}
