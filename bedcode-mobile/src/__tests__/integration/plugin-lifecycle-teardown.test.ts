/**
 * 插件生命周期对称拆解测试（spec §5 主 seam / issues 03 + 04）
 *
 * 被测不变量：「前端注册表为空 ⇔ 后端非存活」，重启用从干净状态重启。
 *
 * - 连续 activate/deactivate 交替 N 次：registry.toolboxViews 该插件入口 0/1
 *   严格跟随，无残留旧引用、无重复（issue 03 验收）；
 * - 前端模块加载失败：plugin_deactivate 必须先于 plugin_mark_error（后端
 *   deactivate 仅对 Activated/Degraded 生效，顺序颠倒实例将无法拆解），
 *   plugins Map 与注册表无残留（issue 03 验收）；
 * - deactivate 在前端模块未加载（plugins Map 无记录）时仍通知后端停用且幂等
 *   （issue 03：修复「deactivate 早退不通知后端」的不对称）；
 * - clearPluginEvents 逐条 dispose（含 tauriUnlisten 反注册）且幂等；
 *   dispose 先于 Tauri listen 建立时，listen resolve 后立即反注册（issue 04）。
 *
 * 测试 seam：mock @tauri-apps/api/core（invoke 命令层 + convertFileSrc 按插件
 * 目录分流成功/失败导入路径）与 @tauri-apps/api/event（listen 捕获 unlisten）；
 * 动态 import 指向 fixtures/mockLifecyclePlugin（注册 toolbox 入口的可复用
 * mock 模块），不触及 Vue 渲染细节。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { makePluginInfo } from '@/__tests__/fixtures'
import { flushAsync, loadFreshModule, resetLocalStorage, clearEventHandlers } from './helpers'

// ==================== mock Tauri 边界 ====================

const mockInvoke = vi.fn()
const eventHandlers: Record<string, Array<(payload: unknown) => void>> = {}
const unlistenSpies: Array<ReturnType<typeof vi.fn>> = []
const mockListen = vi.fn((event: string, handler: (payload: unknown) => void) => {
  ;(eventHandlers[event] ||= []).push(handler)
  const unlisten = vi.fn(() => {
    eventHandlers[event] = (eventHandlers[event] || []).filter((h) => h !== handler)
  })
  unlistenSpies.push(unlisten)
  return Promise.resolve(unlisten)
})

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
  // 按插件目录分流导入路径：mock-ok 指向可解析 mock 模块（成功路径），
  // 其余返回不可解析路径（动态 import 必然失败 → loadFrontend 失败路径）
  convertFileSrc: (p: string) =>
    p.startsWith('/plugins/mock-ok')
      ? '@/__tests__/integration/fixtures/mockLifecyclePlugin'
      : 'invalid-plugin-path.js',
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: any[]) => mockListen(...args),
}))

// ==================== 测试基建 ====================

/** mock 宿主共享运行时：registerRoute 经 getSharedModule('router') 取此 router */
const mockRouterAddRoute = vi.fn(() => vi.fn())

function invokeCalls(cmd: string): unknown[][] {
  return mockInvoke.mock.calls.filter(([c]) => c === cmd).map((call) => call.slice(1))
}

function installDefaultInvoke(): void {
  mockInvoke.mockImplementation(() => Promise.resolve(undefined))
}

/** 前端可加载插件的 manifest（wasm 类型 + ui 权限 + 可成功导入的 mock 模块路径） */
function makeFrontendInfo(overrides: { extensionPath?: string; state?: unknown } = {}) {
  return makePluginInfo({
    id: 'mock-plugin',
    name: 'Mock Plugin',
    version: '1.0.0',
    description: '',
    author: 'test',
    main: 'index.js',
    pluginType: 'wasm',
    permissions: ['ui:toolbox', 'ui:route', 'ui:settings'],
    state: { state: 'Activated' },
    contributes: {},
    source: 'builtin',
    extensionPath: '/plugins/mock-ok',
    sizeBytes: 0,
    installedAt: 0,
    ...overrides,
  } as Parameters<typeof makePluginInfo>[0])
}

beforeEach(() => {
  vi.clearAllMocks()
  unlistenSpies.length = 0
  resetLocalStorage()
  installDefaultInvoke()
  ;(globalThis as any).__BEDCODE_SHARED__ = { router: { addRoute: mockRouterAddRoute } }
})

afterEach(async () => {
  delete (globalThis as any).__BEDCODE_SHARED__
  for (const k of Object.keys(eventHandlers)) delete eventHandlers[k]
  vi.useRealTimers()
})

// ==================== issue 03：loader 对称拆解 ====================

describe('loader activate/deactivate 对称拆解', () => {
  it('连续 activate/deactivate 交替 3 次：入口 0/1 严格跟随，无残留旧引用、无重复', async () => {
    const info = makeFrontendInfo()
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'plugin_get_info') return Promise.resolve(info)
      return Promise.resolve(undefined)
    })

    // 单次 resetModules：loader 与 registry 共享同一模块缓存（同一单例）
    vi.resetModules()
    const { pluginLoader } = await import('@/plugin/loader')
    const { getPluginRegistry } = await import('@/plugin/registry')
    const mockModule = await import('@/__tests__/integration/fixtures/mockLifecyclePlugin')
    mockModule._resetCounts()

    const seenEntries: unknown[] = []
    for (let cycle = 0; cycle < 3; cycle++) {
      await pluginLoader.activate('mock-plugin')
      await flushAsync(3)
      const views = getPluginRegistry().toolboxViews.value
      expect(views, `cycle ${cycle}: enabled`).toHaveLength(1)
      expect(views[0].pluginId).toBe('mock-plugin')
      seenEntries.push(views[0])
      expect(pluginLoader.getActivePlugin('mock-plugin')).toBeDefined()
      expect(mockModule.activateCallCount).toBe(cycle + 1)

      await pluginLoader.deactivate('mock-plugin')
      await flushAsync(3)
      expect(getPluginRegistry().toolboxViews.value, `cycle ${cycle}: disabled`).toHaveLength(0)
      expect(pluginLoader.getActivePlugin('mock-plugin')).toBeUndefined()
    }

    // 每轮注册都是全新对象：无旧引用残留
    expect(new Set(seenEntries).size).toBe(3)
    // 后端命令对称：3 次激活 + 3 次停用
    expect(invokeCalls('plugin_activate')).toHaveLength(3)
    expect(invokeCalls('plugin_deactivate')).toHaveLength(3)
  })

  it('前端模块加载失败：plugin_deactivate 先于 plugin_mark_error，注册表与 Map 无残留', async () => {
    const info = makeFrontendInfo({ extensionPath: '/plugins/mock-broken' })
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'plugin_get_info') return Promise.resolve(info)
      return Promise.resolve(undefined)
    })

    vi.resetModules()
    const { pluginLoader } = await import('@/plugin/loader')
    const { getPluginRegistry } = await import('@/plugin/registry')

    // plugin_activate（默认 mock）成功 → loadFrontend 动态 import 失败
    await pluginLoader.activate('mock-plugin')
    await flushAsync(4)

    expect(invokeCalls('plugin_deactivate')).toHaveLength(1)
    expect(invokeCalls('plugin_mark_error')).toHaveLength(1)
    // 顺序关键：后端 deactivate 仅对 Activated/Degraded 生效，必须先于 markError
    const deactIdx = mockInvoke.mock.calls.findIndex(([c]) => c === 'plugin_deactivate')
    const markIdx = mockInvoke.mock.calls.findIndex(([c]) => c === 'plugin_mark_error')
    expect(deactIdx).toBeLessThan(markIdx)

    // 前端注册表为空 ⇔ 后端非存活：Map 无残留、context/入口已摘除
    expect(pluginLoader.getActivePlugin('mock-plugin')).toBeUndefined()
    expect(getPluginRegistry().getContext('mock-plugin')).toBeUndefined()
    expect(getPluginRegistry().toolboxViews.value).toHaveLength(0)
  })

  it('deactivate 在前端模块未加载时仍停用后端，且重复调用幂等', async () => {
    vi.resetModules()
    const { pluginLoader } = await import('@/plugin/loader')
    const { getPluginRegistry } = await import('@/plugin/registry')
    // 模拟激活期失败残留的 context（前端模块从未进入 plugins Map）
    getPluginRegistry().setContext('ghost-plugin', {} as never)

    await pluginLoader.deactivate('ghost-plugin')
    await pluginLoader.deactivate('ghost-plugin')
    await flushAsync(3)

    // 两次调用都通知后端（修复「plugins 无记录即早退」的不对称）
    expect(invokeCalls('plugin_deactivate')).toHaveLength(2)
    // 注册表残留被清、无副作用抛错
    expect(getPluginRegistry().getContext('ghost-plugin')).toBeUndefined()
    expect(pluginLoader.getActivePlugin('ghost-plugin')).toBeUndefined()
  })
})

// ==================== issue 04：clearPluginEvents 真正 dispose ====================

describe('clearPluginEvents 真正 dispose Tauri listener', () => {
  it('clearPluginEvents 逐条触发 unlisten、内存 handler 清空、重复调用幂等', async () => {
    const events = await loadFreshModule<typeof import('@/plugin/events')>('@/plugin/events')
    const handler = vi.fn()
    events.on('p-dispose', 'tick', handler)
    await flushAsync(3) // Tauri listen 建立

    events.clearPluginEvents('p-dispose')
    expect(unlistenSpies[0]).toHaveBeenCalledTimes(1)

    // 内存总线 handler 已清：emit 不再触达
    events.emit('tick', 'payload')
    expect(handler).not.toHaveBeenCalled()

    // 幂等：已 dispose 的不再触发
    events.clearPluginEvents('p-dispose')
    expect(unlistenSpies[0]).toHaveBeenCalledTimes(1)
  })

  it('dispose 先于 Tauri listen 建立时，listen resolve 后立即反注册', async () => {
    const events = await loadFreshModule<typeof import('@/plugin/events')>('@/plugin/events')
    const d = events.on('p-race', 'evt', () => {})
    d.dispose() // 此刻 listen 尚未 resolve（动态 import + listen 均为微任务）
    await flushAsync(3)

    // 竞态收敛：listen 建立后发现已 dispose → 立即反注册，不留僵尸 listener
    expect(unlistenSpies[0]).toHaveBeenCalledTimes(1)
  })

  it('clearPluginEvents 与 context._disposables 主路径双重清理不冲突', async () => {
    const events = await loadFreshModule<typeof import('@/plugin/events')>('@/plugin/events')
    const handler = vi.fn()
    const d = events.on('p-both', 'evt', handler)
    await flushAsync(3)

    d.dispose() // 主路径（loader._disposables）先行
    events.clearPluginEvents('p-both') // 次级防线兜底
    await flushAsync(2)

    expect(unlistenSpies[0]).toHaveBeenCalledTimes(1) // 仅一次
    events.emit('evt', 'x')
    expect(handler).not.toHaveBeenCalled()
  })
})
