/**
 * 全局 xterm.js 实例管理器
 *
 * 在会话创建时提前创建隐藏的 xterm.js 实例
 * PTY 输出直接写入对应的 xterm 实例
 * 用户打开终端窗口时直接显示，无需额外缓存层
 */

import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { onPtyOutput } from '@/modules/desktop/composables/useDesktopCommands'
import '@xterm/xterm/css/xterm.css'

// xterm 实例存储：sessionId -> { terminal, outputEntries }
const terminalInstances = new Map<string, {
  terminal: Terminal
  fitAddon: FitAddon
  outputBuffer: string[]  // 原始输出数据，用于同步到新窗口
}>()

// 全局监听器
let globalUnlisten: (() => void) | null = null

// 深色主题
const darkTheme = {
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

/**
 * 初始化全局 PTY 输出监听器
 */
export async function initGlobalTerminalManager(): Promise<void> {
  if (globalUnlisten) {
    return
  }

  globalUnlisten = await onPtyOutput((event: any) => {
    const sessionId = event.sessionId || event.session_id
    if (!sessionId) {
      console.warn('[TerminalManager] Event without sessionId:', event)
      return
    }

    const instance = terminalInstances.get(sessionId)
    if (instance) {
      // 解码 base64 数据
      let decodedData: string
      try {
        const binaryString = atob(event.data)
        const bytes = new Uint8Array(binaryString.length)
        for (let i = 0; i < binaryString.length; i++) {
          bytes[i] = binaryString.charCodeAt(i)
        }
        decodedData = new TextDecoder('utf-8', { fatal: false }).decode(bytes)
      } catch (e) {
        console.error('[TerminalManager] Failed to decode base64:', e)
        decodedData = event.data
      }

      // 写入 xterm 实例
      instance.terminal.write(decodedData)
      // 同时保存到缓冲区（用于同步到新窗口）
      instance.outputBuffer.push(decodedData)
    }
  })
}

/**
 * 为会话创建隐藏的 xterm.js 实例
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
    fontFamily: 'Consolas, Monaco, Courier New, monospace',
    theme: darkTheme,
    cursorBlink: false,
    cursorStyle: 'block',
    cursorWidth: 1,
    scrollback: 50000,  // 缓存 50000 行历史
    allowProposedApi: true,
  })

  const fitAddon = new FitAddon()
  terminal.loadAddon(fitAddon)

  terminalInstances.set(sessionId, {
    terminal,
    fitAddon,
    outputBuffer: [],
  })

  return terminal
}

/**
 * 获取会话的 xterm 实例
 */
export function getTerminal(sessionId: string): Terminal | null {
  const instance = terminalInstances.get(sessionId)
  return instance?.terminal || null
}

/**
 * 获取会话的输出缓冲区（用于同步到新窗口）
 */
export function getOutputBuffer(sessionId: string): string {
  const instance = terminalInstances.get(sessionId)
  if (!instance) return ''
  return instance.outputBuffer.join('')
}

/**
 * 清除输出缓冲区（释放内存）
 */
export function clearOutputBuffer(sessionId: string): void {
  const instance = terminalInstances.get(sessionId)
  if (instance) {
    instance.outputBuffer = []
  }
}

/**
 * 销毁会话的 xterm 实例
 */
export function destroyTerminal(sessionId: string): void {
  const instance = terminalInstances.get(sessionId)
  if (instance) {
    instance.terminal.dispose()
    terminalInstances.delete(sessionId)
  }
}

/**
 * 清理所有实例
 */
export function cleanupAllTerminals(): void {
  for (const [, instance] of terminalInstances) {
    instance.terminal.dispose()
  }
  terminalInstances.clear()

  if (globalUnlisten) {
    globalUnlisten()
    globalUnlisten = null
  }
}

/**
 * Composable: 使用全局终端管理器
 */
export function useGlobalTerminal(sessionId: string) {
  return {
    terminal: getTerminal(sessionId),
    outputBuffer: getOutputBuffer(sessionId),
    create: () => createHiddenTerminal(sessionId),
    clearBuffer: () => clearOutputBuffer(sessionId),
    destroy: () => destroyTerminal(sessionId),
  }
}
