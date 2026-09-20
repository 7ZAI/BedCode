/**
 * ConnectionHistoryView 行为契约（票 14，插件前端）
 *
 * 契约来源：宿主 `ConnectionHistoryView.vue`（搬走前的行为基线）+ spec D2/D3。
 *
 * 契约清单：
 * - C1 路由带 deviceId：直接取该设备历史，按连接日分组渲染并给出统计
 * - C2 无 deviceId：取第一台已配对设备作为默认上下文后取历史（侧边栏直入也看得到内容）
 * - C3 清空历史：确认后调 `session.devices.history-clear`，列表清空并成功提示
 * - C4 取数失败：以错误文案提示，不静默（页面停在空态而非假成功）
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import ConnectionHistoryView from '../components/ConnectionHistoryView.vue'
import devMock from '../devMock'

vi.mock('vue-sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}))

const routeState = vi.hoisted(() => ({ query: {} as Record<string, unknown> }))
const routerPush = vi.fn(async () => {})

vi.mock('@binblink/bedcode-plugin-sdk-desktop', () => ({
  getRouter: () => ({
    push: routerPush,
    currentRoute: { value: { query: routeState.query } },
  }),
}))

const seed = devMock.pairing

async function defaultExecute(command: string, args?: unknown) {
  switch (command) {
    case 'session.devices.paired-list':
      return seed.pairedDevices
    case 'session.devices.history-list': {
      const deviceId = (args as { deviceId: string }).deviceId
      return seed.history.filter((e) => e.deviceId === deviceId)
    }
    case 'session.devices.history-clear':
      return { cleared: true }
    default:
      throw new Error(`unexpected command: ${command}`)
  }
}

const execute = vi.fn(defaultExecute)

function makeContext(): PluginContext {
  return {
    id: 'com.bedcode.session',
    i18n: {
      t: (key: string) => key,
      getI18n: () => ({ global: { locale: { value: 'zh-CN' }, t: (key: string) => key } }),
      registerMessages: vi.fn(),
    },
    commands: { execute },
  } as unknown as PluginContext
}

function mountView() {
  return mount(ConnectionHistoryView, {
    global: {
      provide: { pluginContext: makeContext() },
      stubs: { teleport: true },
    },
  })
}

function commandsTo(id: string) {
  return execute.mock.calls.filter((c) => c[0] === id)
}

function findButton(wrapper: ReturnType<typeof mountView>, text: string) {
  return wrapper.findAll('button').find((b) => b.text() === text)
}

describe('ConnectionHistoryView（连接历史页面）', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    execute.mockImplementation(defaultExecute)
    routeState.query = {}
  })

  it('C1 路由带 deviceId：取该设备历史并按日分组渲染统计', async () => {
    routeState.query = { deviceId: seed.pairedDevices[0].id }
    const wrapper = mountView()
    await flushPromises()

    expect(commandsTo('session.devices.history-list')[0][1]).toEqual({
      deviceId: seed.pairedDevices[0].id,
    })

    const text = wrapper.text()
    // 设备名（来自 paired-list）+ 统计（3 条：2 成功 1 失败）
    expect(text).toContain('Pixel 9')
    expect(text).toContain('pairing.history.statistics')
    // 分组标题按日期呈现（两组：09-20 与 09-19/09-18 依 demo 数据落在各自日期）
    expect(text).toContain('2026-09-20')
    // 认证方式与结果文案
    expect(text).toContain('pairing.history.method.pairingCode')
    expect(text).toContain('pairing.history.result.failed')
    expect(text).toContain('192.168.1.50:52000')
  })

  it('C2 无 deviceId：默认选第一台已配对设备并加载其历史', async () => {
    routeState.query = {}
    const wrapper = mountView()
    await flushPromises()

    expect(commandsTo('session.devices.history-list')[0][1]).toEqual({
      deviceId: seed.pairedDevices[0].id,
    })
    expect(wrapper.text()).toContain('Pixel 9')
  })

  it('C3 清空历史：确认后调 history-clear，列表清空并提示成功', async () => {
    routeState.query = { deviceId: seed.pairedDevices[0].id }
    const wrapper = mountView()
    await flushPromises()
    const { toast } = await import('vue-sonner')

    await findButton(wrapper, 'pairing.history.clear')!.trigger('click')
    await flushPromises()

    const confirm = wrapper
      .findAll('button')
      .filter((b) => b.text() === 'pairing.button.clear')
      .find((b) => b.element.closest('.fixed') !== null)
    expect(confirm, '清空确认弹窗必须出现').toBeTruthy()
    await confirm!.trigger('click')
    await flushPromises()

    expect(commandsTo('session.devices.history-clear')[0][1]).toEqual({
      deviceId: seed.pairedDevices[0].id,
    })
    expect(wrapper.text()).not.toContain('192.168.1.50:52000')
    expect(wrapper.text()).toContain('pairing.history.empty')
    expect(toast.success).toHaveBeenCalledWith('pairing.history.cleared')
  })

  it('C4 取数失败：以错误文案提示且停在空态（不静默、不显示假数据）', async () => {
    routeState.query = { deviceId: seed.pairedDevices[0].id }
    execute.mockImplementation(async (command: string) => {
      if (command === 'session.devices.history-list') throw new Error('boom')
      return defaultExecute(command)
    })
    const { toast } = await import('vue-sonner')

    const wrapper = mountView()
    await flushPromises()

    expect(toast.error).toHaveBeenCalledWith('pairing.history.loadFailed')
    expect(wrapper.text()).not.toContain('192.168.1.50:52000')
  })
})
