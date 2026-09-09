/**
 * 前端日志框架（基于 loglevel）——替代原生 console.* 的统一日志入口
 *
 * 为什么用 loglevel 而非裸 console：
 * - 统一 level 语义（trace/debug/info/warn/error），与后端 tracing 对齐
 * - methodFactory 官方扩展点：日志输出可整体接管（转发落盘 / 生产剥离），
 *   无需逐处 console 嫁接
 * - 生产构建零开销：空函数实现，日志"相当于不存在"
 *
 * 行为契约（双端一致，与旧 devConsoleRelay 等价）：
 * - dev（import.meta.env.DEV）：先调原始 console（DevTools 照常可见），
 *   再批量转发到 Rust `report_frontend_log`（target=frontend，logcat 可见）
 * - release：methodFactory 返回空函数，连 DevTools 控制台也不打印
 * - 转发失败静默（仅一次警告），防止递归输出
 */

import loglevel from 'loglevel'
import { invoke } from '@tauri-apps/api/core'

/** 转发给后端的单条日志记录（level 后端已归一化，见 Rust normalized_level） */
export interface FrontendLogEntry {
  level: string
  message: string
}

/** 攒批条数阈值，达到后立即发送 */
const FLUSH_THRESHOLD = 50
/** 定时发送间隔（毫秒），保证低频日志也能及时落盘 */
const FLUSH_INTERVAL_MS = 400

const originalMethods: Record<string, (...args: unknown[]) => void> = {
  trace: console.trace,
  debug: console.debug,
  log: console.log,
  info: console.info,
  warn: console.warn,
  error: console.error,
}

/** 将单个日志参数序列化为文本：Error 取 stack，对象尝试 JSON，其余 String 化 */
function stringifyArg(arg: unknown): string {
  if (arg instanceof Error) {
    // stack 比 message 信息全（含调用栈），AI agent 排查更有效
    return arg.stack || arg.message
  }
  if (typeof arg === 'object' && arg !== null) {
    try {
      const json = JSON.stringify(arg)
      return json ?? String(arg)
    } catch {
      // 循环引用等无法直接 JSON 化：用 visited-set replacer 保留字段可读性
      const seen = new WeakSet<object>()
      try {
        const json = JSON.stringify(arg, (_key, value) => {
          if (typeof value === 'object' && value !== null) {
            if (seen.has(value)) return '[Circular]'
            seen.add(value)
          }
          return value
        })
        return json ?? String(arg)
      } catch {
        return String(arg)
      }
    }
  }
  if (arg === undefined) return 'undefined'
  return String(arg)
}

/** 将多参数拼接为单条文本（落盘后可直接 grep） */
export function formatLogArgs(args: unknown[]): string {
  return args.map(stringifyArg).join(' ')
}

/** 当前是否处于 dev 构建（测试可注入） */
const isDev = () => import.meta.env.DEV

/** 批量转发状态（仅 dev 建立，release 不触碰） */
let entries: FrontendLogEntry[] = []
let flushTimer: ReturnType<typeof setInterval> | null = null
let reportedFailure = false

/** 发送攒批日志到 Rust（失败静默，仅 warn 一次，避免转发本身引发递归日志） */
function flush() {
  if (entries.length === 0) return
  const batch = entries
  entries = []
  invoke('report_frontend_log', { logs: batch }).catch(() => {
    if (!reportedFailure) {
      reportedFailure = true
      originalMethods.warn.call(console, '[frontendLogger] 日志转发失败（仅 dev 生效，release 无此命令）')
    }
  })
}

/** 构造 loglevel 的 methodFactory：dev 转发 + 原始 console，release 空函数 */
function makeMethodFactory() {
  const dev = isDev()
  // 语义映射：loglevel 方法 → 后端级别（loglevel 无 log 方法，debug 即对应原 console.log）
  const levelMap: Record<string, string> = {
    trace: 'debug',
    debug: 'debug',
    info: 'info',
    warn: 'warn',
    error: 'error',
  }

  return (
    methodName: string,
    _logLevel: loglevel.LogLevelNumbers,
    _loggerName: string | symbol,
  ): ((...args: unknown[]) => void) => {
    if (!dev) {
      // 生产：空函数，日志零开销零输出
      return () => {}
    }

    const original = originalMethods[methodName] ?? originalMethods.log
    const level = levelMap[methodName] ?? 'debug'

    // 启动定时器（幂等）
    if (!flushTimer) {
      flushTimer = setInterval(flush, FLUSH_INTERVAL_MS)
    }

    return (...args: unknown[]): void => {
      original.apply(console, args)
      const message = formatLogArgs(args)
      if (!message) return
      entries.push({ level, message })
      if (entries.length >= FLUSH_THRESHOLD) flush()
    }
  }
}

// SAFETY: loglevel 的 Logger 类型没有 log 方法（方法名是 debug），
// 但业务代码大量使用 console.log 语义的日志，此处显式扩展类型并补挂 log 方法。
// 断言方向是「收窄」：运行时的 logger 就是 loglevel Logger，扩展字段仅 TS 可见。
interface BedCodeLogger extends loglevel.Logger {
  /** console.log 语义映射到 debug（与旧 devConsoleRelay 一致） */
  log: (...args: unknown[]) => void
}

// 显式导出：业务代码统一从本模块 import logger 对象调用
// （不导出 error/log 等具名，避免与业务 catch 参数/局部变量遮蔽）
const logger = loglevel.getLogger('bedcode') as BedCodeLogger
logger.setLevel(loglevel.levels.DEBUG)
logger.methodFactory = makeMethodFactory()
// 重建 logger 内部方法（methodFactory 变更后需调用）
logger.setLevel(logger.getLevel())
logger.log = logger.debug.bind(logger)

export { logger }

/** 供测试/调试：访问原始 logger 实例 */
export const rawLogger = logger

/** 供测试：重置转发状态（flush 定时器、失败标记） */
export function resetLoggerState() {
  if (flushTimer) {
    clearInterval(flushTimer)
    flushTimer = null
  }
  entries = []
  reportedFailure = false
}

export default logger