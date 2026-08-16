/**
 * 集成测试公共工具（非测试文件，不被 vitest include 匹配）
 *
 * 与桌面端 `bedcode-desktop/src/__tests__/integration/*` 的模式对齐：
 * mock 三连（@tauri-apps/api/core + /event + /plugin-http + vue-sonner +
 * plugin-os）按桌面端惯例写在每个测试文件顶部（vi.mock 是 per-file 的），
 * 本文件只提供跨文件共享的纯工具函数。
 */

import { vi } from 'vitest'

/**
 * 推进异步链
 *
 * mock invoke / listen 返回的 Promise 链是纯微任务（无 setTimeout），
 * setTimeout(0) 只在微任务队列排空后触发，一次调用即可全部推进；
 * fake timers 下 setTimeout 被伪造，用 advanceTimersByTimeAsync(0) 等价推进。
 *
 * @param times - flush 轮数（多层链需要多轮时增加）
 */
export async function flushAsync(times = 2): Promise<void> {
  for (let i = 0; i < times; i++) {
    if (vi.isFakeTimers()) {
      await vi.advanceTimersByTimeAsync(0)
    } else {
      await new Promise((r) => setTimeout(r, 0))
    }
  }
}

/**
 * 重新加载被测模块（fresh 模块级单例）
 *
 * useMobileConnection 是模块级单例（init() 在模块加载时立即执行、模块级
 * ref 状态跨用例残留），测试间必须 vi.resetModules() 后重新 import 才能
 * 得到干净状态。vi.mock 注册不受 reset 影响（mock 工厂闭包引用的 hoisted
 * mock 实例保留），仅被测模块的模块缓存被清除。
 */
export async function loadFreshModule<T>(path: string): Promise<T> {
  vi.resetModules()
  return await import(path)
}

/** 清空 localStorage（happy-dom 原生实现） */
export function resetLocalStorage(): void {
  localStorage.clear()
}

/**
 * 清空事件捕获器（重新加载被测模块前必须调用）
 *
 * 模块级单例重新加载（vi.resetModules）时旧模块的监听器不会自动 unlisten
 * （模块卸载无清理钩子），若不清空，emit 会同时触发旧模块与现模块的 handler，
 * 造成 invoke 双调用等断言污染。
 */
export function clearEventHandlers(
  eventHandlers: Record<string, Array<(payload: unknown) => void>>,
): void {
  for (const k of Object.keys(eventHandlers)) delete eventHandlers[k]
}
