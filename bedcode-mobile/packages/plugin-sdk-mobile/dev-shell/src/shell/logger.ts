/**
 * 壳层日志桥
 * -----------------------------------------------------------------------------
 * dev-shell 的壳复用宿主的 `src/utils/frontendLogger` 同款职责（带上下文的统一日志），
 * 但落到 dev-shell 自己的日志通道——即 `registry.ts` 的 `pushLog`，它同时写
 * 浏览器 console 与右下角日志面板。
 *
 * 为什么不直接用 console：壳内出问题时，用户的第一现场是那个日志面板；
 * 只进 console 等于把最需要被看见的错误藏起来。
 *
 * 与宿主 frontendLogger 的差异：宿主的实参是 (message, error?)，这里同样接受可选
 * 错误对象并拼进 message（日志面板只显示一条 message 字段）——这样从宿主移植过来的
 * 两参调用无需逐个改写，且错误细节不丢。
 */

import { pushLog } from '../registry'

/** 壳层日志级别（与 DevLogEntry 对齐） */
type Level = 'debug' | 'info' | 'warn' | 'error'

/** 稳定低熵的错误折叠：Error 取 message，其他取 String，避免大对象污染日志 */
function foldError(error: unknown): string {
  if (error === undefined) return ''
  if (error instanceof Error) return error.message
  return String(error)
}

/**
 * 壳层 logger
 *
 * `scope` 是壳自身的固定标识（`Shell`），插件侧日志仍走 `context.logger`，
 * 两条通道在日志面板里可区分。
 */
export const logger = {
  debug: (message: string, error?: unknown) =>
    pushLog('debug', 'Shell', error === undefined ? message : `${message} :: ${foldError(error)}`),
  info: (message: string, error?: unknown) =>
    pushLog('info', 'Shell', error === undefined ? message : `${message} :: ${foldError(error)}`),
  warn: (message: string, error?: unknown) =>
    pushLog('warn', 'Shell', error === undefined ? message : `${message} :: ${foldError(error)}`),
  error: (message: string, error?: unknown) =>
    pushLog('error', 'Shell', error === undefined ? message : `${message} :: ${foldError(error)}`),
} satisfies Record<Level, (message: string, error?: unknown) => void>

export type { Level }