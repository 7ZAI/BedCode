/**
 * 终端域测试替身（票 15：终端域测试随源码迁入插件）
 *
 * - `installFakeHost()`：注入域宿主服务替身（t=回显 key / invoke 可编程并可断言 /
 *   内存 storage / 记录 toast 与日志 / 可注入 events 触发）。`host.ts` 的降级实现
 *   对纯逻辑测试已够用，需要断言调用面时才注入。
 * - `installMockMobileApi()`：安装 SDK 共享运行时替身（`window.__BEDCODE_SHARED__.mobileApi`，
 *   即 `getMobileApi()` 的读取源）。
 */

import { ref, type Ref } from 'vue'
import { initTerminalHost, type TerminalHostServices } from '../host'

// ==================== host 服务替身 ====================

export interface FakeHost {
  services: TerminalHostServices
  /** 记录到的插件命令调用（invoke） */
  calls: { command: string; args?: any }[]
  /** 记录到的 toast */
  toasts: { type: string; message: string }[]
  /** 记录到的日志 */
  logs: { level: string; message: string }[]
  /** 注册命令响应（值或函数；缺省 resolve undefined） */
  respond(command: string, result: any | ((args?: any) => any)): void
  /** 触发 host.events 订阅（store 的 terminal-state / terminal-resync 注入点） */
  emitEvent(event: string, payload: any): void
  /** 清空记录（同一用例内分段断言用） */
  reset(): void
}

export function createFakeHost(): FakeHost {
  const calls: FakeHost['calls'] = []
  const toasts: FakeHost['toasts'] = []
  const logs: FakeHost['logs'] = []
  const responders = new Map<string, any | ((args?: any) => any)>()
  const listeners = new Map<string, Set<(...a: any[]) => void>>()
  const memory = new Map<string, any>()

  const pushLog =
    (level: string) =>
    (...args: any[]): void => {
      logs.push({ level, message: args.map((a) => (a instanceof Error ? a.message : String(a))).join(' ') })
    }

  const services: TerminalHostServices = {
    t: (key) => key,
    invoke: async (command, args) => {
      calls.push({ command, args })
      const r = responders.get(command)
      if (typeof r === 'function') return r(args)
      return r
    },
    storage: {
      get: async (k) => memory.get(k),
      set: async (k, v) => {
        memory.set(k, v)
      },
      delete: async (k) => {
        memory.delete(k)
      },
    },
    events: {
      on: (event, handler) => {
        let set = listeners.get(event)
        if (!set) {
          set = new Set()
          listeners.set(event, set)
        }
        set.add(handler)
        return {
          dispose() {
            set!.delete(handler)
          },
        }
      },
    },
    logger: {
      log: pushLog('info'),
      debug: pushLog('debug'),
      info: pushLog('info'),
      warn: pushLog('warn'),
      error: pushLog('error'),
    },
    toast: {
      success: (m) => toasts.push({ type: 'success', message: m }),
      error: (m) => toasts.push({ type: 'error', message: m }),
      info: (m) => toasts.push({ type: 'info', message: m }),
      warning: (m) => toasts.push({ type: 'warning', message: m }),
    },
  }

  return {
    services,
    calls,
    toasts,
    logs,
    respond(command, result) {
      responders.set(command, result)
    },
    emitEvent(event, payload) {
      const set = listeners.get(event)
      if (set) for (const fn of [...set]) fn(payload)
    },
    reset() {
      calls.length = 0
      toasts.length = 0
      logs.length = 0
    },
  }
}

/** 注入替身并返回句柄（beforeEach 调用） */
export function installFakeHost(): FakeHost {
  const fake = createFakeHost()
  initTerminalHost(fake.services)
  return fake
}

/** 卸载替身（afterEach 调用；回到 host.ts 降级实现） */
export function uninstallFakeHost(): void {
  initTerminalHost(null)
}

// ==================== mobileApi（SDK 共享运行时替身） ====================

export interface MockMobileApiOptions {
  activeSessionId?: string | null
  isConnected?: boolean
  mockSessionId?: string | null
  httpRequest?: (path: string, options?: any) => Promise<any>
  openTerminalStream?: (
    sessionId: string,
    onBytes: (bytes: Uint8Array) => void,
  ) => Promise<{ dispose(): void }>
}

export interface MockMobileApi extends Record<string, any> {
  activeSessionId: Ref<string | null>
  activeSessions: Ref<any[]>
  sessionConfigs: Ref<any[]>
  isConnected: Ref<boolean>
  isDark: Ref<boolean>
  mobileSettings: Ref<Record<string, any>>
}

/** 安装 SDK 共享运行时替身（`getMobileApi()` 读取源） */
export function installMockMobileApi(options: MockMobileApiOptions = {}): MockMobileApi {
  const api: MockMobileApi = {
    activeSessionId: ref(options.activeSessionId ?? null),
    activeSessions: ref([]),
    sessionConfigs: ref([]),
    isConnected: ref(options.isConnected ?? false),
    isDark: ref(false),
    mobileSettings: ref({ vibrate: false }),
    httpRequest: options.httpRequest ?? (async () => ({ code: 0, message: 'ok' })),
    loadActiveSessions: async () => {},
    loadSessionConfigs: async () => {},
    openTerminalStream: options.openTerminalStream ?? (async () => ({ dispose() {} })),
    onSessionEvent: () => ({ dispose() {} }),
    mockSessionId: options.mockSessionId ?? null,
  }
  const shared = ((window as any).__BEDCODE_SHARED__ ??= {})
  shared.mobileApi = api
  shared.i18n = shared.i18n ?? { global: { t: (key: string) => key } }
  return api
}

/** 拆除共享运行时替身（afterEach） */
export function uninstallMockMobileApi(): void {
  const shared = (window as any).__BEDCODE_SHARED__
  if (shared) {
    delete shared.mobileApi
  }
}
