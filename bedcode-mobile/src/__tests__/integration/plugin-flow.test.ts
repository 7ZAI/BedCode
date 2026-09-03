/**
 * 插件流组合集成测试（L2 场景 5）
 *
 * 协作实体：真实 pluginLoader（模块级单例，清单过滤 / 扫描轮询 / 加载降级） +
 * plugin registry（扩展点注册清理） + pluginCmds（invoke 命令层）。
 *
 * 测试 seam：mock invoke（plugin_list_loaded / plugin_is_enabled /
 * plugin_mark_error）+ convertFileSrc 返回不可解析路径（前端模块动态 import
 * 必然失败 → 优雅降级路径可测）。
 *
 * 覆盖：loadAll 清单过滤（rust-only 跳过 / 未启用跳过）；空清单扫描轮询
 * 补载（fake timers 压缩 250ms 轮询）；前端模块加载失败 → plugin_mark_error
 * + registry 清理（半激活状态摘除）。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import type { PluginInfo } from '@/plugin/types'
import { flushAsync, loadFreshModule, resetLocalStorage, clearEventHandlers } from './helpers'

// ==================== mock Tauri 边界 ====================

const mockInvoke = vi.fn()
const eventHandlers: Record<string, Array<(payload: unknown) => void>> = {}
const mockListen = vi.fn((event: string, handler: (payload: unknown) => void) => {
  if (!eventHandlers[event]) eventHandlers[event] = []
  eventHandlers[event].push(handler)
  return Promise.resolve(() => {
    eventHandlers[event] = (eventHandlers[event] || []).filter((h) => h !== handler)
  })
})

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
  // 插件前端模块经 asset protocol 直读；测试返回不可解析路径使动态 import 失败
  convertFileSrc: () => 'invalid-plugin-path.js',
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: any[]) => mockListen(...args),
}))

// ==================== 测试基建 ====================

/** invoke 返回形状的 PluginInfo（非线协议 DTO，测试内构造） */
function makePluginInfo(overrides: Partial<PluginInfo> = {}): PluginInfo {
  return {
    id: 'plugin-1',
    name: 'Plugin 1',
    version: '1.0.0',
    description: '',
    author: 'test',
    main: 'index.js',
    pluginType: 'frontend',
    permissions: [],
    state: 'Loaded',
    contributes: {},
    source: 'builtin',
    extensionPath: '/plugins/plugin-1',
    sizeBytes: 1024,
    ...overrides,
  }
}

function invokeCalls(cmd: string): unknown[][] {
  return mockInvoke.mock.calls.filter(([c]) => c === cmd).map((call) => call.slice(1))
}

/** 默认 invoke 分发：plugin_list_loaded 空列表 + 其他安全值 */
function installInvokeMock(manifests: () => PluginInfo[]) {
  mockInvoke.mockImplementation((cmd: string, args: Record<string, unknown>) => {
    switch (cmd) {
      case 'plugin_list_loaded':
        return Promise.resolve(manifests())
      case 'plugin_is_enabled':
        return Promise.resolve(false)
      case 'plugin_mark_error':
      case 'plugin_activate':
      case 'plugin_deactivate':
        return Promise.resolve(undefined)
      default:
        return Promise.resolve(undefined)
    }
  })
}

beforeEach(() => {
  vi.clearAllMocks()
  resetLocalStorage()
})

afterEach(() => {
  for (const k of Object.keys(eventHandlers)) delete eventHandlers[k]
  vi.useRealTimers()
})

describe('插件流：pluginLoader × registry', () => {
  it('loadAll 清单过滤：rust-only 插件跳过、未启用前端插件不加载', async () => {
    const manifests = [
      makePluginInfo({ id: 'rust-only', pluginType: 'rust' }),
      makePluginInfo({ id: 'frontend-disabled', pluginType: 'frontend' }),
    ]
    installInvokeMock(() => manifests)
    const { pluginLoader } = await loadFreshModule<typeof import('@/plugin/loader')>('@/plugin/loader')

    await pluginLoader.loadAll()
    await flushAsync()

    // rust-only：不查询启用状态、不加载前端模块
    // frontend 未启用：只查询启用状态，跳过加载
    expect(invokeCalls('plugin_is_enabled')).toEqual([[{ pluginId: 'frontend-disabled' }]])
    expect(invokeCalls('plugin_mark_error')).toHaveLength(0)
    expect(pluginLoader.getActivePlugin('frontend-disabled')).toBeUndefined()
  })

  it('空清单扫描轮询：首次空列表 → 250ms 轮询补载 → 第二次非空即加载', async () => {
    vi.useFakeTimers()
    const manifests: PluginInfo[] = []
    installInvokeMock(() => manifests)
    const { pluginLoader } = await loadFreshModule<typeof import('@/plugin/loader')>('@/plugin/loader')

    // 首次查询：扫描未完成，空列表 → 调度轮询
    const loadPromise = pluginLoader.loadAll()
    await vi.advanceTimersByTimeAsync(0)
    await loadPromise
    expect(invokeCalls('plugin_list_loaded')).toHaveLength(1)

    // 扫描完成（第二次查询返回 rust-only 插件）→ 补载完成，不再轮询
    manifests.push(makePluginInfo({ id: 'rust-only', pluginType: 'rust' }))
    await vi.advanceTimersByTimeAsync(250)
    await flushAsync()

    expect(invokeCalls('plugin_list_loaded')).toHaveLength(2)
    // rust-only 补载不触发前端加载
    expect(invokeCalls('plugin_is_enabled')).toHaveLength(0)
  })

  it('前端模块加载失败：plugin_mark_error + registry 清理（半激活状态摘除）', async () => {
    const manifests = [makePluginInfo({ id: 'frontend-ok', pluginType: 'frontend' })]
    installInvokeMock(() => manifests)
    mockInvoke.mockImplementation((cmd: string) => {
      switch (cmd) {
        case 'plugin_list_loaded':
          return Promise.resolve(manifests)
        case 'plugin_is_enabled':
          return Promise.resolve(true)
        case 'plugin_mark_error':
          return Promise.resolve(undefined)
        default:
          return Promise.resolve(undefined)
      }
    })
    const { pluginLoader } = await loadFreshModule<typeof import('@/plugin/loader')>('@/plugin/loader')
    const { getPluginRegistry } = await loadFreshModule<typeof import('@/plugin/registry')>('@/plugin/registry')

    await pluginLoader.loadAll()
    await flushAsync(4)

    // 动态 import 失败（convertFileSrc 返回不可解析路径）→ 标记错误 + 无半激活残留
    expect(invokeCalls('plugin_mark_error')).toHaveLength(1)
    expect(invokeCalls('plugin_mark_error')[0]).toMatchObject([{ pluginId: 'frontend-ok' }])
    expect(pluginLoader.getActivePlugin('frontend-ok')).toBeUndefined()
    expect(getPluginRegistry().getContext('frontend-ok')).toBeUndefined()
  })
})
