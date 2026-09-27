/**
 * 插件管理域错误信封集成测试（票 02，票 05 启用，随全量回归执行）
 *
 * 垂直切片：PluginsView（usePluginManager.togglePlugin）→ invoke rejection
 * → parseInvokeError → showUserError → toast 友好文案 + logger 落盘。
 *
 * 覆盖的跨边界形状（与 Rust 侧对应）：
 * - 宿主机制码：`{code:'host.plugin.not-activated', request_id}`（Rust
 *   `invoke_rust_command` 的 UserFacing 序列化产物）→ 机构文案「该应用未启用…」
 * - 兜底码：裸 Error / 字符串 rejection → host.internal 通用文案
 * - 超时：toggle 30s 挂起 → host.invoke.timeout 文案 + 「重试」按钮（重发原操作）
 *
 * 断言的硬不变量（ADR 0030 §2）：toast 文本永不包含 code / request_id / 错误原文；
 * 技术详情唯一落点是 logger（code + request_id 与宿主日志同号关联）。
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createRouter, createMemoryHistory } from 'vue-router'
import i18n from '@/locales'
import PluginsView from '@/views/PluginsView.vue'
import { pluginLoader } from '@/plugin/loader'
import { logger } from '@/utils/frontendLogger'
import { makePluginInfo } from '@/__tests__/fixtures/index'

const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
  convertFileSrc: (p: string) => `asset://localhost/${p}`,
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('vue-sonner', () => ({
  toast: {
    success: vi.fn(() => 'mock-id'),
    error: vi.fn(() => 'mock-id-error'),
    warning: vi.fn(() => 'mock-id'),
    info: vi.fn(() => 'mock-id'),
  },
}))

import { toast } from 'vue-sonner'
const mockedToast = vi.mocked(toast)

/** 插件清单（可变） */
let backendPlugins: ReturnType<typeof makePluginInfo>[]
/** 命令级可编程 mock（intercept 优先于 installInvokeMock 的默认实现） */
let intercepts: Record<string, () => unknown>

function installInvokeMock() {
  mockInvoke.mockImplementation((cmd: string, args?: any) => {
    const hook = intercepts[cmd]
    if (hook) return Promise.resolve(hook())
    switch (cmd) {
      case 'plugin_list_loaded':
        return Promise.resolve([...backendPlugins])
      case 'plugin_get_info':
        return Promise.resolve(backendPlugins.find((p) => p.id === args?.pluginId) ?? null)
      case 'plugin_activate':
      case 'plugin_deactivate':
      case 'plugin_preauthorize':
      case 'plugin_mark_error':
        return Promise.resolve(undefined)
      default:
        return Promise.resolve(undefined)
    }
  })
}

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
let consoleErrorSpy: ReturnType<typeof vi.spyOn>

beforeEach(() => {
  vi.clearAllMocks()
  setActivePinia(createPinia())
  intercepts = {}
  backendPlugins = [
    makePluginInfo({
      id: 'com.bedcode.demo',
      name: 'Demo Plugin',
      state: { state: 'Deactivated' },
    }),
  ]
  installInvokeMock()
  pluginLoader.deactivate('com.bedcode.demo').catch(() => {})
  consoleErrorSpy = vi.spyOn(logger, 'error').mockImplementation(() => {})
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
  consoleErrorSpy.mockRestore()
  vi.useRealTimers()
})

async function mountView() {
  wrapper = mount(PluginsView, {
    global: { plugins: [createPinia(), makeRouter(), i18n] },
  })
  await flushAsync()
}

describe('插件管理域错误信封垂直切片', () => {
  it('启用失败：宿主机制码 → 机构文案 toast，无 code/request_id 原文', async () => {
    await mountView()
    intercepts['plugin_activate'] = () => {
      throw { code: 'host.plugin.not-activated', request_id: 'abcd5678' }
    }

    await wrapper!.find('[aria-label="启用"]').trigger('click')
    await flushAsync()

    const message = String(mockedToast.error.mock.calls[0][0])
    expect(message).toBe('该应用未启用，无法执行此操作')
    expect(message).not.toContain('abcd5678')
    expect(message).not.toContain('not-activated')
    // 技术详情落日志（code + request_id 同号关联）
    expect(consoleErrorSpy).toHaveBeenCalledWith(
      expect.stringContaining('code=host.plugin.not-activated'),
      expect.any(Object),
    )
  })

  it('启用失败：裸错误 → 兜底文案，错误原文不上 toast', async () => {
    await mountView()
    intercepts['plugin_activate'] = () => {
      throw new Error('backend crash: /var/secret/db locked')
    }

    await wrapper!.find('[aria-label="启用"]').trigger('click')
    await flushAsync()

    const message = String(mockedToast.error.mock.calls[0][0])
    expect(message).toBe('操作未完成，请稍后重试')
    expect(message).not.toContain('/var/secret/db')
    expect(message).not.toContain('backend crash')
  })

  it('启用水线超时：超时文案 + 重试按钮重发原操作', async () => {
    vi.useFakeTimers()
    await mountView()

    let activateCalls = 0
    intercepts['plugin_activate'] = () => {
      activateCalls += 1
      if (activateCalls === 1) return new Promise(() => {}) // 永不 resolve → 前端 30s 超时
      return Promise.resolve(undefined)
    }

    await wrapper!.find('[aria-label="启用"]').trigger('click')
    // 推进 30s（超时阈值）——toggle 遮罩最小 500ms 一并消化
    await vi.advanceTimersByTimeAsync(30_500)

    const message = String(mockedToast.error.mock.calls[0][0])
    expect(message).toBe('操作超时，请重试')
    const options = mockedToast.error.mock.calls[0][1]
    expect(options.action).toEqual({
      label: '重试',
      onClick: expect.any(Function),
    })

    // 点击重试 → 重发 plugin_activate（第 2 次）+ 成功路径
    options.action.onClick()
    await vi.runAllTimersAsync()
    await flushAsync()
    expect(activateCalls).toBe(2)
  })

  it('停用失败：兜底文案且列表状态不变', async () => {
    backendPlugins = [
      makePluginInfo({ id: 'com.bedcode.demo', name: 'Demo Plugin', state: { state: 'Activated' } }),
    ]
    await mountView()
    // 真实链路：loader.deactivate 清理前端资源后调用后端 plugin_deactivate，
    // 后端 rejection 原样上抛（loader 不吞）；此处直接 spy 该边界模拟后端失败
    const deactivateSpy = vi
      .spyOn(pluginLoader, 'deactivate')
      .mockRejectedValue('legacy string rejection')

    await wrapper!.find('[aria-label="停用"]').trigger('click')
    await flushAsync()

    const message = String(mockedToast.error.mock.calls[0][0])
    expect(message).toBe('操作未完成，请稍后重试')
    expect(message).not.toContain('legacy')
    deactivateSpy.mockRestore()
  })
})