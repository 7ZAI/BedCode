import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import i18n from '@/locales'
import PluginApprovalDialog from '@/components/PluginApprovalDialog.vue'
import type { PluginInfo } from '@/plugin/types'

/**
 * 权限审批弹层行为测试（ADR 0020 / 审计票 03）
 *
 * 被测契约：
 * - 渲染：逐条列出 manifest 声明权限，高危位额外出现高位标记与后果文案
 * - 提交：确认 → 调宿主 `plugin_approve` → 成功 emit('approved') + 成功 toast
 * - 反例：宿主拒绝 → 错误 toast、不 emit approved（弹层保持，供重试）
 * - 取消：不调宿主、emit('close')
 */

const invokeMock = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }))

const toastMock = vi.hoisted(() => ({ success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() }))
vi.mock('vue-sonner', () => ({ toast: toastMock }))

function makePlugin(overrides: Partial<PluginInfo> = {}): PluginInfo {
  return {
    id: 'com.test.approve',
    name: 'Approve Test',
    version: '1.0.0',
    description: '',
    author: 'tester',
    main: 'index.js',
    pluginType: 'ts-only',
    rustLibrary: '',
    permissions: ['storage', 'process:run', 'pty:spawn'],
    state: { state: 'NeedsApproval' },
    extensionPath: '/tmp/plugins/com.test.approve',
    contributes: {},
    source: 'user-installed',
    sizeBytes: 10,
    ...overrides,
  } as PluginInfo
}

function mountDialog(plugin: PluginInfo | null) {
  return mount(PluginApprovalDialog, {
    props: { plugin },
    // teleport 用内联桩：Teleport 会把节点挪到 body，wrapper.text()/findAll 够不着
    global: { plugins: [i18n], stubs: { teleport: true, transition: false } },
    attachTo: document.body,
  })
}

/** 弹层确认按钮（第二个操作按钮：取消在前，确认为带品牌的按钮） */
function confirmButton(wrapper: ReturnType<typeof mountDialog>) {
  const buttons = wrapper.findAll('button')
  return buttons[buttons.length - 1]
}

describe('PluginApprovalDialog', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    toastMock.success.mockReset()
    toastMock.error.mockReset()
  })

  it('逐条渲染权限清单，高危位带高位标记与后果文案', () => {
    const wrapper = mountDialog(makePlugin())
    const text = wrapper.text()

    // 三条件权限文案（来自真实 i18n，非占位）
    expect(text).toContain('存储')
    expect(text).toContain('进程执行')
    expect(text).toContain('私有终端创建')
    // 高危位：标记 + 后果文案各出现两次（process:run / pty:spawn）
    expect(wrapper.findAll('span').filter((s) => s.text() === '高危')).toHaveLength(2)
    expect(text).toContain('可在你的机器上执行任意命令与脚本')
    expect(text).toContain('可创建伪终端并在你的机器上启动任意 shell')
    // 非高危位（storage）不得出现后果文案
    expect(text).not.toContain('读写应用本地存储（高危）')
    // 未知权限回退原文，不得显示成占位符
    expect(text).not.toContain('未知权限')
  })

  it('未请求任何权限时显示空态文案', () => {
    const wrapper = mountDialog(makePlugin({ permissions: [] }))
    expect(wrapper.text()).toContain('该应用未请求任何权限')
  })

  it('确认后调用宿主审批命令并通知调用方', async () => {
    invokeMock.mockResolvedValue(['storage', 'process:run'])
    const wrapper = mountDialog(makePlugin())

    await confirmButton(wrapper).trigger('click')
    await flushPromises()

    expect(invokeMock).toHaveBeenCalledWith('plugin_approve', { pluginId: 'com.test.approve' })
    expect(wrapper.emitted('approved')).toEqual([['com.test.approve']])
    expect(toastMock.success).toHaveBeenCalledTimes(1)
    expect(toastMock.error).not.toHaveBeenCalled()
  })

  it('宿主拒绝时不通知成功，弹层保持并提示错误', async () => {
    invokeMock.mockRejectedValue(new Error('requires user approval'))
    const wrapper = mountDialog(makePlugin())

    await confirmButton(wrapper).trigger('click')
    await flushPromises()

    expect(wrapper.emitted('approved')).toBeUndefined()
    expect(toastMock.success).not.toHaveBeenCalled()
    expect(toastMock.error).toHaveBeenCalledTimes(1)
    expect(String(toastMock.error.mock.calls[0][0])).toContain('requires user approval')
    // 弹层仍在（plugin prop 未变）→ 用户可重试
    expect(wrapper.text()).toContain('批准应用权限')
  })

  it('取消不调用宿主，仅通知关闭', async () => {
    const wrapper = mountDialog(makePlugin())

    const cancel = wrapper.findAll('button')[0]
    await cancel.trigger('click')
    await flushPromises()

    expect(invokeMock).not.toHaveBeenCalled()
    expect(wrapper.emitted('close')).toHaveLength(1)
    expect(wrapper.emitted('approved')).toBeUndefined()
  })

  it('plugin 为 null 时不渲染弹层（调用方以 v-if/prop 控制）', () => {
    const wrapper = mountDialog(null)
    expect(wrapper.text()).toBe('')
  })
})
