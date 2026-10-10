/**
 * 插件重新激活循环测试（loader 路径）
 *
 * 复现用户反馈：enable→disable→enable 后应用入口不重现。
 * 协作：真实 pluginLoader（activate/deactivate/loadFrontend）+ 真实 registry
 * + 可成功动态导入的 mock 前端模块（fixtures/mockLifecyclePlugin）。
 *
 * 关键：loader 的 activate 把 pluginActivate（后端 WASM）与 loadFrontend
 * （前端注册入口）放同一 try — 验证再激活时 module.activate 是否被重新调用、
 * 运行面是否重新落到壳注册表。
 *
 * 票 2026-10-10 批次 C2：断言面由 `pluginRegistry.toolboxViews`（旧嵌入扩展点，
 * 已退役）改为 `shellRegistry` 的 `contributions.surface`——当前唯一运行面形态。
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { PluginInfo } from '@/plugin/types'
import { flushAsync, resetLocalStorage, clearEventHandlers } from './helpers'

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
  // 动态 import 指向可解析的 mock 模块别名路径（成功导入，区别于 plugin-flow 的失败路径）
  convertFileSrc: () => '@/__tests__/integration/fixtures/mockLifecyclePlugin',
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: any[]) => mockListen(...args),
}))

// ==================== 测试基建 ====================

function makePluginInfo(overrides: Partial<PluginInfo> = {}): PluginInfo {
  return {
    id: 'mock-plugin',
    name: 'Mock Plugin',
    version: '1.0.0',
    description: '',
    author: 'test',
    main: 'index.js',
    pluginType: 'frontend',
    // 覆盖 context.ui.registerRoute（运行面与壳设置入口免权限）
    permissions: ['ui:route'],
    state: 'Loaded',
    contributes: {},
    source: 'builtin',
    extensionPath: '/plugins/mock-plugin',
    sizeBytes: 0,
    ...overrides,
  }
}

// mock 宿主共享运行时：registerRoute 经 getSharedModule('router') 取此 router
const mockRouterRemove = vi.fn()
const mockRouterAddRoute = vi.fn(() => mockRouterRemove)

beforeEach(() => {
  vi.clearAllMocks()
  resetLocalStorage()
  ;(globalThis as any).__BEDCODE_SHARED__ = { router: { addRoute: mockRouterAddRoute } }
})

afterEach(() => {
  delete (globalThis as any).__BEDCODE_SHARED__
})

afterEach(() => {
  for (const k of Object.keys(eventHandlers)) delete eventHandlers[k]
})

describe('插件重新激活循环（loader 路径）', () => {
  it('disable→enable 后运行面重新注册到壳注册表', async () => {
    const info = makePluginInfo()
    mockInvoke.mockImplementation((cmd: string) => {
      switch (cmd) {
        case 'plugin_list_loaded':
          return Promise.resolve([info])
        case 'plugin_is_enabled':
          return Promise.resolve(true)
        case 'plugin_get_info':
          return Promise.resolve(info)
        case 'plugin_activate':
        case 'plugin_deactivate':
        case 'plugin_mark_error':
          return Promise.resolve(undefined)
        default:
          return Promise.resolve(undefined)
      }
    })

    // 单次 resetModules 后 loader 与 registry 共享同一模块缓存（同一单例）
    vi.resetModules()
    const { pluginLoader } = await import('@/plugin/loader')
    const { getShellRegistry } = await import('@/shell/registry')
    const { toShellApp, PLUGIN_APP_SOURCE_ID } = await import('@/shell/adapters/pluginAppSource')
    const registry = getShellRegistry()
    // 生产顺序：壳先 list() 把应用写进注册表，之后激活才有运行面可挂
    registry.upsertApps(PLUGIN_APP_SOURCE_ID, [toShellApp(info)])
    const mockModule = await import('@/__tests__/integration/fixtures/mockLifecyclePlugin')
    mockModule._resetCounts()

    // 1. 首次激活：module.activate 被调用，入口落到 registry
    await pluginLoader.activate('mock-plugin')
    await flushAsync(3)
    expect(mockModule.activateCallCount).toBe(1)
    expect(registry.getApp('mock-plugin')?.contributions.surface).toBeTruthy()

    // 2. 停用：clearPlugin 摘除入口
    await pluginLoader.deactivate('mock-plugin')
    await flushAsync(3)
    expect(registry.getApp('mock-plugin')?.contributions.surface).toBeUndefined()

    // 3. 重新激活：module.activate 应再次被调用，入口应重新注册
    await pluginLoader.activate('mock-plugin')
    await flushAsync(3)
    expect(mockModule.activateCallCount).toBe(2)
    expect(registry.getApp('mock-plugin')?.contributions.surface).toBeTruthy()
  })
})
