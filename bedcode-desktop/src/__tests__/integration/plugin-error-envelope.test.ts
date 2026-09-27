/**
 * 插件运行时错误信封集成测试（票 03，票 05 启用，随全量回归执行）
 *
 * 跨边界形状（与 Rust `system::error::EventEnvelope::payload` 逐字段对齐）：
 *   宿主 `notify_plugin_runtime_error` / `notify_plugin_self_check_error`
 *     → 事件 `plugin:runtime-error` / `plugin:error` 载荷 `{ code, request_id, params }`
 *     → runtime-listeners → parseInvokeError → showUserError → toast 友好文案
 *   以及插件降级态：PluginDetailView 降级段只出带应用名的通用文案（无失败原因原文）。
 *
 * 断言的硬不变量（ADR 0030 §2）：
 * - toast 文本只含 `errors.<code>` 模板 + 显示名插值，**不含** code / request_id /
 *   panic 消息 / 回溯 / 命令名
 * - 技术详情的唯一落点是日志：`logger.error` 拿到 code + request_id（可与宿主日志
 *   里同号详情按值关联）
 * - 事件载荷畸形 / 缺 code（遗留形状）不抛错，落 `host.internal` 通用文案
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createRouter, createMemoryHistory } from 'vue-router'
import i18n from '@/locales'
import { setupPluginRuntimeListeners } from '@/plugin/runtime-listeners'
import { logger } from '@/utils/frontendLogger'
import PluginDetailView from '@/views/PluginDetailView.vue'
import { makePluginInfo, makeDegradedPluginInfo } from '@/__tests__/fixtures/index'

// ==================== mock Tauri / toast 边界 ====================

const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
  convertFileSrc: (p: string) => `asset://localhost/${p}`,
}))

/** 事件监听注册表：事件名 → handler（模拟 Tauri listen 捕获宿主派发） */
const listeners = new Map<string, (event: { payload: unknown }) => void>()
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((name: string, handler: (event: { payload: unknown }) => void) => {
    listeners.set(name, handler)
    return Promise.resolve(() => {})
  }),
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

/** 模拟宿主派发一条事件（载荷形状 = Rust EventEnvelope::payload） */
function emit(name: string, payload: unknown): void {
  listeners.get(name)?.({ payload })
}

// ==================== 基建 ====================

let errorSpy: ReturnType<typeof vi.spyOn>
let wrapper: ReturnType<typeof mount> | null = null
let backendPlugins: ReturnType<typeof makePluginInfo>[] = []

async function flushAsync(): Promise<void> {
  await new Promise((r) => setTimeout(r, 0))
}

beforeEach(() => {
  vi.clearAllMocks()
  listeners.clear()
  setActivePinia(createPinia())
  backendPlugins = []
  mockInvoke.mockImplementation((cmd: string) =>
    Promise.resolve(
      cmd === 'plugin_list_loaded'
        ? [...backendPlugins]
        : cmd === 'plugin_get_info'
          ? (backendPlugins[0] ?? null)
          : undefined,
    ),
  )
  errorSpy = vi.spyOn(logger, 'error').mockImplementation(() => {})
  setupPluginRuntimeListeners()
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
  errorSpy.mockRestore()
})

describe('插件运行时错误信封', () => {
  it('trap：toast 只显示友好模板 + 应用名，详情不进界面', () => {
    emit('plugin:runtime-error', {
      code: 'host.plugin.trap',
      request_id: 'a1b2c3d4',
      params: { name: 'AI Chatbox' },
    })

    expect(mockedToast.error).toHaveBeenCalledWith(
      '应用「AI Chatbox」运行异常，已尝试自动恢复',
      { duration: 5000 },
    )
    const message = mockedToast.error.mock.calls[0][0] as string
    expect(message).not.toContain('host.plugin.trap')
    expect(message).not.toContain('a1b2c3d4')
  })

  it('自动恢复失败：提示去应用中心处理，不含恢复失败原因', () => {
    emit('plugin:runtime-error', {
      code: 'host.plugin.recovery-failed',
      request_id: 'beefcafe',
      params: { name: '文件传输' },
    })
    expect(mockedToast.error.mock.calls[0][0]).toBe(
      '应用「文件传输」运行异常且未能自动恢复，请到应用中心处理',
    )
  })

  it('自检失败：plugin:error 载荷走 { plugin } 参数模板', () => {
    emit('plugin:error', {
      code: 'host.plugin.self-check-failed',
      request_id: '0badf00d',
      params: { plugin: '终端会话' },
    })
    expect(mockedToast.error.mock.calls[0][0]).toBe('应用「终端会话」启动自检失败，请检查配置')
  })

  it('技术详情只落日志：code + request_id 可与宿主日志按值关联', () => {
    // 曾经的回归：panic 消息 / 回溯被截断 120 字塞进 toast
    const panicDetail = 'wasm trap: unreachable at 0x1234\nstack backtrace:\n  0: 0x1234'
    emit('plugin:runtime-error', {
      code: 'host.plugin.trap',
      request_id: 'deadbeef',
      params: { name: 'AI Chatbox' },
    })

    const message = mockedToast.error.mock.calls[0][0] as string
    for (const fragment of ['wasm trap', 'unreachable', '0x1234', 'backtrace', panicDetail]) {
      expect(message).not.toContain(fragment)
    }
    // showUserError 日志：arg1=消息串（code + request_id，可寻址宿主日志同号详情）；
    // arg2=归一化后的 UserError（camelCase requestId）——原始事件信封已在其 message 字段
    expect(errorSpy).toHaveBeenCalledWith(
      expect.stringContaining('code=host.plugin.trap'),
      expect.objectContaining({ requestId: 'deadbeef' }),
    )
  })

  it('畸形 / 遗留形状载荷：落通用文案，不抛错也不静默吞', () => {
    emit('plugin:runtime-error', { plugin_id: 'com.bedcode.legacy', error: 'raw string payload' })
    expect(mockedToast.error).toHaveBeenCalledWith('操作未完成，请稍后重试', { duration: 5000 })

    emit('plugin:error', 'not-an-object')
    expect(mockedToast.error).toHaveBeenCalledTimes(2)
  })

  it('业务通知通道（plugin:notify）维持现状：原样 info 提示', () => {
    emit('plugin:notify', {
      plugin_id: 'com.bedcode.terminal-session',
      title: '会话已结束',
      body: '点击查看详情',
    })
    // useToast.info → toast.info(message, { duration, position })
    expect(mockedToast.info.mock.calls[0][0]).toBe('会话已结束: 点击查看详情')
    expect(mockedToast.error).not.toHaveBeenCalled()
  })

  it('降级插件详情页：降级段只出带应用名的通用文案', async () => {
    backendPlugins = [
      makeDegradedPluginInfo({
        id: 'com.bedcode.other',
        name: 'Other Plugin',
        state: { state: 'Degraded', error: 'on_startup failed: db migration error at line 42' },
      }),
    ]
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [{ path: '/plugins/:id', name: 'plugin-detail', component: { template: '<div />' } }],
    })
    router.push('/plugins/com.bedcode.other')
    await router.isReady()

    wrapper = mount(PluginDetailView, {
      global: { plugins: [createPinia(), router, i18n] },
    })
    await flushAsync()

    const text = wrapper.text()
    expect(text).toContain('应用「Other Plugin」降级运行，部分功能不可用')
    expect(text).not.toContain('db migration error')
    expect(text).not.toContain('line 42')
  })
})
