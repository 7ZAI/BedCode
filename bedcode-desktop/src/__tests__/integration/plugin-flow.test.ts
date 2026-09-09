/**
 * 插件流组合集成测试（L2 场景 4）
 *
 * 协作实体：pluginLoader（真实单例加载器） + usePluginManager（composable，
 * 经 PluginsView 挂载真实执行） + PluginsView（插件列表组件）。
 *
 * 覆盖用户路径：清单加载 → 分区渲染（已启用/未启用）→ 停用联动（行迁移）→
 * 启用联动（后端激活通知 + 前端模块加载失败恢复）。
 *
 * 测试 seam（与 useServer.test.ts 同模式）：
 * - 只 mock @tauri-apps/api 边界：core.invoke + convertFileSrc（asset protocol
 *   路径转换，测试环境无真实 asset 服务器）+ event.listen（dev-reload 热重载
 *   监听）；Pinia / router / i18n / composables / loader / 组件逻辑全部真实执行
 * - fixture 数据取自工厂（makePluginInfo）
 *
 * 环境限制说明：
 * - 启用路径中 pluginLoader.loadInline 的「动态 import 插件入口」在 vitest 内
 *   必然失败（asset:// 协议无服务器可解析）——断言到「后端激活通知已发 +
 *   plugin_mark_error 恢复」为止，前端模块加载属 Tauri 资产服务器能力，
 *   非前端逻辑；loader 的失败恢复路径（mark_error + 列表重载）正是被测行为
 * - fake timers：togglePlugin 有 500ms 最小遮罩时长，用 fake timers 推进；
 *   期间 togglingId 保持非空 → 按钮禁用态可观测（联动断言）
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { logger } from '@/utils/frontendLogger'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createRouter, createMemoryHistory } from 'vue-router'
import i18n from '@/locales'
import PluginsView from '@/views/PluginsView.vue'
import { pluginLoader } from '@/plugin/loader'
import { makePluginInfo, makeDegradedPluginInfo } from '@/__tests__/fixtures/index'

// ==================== mock Tauri 边界 ====================

const mockInvoke = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
  // asset protocol 路径转换：loader 用它构造插件入口 URL（测试环境无 asset 服务器，
  // 返回合法协议串即可让 loader 走到动态 import 步骤）
  convertFileSrc: (p: string) => `asset://localhost/${p}`,
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

// ==================== 测试基建 ====================

/** 后端插件清单（可变：mount 后改写以模拟后端状态变化） */
let backendPlugins: ReturnType<typeof makePluginInfo>[]

function installInvokeMock() {
  mockInvoke.mockImplementation((cmd: string, args?: any) => {
    switch (cmd) {
      case 'plugin_list_loaded':
        return Promise.resolve([...backendPlugins])
      case 'plugin_get_info':
        return Promise.resolve(backendPlugins.find((p) => p.id === args?.pluginId) ?? null)
      case 'plugin_activate':
        return Promise.resolve(undefined)
      case 'plugin_deactivate':
        return Promise.resolve(undefined)
      case 'plugin_mark_error':
        return Promise.resolve(undefined)
      default:
        return Promise.resolve(undefined)
    }
  })
}

function invokeCalls(cmd: string): unknown[][] {
  // 去掉调用数组首元素（命令名），只保留参数：与 toHaveBeenCalledWith 的参数形态一致
  return mockInvoke.mock.calls.filter(([c]) => c === cmd).map((call) => call.slice(1))
}

/**
 * 推进异步链：setTimeout(0) 只在微任务队列排空后触发，mock invoke 的纯微任务
 * async 链一次调用即可全部推进；fake timers 下 setTimeout 被伪造，用
 * advanceTimersByTimeAsync(0) 等价推进
 */
async function flushAsync(): Promise<void> {
  if (vi.isFakeTimers()) {
    await vi.advanceTimersByTimeAsync(0)
  } else {
    await new Promise((r) => setTimeout(r, 0))
  }
}

function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/plugins/:id', name: 'plugin-detail', component: { template: '<div />' } }],
  })
}

let wrapper: ReturnType<typeof mount> | null = null
let consoleWarnSpy: ReturnType<typeof vi.spyOn>
let consoleErrorSpy: ReturnType<typeof vi.spyOn>

beforeEach(() => {
  vi.clearAllMocks()
  setActivePinia(createPinia())
  backendPlugins = []
  installInvokeMock()
  // loader 单例跨用例清空（模块级 Map）；activate 失败路径不残留任何注册
  pluginLoader.deactivate('com.bedcode.demo').catch(() => {})
  pluginLoader.deactivate('com.bedcode.other').catch(() => {})
  // 静音预期内的 warn（无前端模块停用）与 error（动态 import 失败）
  consoleWarnSpy = vi.spyOn(logger, 'warn').mockImplementation(() => {})
  consoleErrorSpy = vi.spyOn(logger, 'error').mockImplementation(() => {})
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
  consoleWarnSpy.mockRestore()
  consoleErrorSpy.mockRestore()
  vi.useRealTimers()
})

async function mountView() {
  wrapper = mount(PluginsView, {
    global: {
      plugins: [createPinia(), makeRouter(), i18n],
    },
  })
  await flushAsync()
}

// ==================== 场景 ====================

describe('插件流：pluginLoader × usePluginManager × PluginsView', () => {
  it('清单加载：plugin_list_loaded → 已启用/未启用分区渲染', async () => {
    backendPlugins = [
      makePluginInfo({
        id: 'com.bedcode.demo',
        name: 'Demo Plugin',
        state: { state: 'Activated' },
      }),
      makePluginInfo({
        id: 'com.bedcode.other',
        name: 'Other Plugin',
        state: { state: 'Deactivated' },
      }),
    ]
    await mountView()

    // 清单经 loader 命令层真实取数
    expect(invokeCalls('plugin_list_loaded')).toHaveLength(1)
    // 计数联动：1/2 已启用
    expect(wrapper!.text()).toContain('1/2')
    // 分区渲染：激活进已启用，停用进未启用
    const sections = wrapper!.text()
    expect(sections).toContain('已启用')
    expect(sections).toContain('未启用')
    // Demo Plugin 行的开关是「停用」语义，Other Plugin 是「启用」语义
    expect(wrapper!.find('[aria-label="停用"]').exists()).toBe(true)
    expect(wrapper!.find('[aria-label="启用"]').exists()).toBe(true)
  })

  it('降级插件：进已启用分区 + 显示「已降级」徽章 + 开关保持停用语义', async () => {
    backendPlugins = [
      makePluginInfo({
        id: 'com.bedcode.demo',
        name: 'Demo Plugin',
        state: { state: 'Activated' },
      }),
      // on_startup 失败的 Degraded：实例在运行，归属已启用分区但带降级标识
      makeDegradedPluginInfo({
        id: 'com.bedcode.other',
        name: 'Other Plugin',
        state: { state: 'Degraded', error: 'on_startup failed: db locked' },
      }),
    ]
    await mountView()

    const text = wrapper!.text()
    // 分区计数含 Degraded（实例运行中），且徽章明确标注降级而非笼统「已启用」
    expect(text).toContain('2/2')
    expect(text).toContain('已启用')
    expect(text).toContain('已降级')
    // 降级行开关保持「停用」语义（实例在运行），不误入未启用分区
    const disableToggles = wrapper!.findAll('[aria-label="停用"]')
    expect(disableToggles.length).toBe(2)
    // 悬停可见原始降级原因
    const badge = wrapper!.find('span[title="on_startup failed: db locked"]')
    expect(badge.exists()).toBe(true)
    expect(badge.text()).toContain('已降级')
  })

  it('停用联动：toggle → loader 停用 → 列表重载 → 行从已启用迁到未启用', async () => {
    vi.useFakeTimers()
    backendPlugins = [
      makePluginInfo({
        id: 'com.bedcode.demo',
        name: 'Demo Plugin',
        state: { state: 'Activated' },
      }),
    ]
    await mountView()

    // 模拟后端状态变化：停用成功后后端持久化为 Deactivated
    backendPlugins = [
      makePluginInfo({
        id: 'com.bedcode.demo',
        name: 'Demo Plugin',
        state: { state: 'Deactivated' },
      }),
    ]

    await wrapper!.find('[aria-label="停用"]').trigger('click')
    await flushAsync()
    // 列表已重载（行迁到未启用分区，按钮 aria-label 变「启用」），但 500ms 最小
    // 遮罩时长内 togglingId 未清 → 开关保持禁用（防重复点击联动）
    const movingToggle = wrapper!.find('[aria-label="启用"]')
    expect(movingToggle.exists()).toBe(true)
    expect(movingToggle.attributes('disabled')).toBeDefined()
    await vi.advanceTimersByTimeAsync(600)
    await flushAsync()

    // 状态联动：列表重载（第二次 plugin_list_loaded）+ 行迁到未启用分区
    expect(invokeCalls('plugin_list_loaded')).toHaveLength(2)
    // loader 无前端模块（测试环境未加载入口）→ 不通知后端 plugin_deactivate，
    // 状态以后端重载结果为准
    expect(invokeCalls('plugin_deactivate')).toHaveLength(0)
    // 行迁移完成：停用按钮消失，出现启用按钮且恢复可用，计数 0/1
    expect(wrapper!.find('[aria-label="停用"]').exists()).toBe(false)
    expect(wrapper!.find('[aria-label="启用"]').attributes('disabled')).toBeUndefined()
    expect(wrapper!.text()).toContain('0/1')
  })

  it('启用联动：toggle → 后端激活通知 + 前端加载失败恢复（mark_error + 列表重载）', async () => {
    vi.useFakeTimers()
    backendPlugins = [
      makePluginInfo({
        id: 'com.bedcode.other',
        name: 'Other Plugin',
        state: { state: 'Deactivated' },
      }),
    ]
    await mountView()

    await wrapper!.find('[aria-label="启用"]').trigger('click')
    await flushAsync()
    // 动态 import 失败经 vite-node 异步机制（可能含定时器链）到达 loader 的 catch，
    // runAllTimersAsync 反复推进到无待执行定时器，再排空微任务
    await vi.runAllTimersAsync()
    await flushAsync()

    // 启用链路真实执行：loader 先取插件信息，再通知后端激活
    expect(invokeCalls('plugin_get_info')).toEqual([[{ pluginId: 'com.bedcode.other' }]])
    expect(invokeCalls('plugin_activate')).toEqual([[{ pluginId: 'com.bedcode.other' }]])
    // 动态 import 入口失败（asset 协议无服务器）→ 失败恢复：mark_error + 列表重载
    expect(invokeCalls('plugin_mark_error')).toHaveLength(1)
    expect(invokeCalls('plugin_mark_error')[0]).toEqual([
      { pluginId: 'com.bedcode.other', error: expect.any(String) },
    ])
    expect(invokeCalls('plugin_list_loaded').length).toBeGreaterThanOrEqual(2)
    // 后端状态未变 → 行留在未启用；toggle 结束 → 按钮恢复可用
    expect(wrapper!.find('[aria-label="启用"]').exists()).toBe(true)
    expect(wrapper!.find('[aria-label="启用"]').attributes('disabled')).toBeUndefined()
    expect(wrapper!.text()).toContain('0/1')
  })
})
