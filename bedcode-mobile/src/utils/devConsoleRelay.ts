/**
 * 前端 console 日志转发（仅 debug 构建生效）——AI agent 排查前端问题的通道
 *
 * debug 构建下覆盖 console.log/debug/info/warn/error，把输出批量经 IPC
 * 转发到 Rust 命令 `report_frontend_log`（仅 debug 注册），写进 tracing
 * （target=`frontend`）→ Android 自动转发 logcat；AI agent 通过
 * `pnpm run tauri:android:dev:log` 落盘的 `.dev-logs/android-dev.*.log`
 * 中 grep `frontend` 即可读取。release 构建 Rust 侧不注册该命令，
 * 此处也按 envDev=false 直接 no-op，两端双保险。
 *
 * 设计要点：
 * - 批量攒批（条数阈值 + 定时）再发送，避免高频日志刷爆 IPC
 * - 转发前先调原始 console 方法，WebView DevTools 行为不受影响
 * - 转发失败静默（打一条不带转发的原始警告），防止递归输出
 */

import { invoke } from '@tauri-apps/api/core'

/** 转发给后端的单条日志记录（level 后端已归一化，见 Rust normalized_level） */
export interface ConsoleRelayEntry {
  level: string
  message: string
}

/** 攒批条数阈值，达到后立即发送 */
const FLUSH_THRESHOLD = 50
/** 定时发送间隔（毫秒），保证低频日志也能及时落盘 */
const FLUSH_INTERVAL_MS = 400

let installed = false

/** 将单个 console 参数序列化为文本：Error 取 stack，对象尝试 JSON，其余 String 化 */
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
      //（避免退回 [object Object] 丢失信息）
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

/** 将 console 多参数拼接为单条文本（落盘后可直接 grep） */
export function formatConsoleArgs(args: unknown[]): string {
  return args.map(stringifyArg).join(' ')
}

/**
 * 安装 debug 构建下的 console 转发。
 *
 * @param envDev 是否处于 dev 构建（默认读 import.meta.env.DEV，测试可注入）
 * @returns detach 函数：恢复原始 console 方法并停止定时发送
 */
export function installDevConsoleRelay(envDev: boolean = import.meta.env.DEV): () => void {
  // 幂等：vite HMR 重跑 main.ts 时避免重复覆盖/重复定时器
  if (!envDev || installed) return () => {}

  const original = {
    log: console.log,
    debug: console.debug,
    info: console.info,
    warn: console.warn,
    error: console.error,
  }

  let entries: ConsoleRelayEntry[] = []
  let reportedFailure = false

  /** 发送攒批日志到 Rust（失败静默，仅 warn 一次，避免转发本身引发递归日志） */
  const flush = () => {
    if (entries.length === 0) return
    const batch = entries
    entries = []
    invoke('report_frontend_log', { logs: batch }).catch(() => {
      if (!reportedFailure) {
        reportedFailure = true
        original.warn.call(console, '[devConsoleRelay] 日志转发失败（仅 debug 构建生效，release 无此命令）')
      }
    })
  }

  const makeRelay = (originalFn: (...args: unknown[]) => void, level: string) => {
    return (...args: unknown[]): void => {
      originalFn.apply(console, args)
      const message = formatConsoleArgs(args)
      if (!message) return
      entries.push({ level, message })
      if (entries.length >= FLUSH_THRESHOLD) flush()
    }
  }

  // console.log 语义最接近 info，但对应后端 debug 级（logcat 过滤通常从 debug 起）
  console.log = makeRelay(original.log, 'debug')
  console.debug = makeRelay(original.debug, 'debug')
  console.info = makeRelay(original.info, 'info')
  console.warn = makeRelay(original.warn, 'warn')
  console.error = makeRelay(original.error, 'error')

  const timer = window.setInterval(flush, FLUSH_INTERVAL_MS)
  installed = true

  return () => {
    window.clearInterval(timer)
    console.log = original.log
    console.debug = original.debug
    console.info = original.info
    console.warn = original.warn
    console.error = original.error
    entries = []
    installed = false
  }
}