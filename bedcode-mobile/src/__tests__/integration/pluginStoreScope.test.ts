/**
 * 应用运行期 store 作用域集成测试（真实 loader × 真实 shared-runtime × 真实插件模块）
 *
 * 复现用户反馈并钉住修复：**停止应用（含多任务上滑）后再打开，插件的 Pinia store
 * 必须回到初始 state**。协作实体：真实 pluginLoader（activate/deactivate/loadFrontend）
 * + 真实 shared-runtime（暴露作用域包装版 pinia）+ 可成功动态导入的 mock 插件模块
 * （fixtures/mockStorePlugin，模块顶层 defineStore）。
 *
 * 关键前提：第二次运行期命中的是**同一个模块对象**（ESM 缓存，模块求值只一次）。
 * 若只测「重新 import 后状态是新的」会绕过真正的缺陷——所以本测试显式断言
 * `secondModule === firstModule`。
 *
 * 契约来源：`.scratch/2026-10-14-mobile-app-run-store-scope/spec.md` §4 集成项。
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import type { PluginInfo } from '@/plugin/types'
import { flushAsync, resetLocalStorage } from './helpers'

// ==================== mock Tauri 边界 ====================

const mockInvoke = vi.fn()
const eventHandlers: Record<string, Array<(payload: unknown) => void>> = {}
const mockListen = vi.fn((event: string, handler: (payload: unknown) => void) => {
  ;(eventHandlers[event] ||= []).push(handler)
  return Promise.resolve(() => {
    eventHandlers[event] = (eventHandlers[event] || []).filter((h) => h !== handler)
  })
})

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
  // 动态 import 指向可解析的 mock 插件模块；id 含 broken 的插件给不可解析路径
  // （成功 / 失败两条装载路径在同一文件内可测，无需 doMock 切换）
  convertFileSrc: (path: string) =>
    path.includes('broken-plugin')
      ? 'invalid-plugin-path.js'
      : '@/__tests__/integration/fixtures/mockStorePlugin',
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: any[]) => mockListen(...args),
}))

// 作用域作废保持真实实现，仅加一层可断言的 spy（失败路径 / 停止路径都断言到调用）
vi.mock('@/plugin/app-run-scope', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/plugin/app-run-scope')>()
  return { ...actual, disposeAppRunScope: vi.fn(actual.disposeAppRunScope) }
})

// ==================== 测试基建 ====================

function makePluginInfo(overrides: Partial<PluginInfo> = {}): PluginInfo {
  return {
    id: 'scope-plugin',
    name: 'Scope Plugin',
    version: '1.0.0',
    description: '',
    author: 'test',
    main: 'index.js',
    pluginType: 'frontend',
    permissions: [],
    state: 'loaded',
    contributes: {},
    source: 'builtin',
    extensionPath: '/plugins/scope-plugin',
    sizeBytes: 0,
    ...overrides,
  }
}

beforeEach(() => {
  vi.clearAllMocks()
  resetLocalStorage()
  // 宿主 Pinia（与真实装配同源：initPluginSystem 传给 initSharedRuntime 的就是它）
  setActivePinia(createPinia())
})

afterEach(() => {
  delete (window as any).__BEDCODE_SHARED__
  for (const k of Object.keys(eventHandlers)) delete eventHandlers[k]
})

/** 装载真实 loader 与共享运行时（每个用例 fresh 单例，避免作用域表跨用例残留） */
async function loadRuntime() {
  vi.resetModules()
  const { initSharedRuntime } = await import('@/plugin/shared-runtime')
  await initSharedRuntime(null, createPinia(), null, null, null, null, null)
  const { pluginLoader } = await import('@/plugin/loader')
  const scopeModule = await import('@/plugin/app-run-scope')
  return { pluginLoader, scopeModule }
}

/** 让 plugin_get_info 返回给定清单，其余命令静默成功 */
function stubInvoke(info: PluginInfo): void {
  mockInvoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case 'plugin_get_info':
        return Promise.resolve(info)
      default:
        return Promise.resolve(undefined)
    }
  })
}

describe('插件运行期 store 作用域（loader 路径）', () => {
  it('停止应用后再次启动：命中模块缓存的同一模块，store 仍回到初始 state', async () => {
    const { pluginLoader } = await loadRuntime()
    const info = makePluginInfo()
    stubInvoke(info)

    // 1. 首次启动：装载前端模块并把 store 改成一个可辨识的值
    await pluginLoader.activate(info.id)
    await flushAsync(3)
    const firstModule = pluginLoader.getActivePlugin(info.id)!.module as any
    const firstStore = firstModule.useScopeProbeStore()
    firstStore.count = 42
    expect(firstStore.count).toBe(42)

    // 2. 停止应用（多任务上滑 / × / 详情「停用」同一条链路都是 loader.deactivate）
    await pluginLoader.deactivate(info.id)
    await flushAsync(3)
    expect(pluginLoader.getActivePlugin(info.id)).toBeUndefined()

    // 3. 再次点击启动：模块被 ESM 缓存（同一对象——旧状态复活的成因），
    //    但 store 必须来自新的运行期作用域
    await pluginLoader.activate(info.id)
    await flushAsync(3)
    const secondModule = pluginLoader.getActivePlugin(info.id)!.module as any
    expect(secondModule).toBe(firstModule)

    const secondStore = secondModule.useScopeProbeStore()
    expect(secondStore).not.toBe(firstStore)
    expect(secondStore.count).toBe(0)

    // 旧实例留在旧运行期：其 state 保留但不被任何 useStore() 取回（无跨运行期泄漏）
    expect(firstStore.count).toBe(42)
  })

  it('前端模块装载失败：不残留半截运行期作用域', async () => {
    const { pluginLoader, scopeModule } = await loadRuntime()
    const info = makePluginInfo({ id: 'broken-plugin', extensionPath: '/plugins/broken-plugin' })
    stubInvoke(info)

    await pluginLoader.activate(info.id)
    await flushAsync(3)

    // 装载失败路径也要作废（模块求值到一半可能已建作用域）；前端注册表不残留
    expect(scopeModule.disposeAppRunScope).toHaveBeenCalledWith('broken-plugin')
    expect(pluginLoader.getActivePlugin('broken-plugin')).toBeUndefined()
  })
})
