/**
 * PluginView toggle 失败收敛测试（spec D4 / issue 05）
 *
 * 被测行为：handlePluginToggle catch / 超时兜底路径收敛到「停用」终态——
 * UI 开关回退 + 幂等 pluginLoader.deactivate 拆解后端运行时；持久化
 * enabled 不回写（用户意图保留，下次启动 auto-activate 自愈重试）。
 *
 * 测试 seam：mock @/plugin/commands（后端命令层）+ @/plugin/loader（激活/
 * 拆解可控抛错）+ useToast / vue-router / vue-i18n / plugin-dialog 边界；
 * Toggle 以桩组件模拟 v-model 交互，断言经内部实例 setupState 读取。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import PluginView from '@/views/PluginView.vue'
import { makePluginInfo } from '@/__tests__/fixtures'

// ==================== mock 边界 ====================

const h = vi.hoisted(() => ({
  activate: vi.fn(),
  deactivate: vi.fn(),
  pluginListLoaded: vi.fn(),
  pluginIsEnabled: vi.fn(),
  pluginSetEnabled: vi.fn(),
  pluginPreauthorize: vi.fn(),
}))

vi.mock('@/plugin/loader', () => ({
  pluginLoader: { activate: h.activate, deactivate: h.deactivate },
}))
vi.mock('@/plugin/commands', () => ({
  pluginListLoaded: h.pluginListLoaded,
  pluginIsEnabled: h.pluginIsEnabled,
  pluginSetEnabled: h.pluginSetEnabled,
  pluginPreauthorize: h.pluginPreauthorize,
  pluginInstallFromFile: vi.fn(),
  pluginDownload: vi.fn(),
  pluginUninstall: vi.fn(),
  pluginApprove: vi.fn(),
}))
vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    show: vi.fn(),
    success: vi.fn(),
    error: vi.fn(),
    warning: vi.fn(),
    info: vi.fn(),
  }),
}))
vi.mock('vue-router', () => ({
  useRouter: () => ({ push: vi.fn() }),
}))
vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (k: string) => k }),
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}))

/** Toggle 桩：模拟 v-model 交互（点击发出 update:modelValue 取反） */
const ToggleStub = {
  name: 'Toggle',
  props: { modelValue: { type: Boolean, default: false } },
  emits: ['update:modelValue'],
  template:
    '<button class="toggle-stub" :data-on="String(modelValue)" @click="$emit(\'update:modelValue\', !modelValue)" />',
}

function mountView() {
  return mount(PluginView, {
    global: {
      stubs: {
        Toggle: ToggleStub,
        PluginIcon: true,
        ConfirmDialog: true,
        LoadingDialog: true,
        CollapseSection: true,
      },
      mocks: { $t: (k: string) => k },
    },
  })
}

const PLUGIN_ID = 'com.bedcode.file-transfer'

beforeEach(() => {
  vi.clearAllMocks()
  vi.spyOn(console, 'error').mockImplementation(() => {})
  vi.spyOn(console, 'log').mockImplementation(() => {})
  vi.spyOn(console, 'warn').mockImplementation(() => {})
})

afterEach(() => {
  vi.useRealTimers()
  vi.restoreAllMocks()
})

describe('handlePluginToggle 失败收敛', () => {
  it('启用失败：幂等拆解后端 + 开关回退停用 + 持久化不回写', async () => {
    vi.useFakeTimers()
    const info = makePluginInfo({
      id: PLUGIN_ID,
      name: 'File Transfer',
      pluginType: 'wasm',
      permissions: ['ui:toolbox'],
      state: { state: 'Deactivated' },
    })
    h.pluginListLoaded.mockResolvedValue([info])
    h.pluginIsEnabled.mockResolvedValue(false)
    h.pluginSetEnabled.mockResolvedValue(undefined)
    h.pluginPreauthorize.mockResolvedValue(undefined)
    h.activate.mockRejectedValue(new Error('activation boom'))
    h.deactivate.mockResolvedValue(undefined)

    const wrapper = mountView()
    await flushPromises()
    await flushPromises()

    // 初始：未启用分区，开关为关（点击即「启用」方向）
    expect(wrapper.find('.toggle-stub').attributes('data-on')).toBe('false')

    // 用户点击启用 → 激活失败
    await wrapper.find('.toggle-stub').trigger('click')
    await flushPromises()
    await vi.advanceTimersByTimeAsync(1100) // TOGGLE_MIN_DURATION_MS 补齐
    await flushPromises()

    // 持久化意图只写入一次（启用方向，不回写 false）
    expect(h.pluginSetEnabled).toHaveBeenCalledTimes(1)
    expect(h.pluginSetEnabled).toHaveBeenCalledWith(PLUGIN_ID, true)
    // 幂等拆解后端运行时（catch 路径追加）
    expect(h.deactivate).toHaveBeenCalledWith(PLUGIN_ID)
    // UI 开关收敛到停用终态
    expect((wrapper.vm.$ as any).setupState.pluginEnabledStates[PLUGIN_ID]).toBe(false)
    expect(wrapper.find('.toggle-stub').attributes('data-on')).toBe('false')
  })

  it('启用失败且拆解也失败：开关仍回退，错误仅记日志不阻塞收尾', async () => {
    vi.useFakeTimers()
    const info = makePluginInfo({
      id: PLUGIN_ID,
      name: 'File Transfer',
      pluginType: 'wasm',
      permissions: ['ui:toolbox'],
      state: { state: 'Deactivated' },
    })
    h.pluginListLoaded.mockResolvedValue([info])
    h.pluginIsEnabled.mockResolvedValue(false)
    h.pluginSetEnabled.mockResolvedValue(undefined)
    h.pluginPreauthorize.mockResolvedValue(undefined)
    h.activate.mockRejectedValue(new Error('activation boom'))
    h.deactivate.mockRejectedValue(new Error('teardown boom'))

    const wrapper = mountView()
    await flushPromises()
    await flushPromises()

    await wrapper.find('.toggle-stub').trigger('click')
    await flushPromises()
    await vi.advanceTimersByTimeAsync(1100)
    await flushPromises()

    // 拆解尝试过（失败已兜底），UI 开关回退到用户原方向取反（停用）
    expect(h.deactivate).toHaveBeenCalledWith(PLUGIN_ID)
    expect((wrapper.vm.$ as any).setupState.pluginEnabledStates[PLUGIN_ID]).toBe(false)
  })
})
