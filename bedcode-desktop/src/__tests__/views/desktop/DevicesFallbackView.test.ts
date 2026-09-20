/**
 * DevicesFallbackView 行为契约（票 14）
 *
 * 契约来源：票 13 会话页兜底壳的用户裁决口径（最小壳 = 提示 + 基本操作）在设备域的
 * 同构落地 + 票 14 票面「宿主内置设备入口让位为跳转壳」。
 *
 * 契约清单：
 * - C1 挂载：兜底提示文案 + 经宿主 `list_paired_devices` 取已配对设备并渲染
 * - C2 空态：无已配对设备时给出空态文案（不显示空列表）
 * - C3 撤销设备：确认后 `remove_paired_device` 带 id，随后重新取列表（界面同步）
 * - C4 撤销失败：错误被记录（不静默吞掉），弹窗仍关闭不留假状态
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import i18n from '@/locales'
import DevicesFallbackView from '@/views/DevicesFallbackView.vue'
import { makePairing } from '@/__tests__/fixtures/index'

const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}))

const loggerError = vi.fn()
vi.mock('@/utils/frontendLogger', () => ({
  logger: { log: vi.fn(), info: vi.fn(), warn: vi.fn(), error: (...a: unknown[]) => loggerError(...a) },
}))

let pairedDevices: unknown[]

function installInvokeMock() {
  mockInvoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case 'list_paired_devices':
        return Promise.resolve([...pairedDevices])
      case 'remove_paired_device':
        return Promise.resolve(undefined)
      default:
        return Promise.resolve(undefined)
    }
  })
}

function invokeCalls(cmd: string): unknown[][] {
  return mockInvoke.mock.calls.filter(([c]) => c === cmd).map((call) => call.slice(1))
}

function mountView() {
  return mount(DevicesFallbackView, {
    global: {
      plugins: [createPinia(), i18n],
      stubs: { teleport: true },
    },
  })
}

describe('DevicesFallbackView（设备域兜底壳）', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setActivePinia(createPinia())
    pairedDevices = [
      makePairing({
        id: 'device-1',
        deviceName: 'Phone 1',
        deviceFingerprint: 'fp-1',
        address: '192.168.1.50',
      }),
    ]
    installInvokeMock()
  })

  it('C1 挂载：展示兜底说明，并渲染已配对设备（名称 + 地址）', async () => {
    const wrapper = mountView()
    await flushPromises()

    expect(invokeCalls('list_paired_devices')).toHaveLength(1)
    const text = wrapper.text()
    expect(text).toContain(i18n.global.t('desktop.device.fallbackNotice'))
    expect(text).toContain('Phone 1')
    expect(text).toContain('192.168.1.50')
  })

  it('C2 空态：无已配对设备时给出空态文案', async () => {
    pairedDevices = []
    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.text()).toContain(i18n.global.t('desktop.device.noPaired'))
    expect(wrapper.text()).not.toContain('Phone 1')
  })

  it('C3 撤销设备：确认后调用 remove_paired_device 并重新取列表', async () => {
    const wrapper = mountView()
    await flushPromises()

    // 卡片内的「移除」
    const removeBtn = wrapper
      .findAll('button')
      .find((b) => b.text() === i18n.global.t('common.button.remove'))
    await removeBtn!.trigger('click')
    await flushPromises()

    // 确认弹窗内的「移除」
    const confirm = wrapper
      .findAll('button')
      .filter((b) => b.text() === i18n.global.t('common.button.remove'))
      .find((b) => b.element.closest('.fixed') !== null)
    expect(confirm, '撤销确认弹窗必须出现').toBeTruthy()
    await confirm!.trigger('click')
    await flushPromises()

    expect(invokeCalls('remove_paired_device')[0][0]).toEqual({ id: 'device-1' })
    // 撤销后重新取列表（界面同步，不留已删除条目）
    expect(invokeCalls('list_paired_devices')).toHaveLength(2)
  })

  it('C4 撤销失败：错误被记录，弹窗关闭且不残留待撤销状态', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'remove_paired_device') return Promise.reject(new Error('db locked'))
      if (cmd === 'list_paired_devices') return Promise.resolve([...pairedDevices])
      return Promise.resolve(undefined)
    })
    const wrapper = mountView()
    await flushPromises()

    const removeBtn = wrapper
      .findAll('button')
      .find((b) => b.text() === i18n.global.t('common.button.remove'))
    await removeBtn!.trigger('click')
    await flushPromises()
    const confirm = wrapper
      .findAll('button')
      .filter((b) => b.text() === i18n.global.t('common.button.remove'))
      .find((b) => b.element.closest('.fixed') !== null)
    await confirm!.trigger('click')
    await flushPromises()

    expect(loggerError).toHaveBeenCalled()
    expect(wrapper.find('.fixed').exists()).toBe(false)
  })
})
