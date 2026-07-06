/**
 * Mock Terminal Composable
 *
 * 开发模式下的模拟终端会话，用于测试终端页面样式。
 * 每隔 5 秒随机输出 4~8 行带 ANSI 颜色的字符串。
 * 仅在 DEV 模式下可用，通过 localStorage 开关控制。
 */

import { ref, readonly } from 'vue'
import type { Terminal } from '@xterm/xterm'

// ==================== Constants ====================

/** 模拟会话 ID，TerminalView 据此识别 */
export const MOCK_SESSION_ID = '__mock_terminal__'

/** localStorage 开关键 */
const STORAGE_KEY = 'mock_terminal_enabled'

/** 输出间隔（毫秒） */
const OUTPUT_INTERVAL_MS = 5000

/** 每次输出行数范围 */
const MIN_LINES = 4
const MAX_LINES = 8

// ==================== Random Content ====================

/** ANSI 颜色码 */
const ANSI_COLORS = [
  '\x1b[32m', '\x1b[33m', '\x1b[34m', '\x1b[35m',
  '\x1b[36m', '\x1b[37m', '\x1b[90m', '\x1b[91m',
  '\x1b[92m', '\x1b[93m', '\x1b[94m', '\x1b[95m',
  '\x1b[96m',
]
const ANSI_RESET = '\x1b[0m'

const PATH_SEGMENTS = [
  'src/components/', 'src/composables/', 'src/views/', 'src/stores/',
  'src/utils/', 'src-tauri/src/', 'node_modules/', 'dist/',
  '.cargo/bin/', '~/.config/', '/usr/local/bin/', '/etc/systemd/',
]
const FILE_NAMES = [
  'index.ts', 'main.rs', 'App.vue', 'config.json', 'Cargo.toml',
  'package.json', 'utils.ts', 'mod.rs', 'handler.rs', 'session.rs',
]
const COMMANDS = [
  'npm run tauri:dev', 'cargo build --release', 'git pull origin dev',
  'docker compose up -d', 'npm run test:coverage', 'cargo clippy -- -D warnings',
  'git status', 'ls -la src/', 'cat Cargo.toml', 'rg "TODO" src/',
  'npm run lint', 'cargo test --all', 'git log --oneline -5',
  'ps aux | grep node', 'tail -f /var/log/syslog',
]
const MESSAGES = [
  'Compiling bedcode-mobile v1.1.0',
  'Finished dev [unoptimized + debuginfo] target(s)',
  'Running target/debug/bedcode-mobile',
  'warning: unused import: useToast',
  'error[E0425]: cannot find value config in this scope',
  "thread 'main' has panicked at 'called unwrap on None'",
  '   Compiling tokio v1.38.0',
  '   Downloaded 42 crates (3.1MB) in 1.2s',
  '   Building [======================>] 85/86 steps',
  '   Running unittests src/lib.rs',
  'test result: ok. 12 passed; 0 failed; 0 ignored',
  '   Done in 2.4s',
  "error: process didn't exit successfully",
  'warning: the following packages contain code that will be rejected',
  '   Updating crates.io index',
  '   Locking 24 packages to latest compatible versions',
]

function randomFrom<T>(arr: T[]): T {
  return arr[Math.floor(Math.random() * arr.length)]
}

function randomInt(min: number, max: number): number {
  return Math.floor(Math.random() * (max - min + 1)) + min
}

function generateRandomLine(): string {
  const type = randomInt(0, 5)
  const color = randomFrom(ANSI_COLORS)

  switch (type) {
    case 0: {
      const cmd = randomFrom(COMMANDS)
      return `[1;32m$[0m ${color}${cmd}${ANSI_RESET}`
    }
    case 1: {
      const msg = randomFrom(MESSAGES)
      return `${color}${msg}${ANSI_RESET}`
    }
    case 2: {
      const seg = randomFrom(PATH_SEGMENTS)
      const file = randomFrom(FILE_NAMES)
      return `${color}${seg}${file}${ANSI_RESET}`
    }
    case 3: {
      const now = new Date()
      const ts = now.toTimeString().split(' ')[0]
      const level = randomFrom(['INFO', 'WARN', 'DEBUG', 'ERROR', 'TRACE'])
      const levelColor = level === 'ERROR' ? '\x1b[31m' : level === 'WARN' ? '\x1b[33m' : color
      return `[90m${ts}[0m ${levelColor}[${level}][0m ${color}${randomFrom(MESSAGES)}${ANSI_RESET}`
    }
    case 4: {
      const filled = randomInt(3, 30)
      const total = 30
      const bar = '#'.repeat(filled) + '-'.repeat(total - filled)
      const pct = Math.round((filled / total) * 100)
      return `${color}[${bar}] ${pct}%${ANSI_RESET}`
    }
    default: {
      const msg = randomFrom(MESSAGES)
      return `${color}${msg}${ANSI_RESET}`
    }
  }
}

// ==================== State ====================

const enabled = ref(false)
let outputTimer: ReturnType<typeof setInterval> | null = null
let terminalInstance: Terminal | null = null

// ==================== Functions ====================

/** 检查是否为模拟会话 ID */
export function isMockSession(sessionId: string): boolean {
  return sessionId === MOCK_SESSION_ID
}

/** 读取 localStorage 并初始化启用状态 */
function loadState(): void {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    enabled.value = stored === 'true'
  } catch {
    enabled.value = false
  }
}

/** 将启用状态持久化到 localStorage */
function saveState(): void {
  try {
    localStorage.setItem(STORAGE_KEY, String(enabled.value))
  } catch {
    // 忽略存储失败
  }
}

/** 切换启用状态 */
function toggle(): void {
  enabled.value = !enabled.value
  saveState()
}

/** 向终端实例写入随机输出 */
function writeRandomOutput(): void {
  if (!terminalInstance) return
  const lineCount = randomInt(MIN_LINES, MAX_LINES)
  let output = ''
  for (let i = 0; i < lineCount; i++) {
    output += generateRandomLine() + '\r\n'
  }
  terminalInstance.write(output)
}

/** 绑定终端实例并启动定时输出 */
function startOutput(term: Terminal): void {
  terminalInstance = term
  term.write('\x1b[1;36m--- Mock Terminal (DEV) ---\x1b[0m\r\n')
  term.write('\x1b[90mSimulated terminal output for style testing.\x1b[0m\r\n')
  term.write('\x1b[90mOutput every 5s, 4~8 lines each.\x1b[0m\r\n\r\n')

  if (outputTimer) clearInterval(outputTimer)
  outputTimer = setInterval(writeRandomOutput, OUTPUT_INTERVAL_MS)

  setTimeout(writeRandomOutput, 2000)
}

/** 停止定时输出并解绑 */
function stopOutput(): void {
  if (outputTimer) {
    clearInterval(outputTimer)
    outputTimer = null
  }
  terminalInstance = null
}

// 初始化加载
loadState()

// ==================== Composable ====================

/**
 * 模拟终端 composable
 *
 * 仅在 DEV 模式下可用，用于测试终端页面样式。
 */
export function useMockTerminal() {
  return {
    /** 是否启用模拟终端（仅 DEV 模式有意义） */
    enabled: readonly(enabled),
    /** 是否处于 DEV 模式 */
    isDev: import.meta.env.DEV,
    /** 切换启用状态 */
    toggle,
    /** 绑定终端并启动输出 */
    startOutput,
    /** 停止输出并解绑 */
    stopOutput,
  }
}
