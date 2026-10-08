/**
 * 终端域宿主接入点（票 15：终端 UI 域下沉）
 *
 * 域内唯一与宿主 PluginContext 耦合的位置：
 * - 命令面：`invokeTerminal` → context.commands.execute（插件命令 + dev-shell 前端 handler 回落）
 * - 文案：`t` → context.i18n.t（键相对插件命名空间，如 'terminal.title'）
 * - 存储：`storage` → context.storage（插件私有 KV）
 * - 日志：`logger` → context.logger（写入宿主 tracing；变参在适配层拼接为单参）
 * - 轻提示：`toast` → context.dialogs.showToast
 *
 * 激活时由 `activate.ts` 调用 `initTerminalHost(createTerminalHostServices(ctx))`；
 * 未初始化（单测 / 纯工具模块）时走 fallback：文案回显 key、日志落 console、
 * 存储为内存 Map、命令 reject——保证纯逻辑模块可独立加载。
 * 其余宿主机制（mobileApi / 主题 / 会话事件）经 SDK `getMobileApi()` 直接消费。
 */
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'

export interface TerminalHostServices {
  t: (key: string, params?: Record<string, unknown>) => string
  invoke: (command: string, args?: any) => Promise<any>
  storage: {
    get<T = any>(key: string): Promise<T | undefined>
    set(key: string, value: any): Promise<void>
    delete(key: string): Promise<void>
  }
  /** 插件事件订阅（context.events：内存总线 + Tauri 事件桥接） */
  events: {
    on(event: string, handler: (...args: any[]) => void): { dispose(): void }
  }
  logger: {
    /** 一般日志（对齐宿主 frontendLogger.log；落 info 级） */
    log(...args: any[]): void
    debug(...args: any[]): void
    info(...args: any[]): void
    warn(...args: any[]): void
    error(...args: any[]): void
  }
  toast: {
    success(message: string): void
    error(message: string): void
    info(message: string): void
    warning(message: string): void
  }
}

/** 变参日志参数 → 单行文本（SDK logger 为单参接口） */
function formatArgs(args: any[]): string {
  return args
    .map((a) => {
      if (a instanceof Error) return a.message
      if (typeof a === 'string') return a
      try {
        return JSON.stringify(a)
      } catch {
        return String(a)
      }
    })
    .join(' ')
}

/** 从 PluginContext 构造域内服务（activate / 真机路径） */
export function createTerminalHostServices(ctx: PluginContext): TerminalHostServices {
  return {
    t: (key, params) => ctx.i18n.t(key, params),
    invoke: (command, args) => ctx.commands.execute(command, args),
    storage: {
      get: (key) => ctx.storage.get(key),
      set: (key, value) => ctx.storage.set(key, value),
      delete: (key) => ctx.storage.delete(key),
    },
    events: {
      on: (event, handler) => ctx.events.on(event, handler),
    },
    logger: {
      log: (...a) => ctx.logger.info(formatArgs(a)),
      debug: (...a) => ctx.logger.debug(formatArgs(a)),
      info: (...a) => ctx.logger.info(formatArgs(a)),
      warn: (...a) => ctx.logger.warn(formatArgs(a)),
      error: (...a) => ctx.logger.error(formatArgs(a)),
    },
    toast: {
      success: (m) => ctx.dialogs.showToast(m, 'success'),
      error: (m) => ctx.dialogs.showToast(m, 'error'),
      info: (m) => ctx.dialogs.showToast(m, 'info'),
      warning: (m) => ctx.dialogs.showToast(m, 'warning'),
    },
  }
}

let services: TerminalHostServices | null = null
let fallbackServices: TerminalHostServices | null = null

/** 注入域服务（activate 调用；deactivate 可显式置空） */
export function initTerminalHost(next: TerminalHostServices | null): void {
  services = next
}

/** 未初始化时的降级实现（单测 / 纯工具场景；命令面显式 reject，不静默） */
function fallback(): TerminalHostServices {
  const memory = new Map<string, any>()
  const consoleLog =
    (...args: any[]) =>
    (...logArgs: any[]) =>
      console.log(...args, ...logArgs)
  void consoleLog
  return {
    t: (key) => key,
    invoke: (command) =>
      Promise.reject(new Error(`[terminal-session] host not initialized, command rejected: ${command}`)),
    storage: {
      get: (key) => Promise.resolve(memory.get(key)),
      set: (key, value) => {
        memory.set(key, value)
        return Promise.resolve()
      },
      delete: (key) => {
        memory.delete(key)
        return Promise.resolve()
      },
    },
    // 未初始化时的事件面：内存实现（同事件多监听者），供单测订阅/触发
    events: (() => {
      const listeners = new Map<string, Set<(...args: any[]) => void>>()
      return {
        on(event: string, handler: (...args: any[]) => void) {
          let set = listeners.get(event)
          if (!set) {
            set = new Set()
            listeners.set(event, set)
          }
          set.add(handler)
          return {
            dispose() {
              set!.delete(handler)
              if (set!.size === 0) listeners.delete(event)
            },
          }
        },
      }
    })(),
    logger: {
      log: (...a) => console.log('[terminal-session]', ...a),
      debug: (...a) => console.debug('[terminal-session]', ...a),
      info: (...a) => console.info('[terminal-session]', ...a),
      warn: (...a) => console.warn('[terminal-session]', ...a),
      error: (...a) => console.error('[terminal-session]', ...a),
    },
    toast: {
      success: () => {},
      error: () => {},
      info: () => {},
      warning: () => {},
    },
  }
}

/** 当前域服务（未初始化时返回降级实现） */
export function hostServices(): TerminalHostServices {
  if (!services) {
    fallbackServices ??= fallback()
    return fallbackServices
  }
  return services
}

// ==================== 域内便捷导出（调用点零样板） ====================

/** 域日志（变参；写入宿主 tracing） */
export const logger = {
  log: (...args: any[]): void => hostServices().logger.log(...args),
  debug: (...args: any[]): void => hostServices().logger.debug(...args),
  info: (...args: any[]): void => hostServices().logger.info(...args),
  warn: (...args: any[]): void => hostServices().logger.warn(...args),
  error: (...args: any[]): void => hostServices().logger.error(...args),
}

/** 域文案（键相对插件命名空间，如 'terminal.title'） */
export function t(key: string, params?: Record<string, unknown>): string {
  return hostServices().t(key, params)
}

/** 插件命令执行（context.commands.execute：本地 handler → WASM 命令桥） */
export function invokeTerminal(command: string, args?: any): Promise<any> {
  return hostServices().invoke(command, args)
}

/** 插件事件订阅（context.events） */
export const events = {
  on: (event: string, handler: (...args: any[]) => void): { dispose(): void } =>
    hostServices().events.on(event, handler),
}

/** 插件私有存储（KV） */
export const storage = {
  get: <T = any>(key: string): Promise<T | undefined> => hostServices().storage.get<T>(key),
  set: (key: string, value: any): Promise<void> => hostServices().storage.set(key, value),
  delete: (key: string): Promise<void> => hostServices().storage.delete(key),
}

/** 宿主轻提示 */
export const toast = {
  success: (message: string): void => hostServices().toast.success(message),
  error: (message: string): void => hostServices().toast.error(message),
  info: (message: string): void => hostServices().toast.info(message),
  warning: (message: string): void => hostServices().toast.warning(message),
}
