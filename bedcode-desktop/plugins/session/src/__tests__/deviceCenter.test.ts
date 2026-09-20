/**
 * DeviceCenterView 行为契约（票 14，插件前端）
 *
 * 契约来源：宿主 `DevicesView.vue`（搬走前的行为基线）+ spec D2/D3/D6。
 * 断言的是**外部可见行为**——发往插件后端的调用（命令名 + 入参）、渲染出的
 * 可见内容与文案 key、路由跳转；不测内部实现、不测 mock 自身。
 *
 * 演示数据取 `devMock.pairing`（插件工程持有），保证测试数据与 dev-shell
 * 演示数据同源。
 *
 * 契约清单：
 * - C1 加载：网络信息经 `session.network.info`、设备列表经
 *   `session.devices.paired-list` 取得并渲染（配置无 mock 命令面直调）
 * - C2 生成配对码：`session.pairing.generate` → 页面展示码与剩余秒
 * - C3 生成失败（回执无码）：不静默，提示「未收到有效配对码」
 * - C4 取消配对码：`session.pairing.clear` 且码从页面消失
 * - C5 生成 QR：`session.qr.generate` 带当前选中 host，且二维码画到 canvas
 * - C6 在线判定：`device-connected` 事件把设备移入在线区
 * - C7 离线判定：`device-disconnected` 事件把设备移回离线区
 * - C8 设备接入即清除已使用配对码（后端事件驱动状态流转）
 * - C9 撤销设备：确认弹窗 → `session.devices.revoke` → 重新取列表 → 成功提示
 * - C10 撤销失败：提示错误且不重复取列表（不留下「已删除」的假象）
 * - C11 查看历史：跳转同插件的历史目录并带 deviceId
 * - C12 移动端请求配对：后端事件 → 展示码 + 提示
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import DeviceCenterView from '../components/DeviceCenterView.vue'
import devMock from '../devMock'

vi.mock('vue-sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}))

vi.mock('qrcode', () => ({
  default: { toCanvas: vi.fn(async () => {}) },
}))

const routerPush = vi.fn(async () => {})

vi.mock('@binblink/bedcode-plugin-sdk-desktop', () => ({
  getRouter: () => ({
    push: routerPush,
    currentRoute: { value: { query: {} } },
  }),
}))

const seed = devMock.pairing

/** 命令面路由：按命令 id 返演示数据，并记录调用（断言入参） */
async function defaultExecute(command: string, args?: unknown) {
  switch (command) {
    case 'session.network.info':
      return seed.network
    case 'session.devices.paired-list':
      return seed.pairedDevices
    case 'session.pairing.generate':
      return seed.pairingCode
    case 'session.pairing.status':
      return seed.pairingCode
    case 'session.pairing.clear':
      return { cleared: true }
    // 默认无活跃 QR（本用例组自行指定恢复/生成路径，避免「恢复」静默掩盖「生成」）
    case 'session.qr.info':
      return null
    case 'session.qr.generate':
      return seed.qr
    case 'session.qr.clear':
      return { cleared: true }
    case 'session.devices.revoke':
      return { removed: true, kind: 'pairing' }
    default:
      throw new Error(`unexpected command: ${command} (args=${JSON.stringify(args)})`)
  }
}

const execute = vi.fn(defaultExecute)

/** 宿主事件订阅收集器（模拟 context.events.on 的 Tauri 桥接） */
const eventHandlers: Record<string, Array<(payload: unknown) => void>> = {}
const eventDispose = vi.fn()

const storageGet = vi.fn(async () => undefined as unknown)
const storageSet = vi.fn(async () => {})

function makeContext(): PluginContext {
  return {
    id: 'com.bedcode.session',
    i18n: {
      t: (key: string) => key,
      getI18n: () => ({ global: { locale: { value: 'zh-CN' }, t: (key: string) => key } }),
      registerMessages: vi.fn(),
    },
    commands: { execute },
    storage: { get: storageGet, set: storageSet },
    events: {
      on: (event: string, handler: (payload: unknown) => void) => {
        eventHandlers[event] = eventHandlers[event] ?? []
        eventHandlers[event].push(handler)
        return { dispose: eventDispose }
      },
      emit: vi.fn(),
    },
  } as unknown as PluginContext
}

function mountView() {
  return mount(DeviceCenterView, {
    global: {
      provide: { pluginContext: makeContext() },
      stubs: { teleport: true },
    },
  })
}

/** 触发某个宿主事件（backend push） */
async function fireEvent(event: string, payload?: unknown) {
  for (const handler of eventHandlers[event] ?? []) {
    handler(payload)
  }
  await flushPromises()
}

/** 按文案 key 找按钮（i18n 桩直返 key） */
function findButton(wrapper: ReturnType<typeof mountView>, text: string) {
  return wrapper.findAll('button').find((b) => b.text() === text)
}

function commandsTo(id: string) {
  return execute.mock.calls.filter((c) => c[0] === id)
}

describe('DeviceCenterView（设备与配对页面）', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    execute.mockImplementation(defaultExecute)
    storageGet.mockResolvedValue(undefined)
    for (const key of Object.keys(eventHandlers)) delete eventHandlers[key]
  })

  it('C1 加载：网络信息与设备列表各自经插件命令通道取得并渲染', async () => {
    const wrapper = mountView()
    await flushPromises()

    expect(commandsTo('session.network.info')).toHaveLength(1)
    expect(commandsTo('session.devices.paired-list')).toHaveLength(1)

    // 端口回显 + 两台配对设备（默认 Tab 为「设备配对」，设备卡片需切 Tab 才渲染）
    expect(wrapper.text()).toContain('9000')
    await findButton(wrapper, 'pairing.tab.devices')!.trigger('click')
    await flushPromises()
    const text = wrapper.text()
    expect(text).toContain('Pixel 9')
    expect(text).toContain('Reno 12')
    expect(text).toContain('pairing.device.sectionOnline')
    expect(text).toContain('pairing.device.sectionOffline')
  })

  it('C2 生成配对码：命令回执驱动页面展示码与剩余秒，并起倒计时', async () => {
    const wrapper = mountView()
    await flushPromises()

    await findButton(wrapper, 'pairing.code.generate')!.trigger('click')
    await flushPromises()

    expect(commandsTo('session.pairing.generate')).toHaveLength(1)
    expect(wrapper.text()).toContain('123456')
    // 倒计时徽标 = 剩余秒 + 秒文案 key
    expect(wrapper.text()).toContain('60pairing.time.seconds')
  })

  it('C3 生成失败（回执无码）：以失败文案提示，不静默', async () => {
    const wrapper = mountView()
    await flushPromises()

    execute.mockImplementation(async (command: string) => {
      if (command === 'session.pairing.generate') return null
      return defaultExecute(command)
    })
    const { toast } = await import('vue-sonner')

    await findButton(wrapper, 'pairing.code.generate')!.trigger('click')
    await flushPromises()

    expect(toast.error).toHaveBeenCalledWith('pairing.code.generateFailedNoCode')
  })

  it('C4 取消配对码：调 session.pairing.clear 且码从页面消失', async () => {
    const wrapper = mountView()
    await flushPromises()
    await findButton(wrapper, 'pairing.code.generate')!.trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('123456')

    await findButton(wrapper, 'pairing.button.cancel')!.trigger('click')
    await flushPromises()

    expect(commandsTo('session.pairing.clear')).toHaveLength(1)
    expect(wrapper.text()).not.toContain('123456')
  })

  it('C5 生成 QR：命令带当前选中 host，且载荷渲染进 canvas', async () => {
    const wrapper = mountView()
    await flushPromises()

    await findButton(wrapper, 'pairing.qr.generate')!.trigger('click')
    await flushPromises()

    // 未持久化过 host → 自动选第一个可用 IPv4（与宿主原页同口径）
    expect(commandsTo('session.qr.generate')[0][1]).toEqual({ host: seed.network.addresses[0] })
    expect(wrapper.find('canvas').exists()).toBe(true)

    const QRCode = (await import('qrcode')).default
    expect(QRCode.toCanvas).toHaveBeenCalled()
    const payload = JSON.parse((QRCode.toCanvas as any).mock.calls[0][1])
    expect(payload).toEqual({
      host: seed.qr.host,
      port: seed.qr.port,
      token: seed.qr.token,
    })
  })

  it('C5b 已有活跃 QR：进入页面恢复展示（不重新生成 token）', async () => {
    execute.mockImplementation(async (command: string) => {
      if (command === 'session.qr.info') return seed.qr
      return defaultExecute(command)
    })
    const wrapper = mountView()
    await flushPromises()

    expect(commandsTo('session.qr.info')).toHaveLength(1)
    expect(commandsTo('session.qr.generate')).toHaveLength(0)
    expect(wrapper.find('canvas').exists()).toBe(true)
  })

  it('C6/C7 在线与离线判定由后端事件驱动（同一台设备在两个分区之间迁移）', async () => {
    const wrapper = mountView()
    await flushPromises()
    await findButton(wrapper, 'pairing.tab.devices')!.trigger('click')
    await flushPromises()

    // 初始：无在线连接 → 两台设备都在离线区
    const offlineSection = () => wrapper.findAll('section')[1].text()
    expect(offlineSection()).toContain('Pixel 9')

    await fireEvent('device-connected', { fingerprint: 'fp-pixel-9' })
    const onlineSection = () => wrapper.findAll('section')[0].text()
    expect(onlineSection()).toContain('Pixel 9')
    expect(offlineSection()).not.toContain('Pixel 9')
    expect(onlineSection()).toContain('pairing.device.connected')

    await fireEvent('device-disconnected', { fingerprint: 'fp-pixel-9' })
    expect(offlineSection()).toContain('Pixel 9')
    expect(onlineSection()).not.toContain('Pixel 9')
  })

  it('C8 设备接入：清除已使用的配对码并重新取设备列表', async () => {
    const wrapper = mountView()
    await flushPromises()
    await findButton(wrapper, 'pairing.code.generate')!.trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('123456')

    await fireEvent('device-connected', { fingerprint: 'fp-new' })

    expect(commandsTo('session.pairing.clear')).toHaveLength(1)
    expect(wrapper.text()).not.toContain('123456')
    // onMounted 一次 + 事件驱动一次
    expect(commandsTo('session.devices.paired-list')).toHaveLength(2)
  })

  it('C9 撤销设备：确认后调 session.devices.revoke 并重新取列表', async () => {
    const wrapper = mountView()
    await flushPromises()
    await findButton(wrapper, 'pairing.tab.devices')!.trigger('click')
    await flushPromises()

    // 第一台设备的「移除」按钮
    await findButton(wrapper, 'pairing.button.remove')!.trigger('click')
    await flushPromises()

    // 确认弹窗内的「移除」（危险按钮，与卡片内按钮同文案）
    const confirm = wrapper
      .findAll('button')
      .filter((b) => b.text() === 'pairing.button.remove')
      .find((b) => b.element.closest('.fixed') !== null)
    expect(confirm, '撤销确认弹窗必须出现').toBeTruthy()
    await confirm!.trigger('click')
    await flushPromises()

    expect(commandsTo('session.devices.revoke')[0][1]).toEqual({
      id: seed.pairedDevices[0].id,
    })
    expect(commandsTo('session.devices.paired-list')).toHaveLength(2)
    const { toast } = await import('vue-sonner')
    expect(toast.success).toHaveBeenCalledWith('pairing.device.removed')
  })

  it('C10 撤销失败：以错误文案提示且不重复取列表（不留「已删除」假象）', async () => {
    const wrapper = mountView()
    await flushPromises()
    await findButton(wrapper, 'pairing.tab.devices')!.trigger('click')
    await flushPromises()

    execute.mockImplementation(async (command: string) => {
      if (command === 'session.devices.revoke') throw new Error('revoke failed')
      return defaultExecute(command)
    })
    const { toast } = await import('vue-sonner')

    await findButton(wrapper, 'pairing.button.remove')!.trigger('click')
    await flushPromises()
    const confirm = wrapper
      .findAll('button')
      .filter((b) => b.text() === 'pairing.button.remove')
      .find((b) => b.element.closest('.fixed') !== null)
    await confirm!.trigger('click')
    await flushPromises()

    expect(toast.error).toHaveBeenCalledWith('pairing.error.revokeFailed')
    expect(toast.success).not.toHaveBeenCalled()
    expect(commandsTo('session.devices.paired-list')).toHaveLength(1)
  })

  it('C11 查看历史：跳转同插件历史目录并带上设备 id', async () => {
    const wrapper = mountView()
    await flushPromises()
    await findButton(wrapper, 'pairing.tab.devices')!.trigger('click')
    await flushPromises()

    await findButton(wrapper, 'pairing.device.historyView')!.trigger('click')
    await flushPromises()

    expect(routerPush).toHaveBeenCalledWith({
      name: 'plugin-sidebar-view',
      params: { pluginId: 'com.bedcode.session', viewId: 'session.history' },
      query: { deviceId: seed.pairedDevices[0].id },
    })
  })

  it('C12 移动端请求配对：后端事件展示码并以请求文案提示', async () => {
    const wrapper = mountView()
    await flushPromises()
    const { toast } = await import('vue-sonner')

    await fireEvent('pairing-code-generated', { code: '654321', expires_in: 90 })

    expect(wrapper.text()).toContain('654321')
    expect(wrapper.text()).toContain('90pairing.time.seconds')
    expect(toast.info).toHaveBeenCalledWith('pairing.code.request')
  })

  it('C13 配对码倒计时归零：自动清除后端状态并从页面消失（防过期码复用）', async () => {
    vi.useFakeTimers()
    try {
      execute.mockImplementation(async (command: string) => {
        if (command === 'session.pairing.generate') {
          return { code: '111111', created_at: '2026-09-20T05:00:00Z', expires_in: 1 }
        }
        return defaultExecute(command)
      })
      const wrapper = mountView()
      await vi.advanceTimersByTimeAsync(0)

      await findButton(wrapper, 'pairing.code.generate')!.trigger('click')
      await vi.advanceTimersByTimeAsync(0)
      // 正向断言先行：生成后确实展示（避免恒真断言）
      expect(wrapper.text()).toContain('111111')

      await vi.advanceTimersByTimeAsync(2000)

      expect(commandsTo('session.pairing.clear')).toHaveLength(1)
      expect(wrapper.text()).not.toContain('111111')
    } finally {
      vi.useRealTimers()
    }
  })
})
