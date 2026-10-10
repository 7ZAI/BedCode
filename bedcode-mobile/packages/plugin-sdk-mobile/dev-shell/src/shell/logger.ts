/**
 * 壳层日志桥
 * -----------------------------------------------------------------------------
 * dev-shell 的壳复用宿主的 `src/utils/frontendLogger` 同款职责（带上下文的统一日志），
 * 但落到 dev-shell 自己的日志通道——即 `registry.ts` 的 `pushLog`，它同时写
 * 浏览器 console 与右下角日志面板。
 *
 * 为什么不直接用 console：壳内出问题时，用户的第一现场是那个日志面板；
 * 只进 console 等于把最需要被看见的错误藏起来。
 */

import { pushLog } from '../registry'

/** 壳层日志级别（与 DevLogEntry 对齐） */
type Level = 'debug' | 'info' | 'warn' | 'error'

/**
 * 壳层 logger
 *
 * `scope` 是壳自身的固定标识（`Shell`），插件侧日志仍走 `context.logger`，
 * 两条通道在日志面板里可区分。
 */
export const logger = {
  debug: (message: string) => pushLog('debug', 'Shell', message),
  info: (message: string) => pushLog('info', 'Shell', message),
  warn: (message: string) => pushLog('warn', 'Shell', message),
  error: (message: string) => pushLog('error', 'Shell', message),
} satisfies Record<Level, (message: string) => void>

export type { Level }