/**
 * 配对流组合集成测试（L2 场景 1）
 *
 * 协作实体：usePairing（composable） + useDeviceStore（真实 Pinia store） +
 * useSettingsStore（端口回显） + DevicesView（配对视图组件挂载）。
 *
 * 覆盖用户路径：生成配对码 → 界面展示（码 + 倒计时） → 移动端接入
 * （device-connected 事件驱动状态流转：配对码自动清除 + 设备列表刷新） →
 * 配对码过期自动清除。
 *
 * 测试 seam（与 useServer.test.ts 同模式）：
 * - 只 mock @tauri-apps/api 边界：core.invoke + event.listen（按事件名捕获回调，
 *   测试内手动触发模拟后端事件推送）
 * - Pinia / vue-router / i18n / composables / store 内部逻辑全部真实执行
 * - fixture 数据全部取自工厂（makePairing / makePairingCodeInfo /
 *   makeDeviceConnectionInfo / makeAppConfig）
 *
 * 环境限制说明：
 * - QR 码路径（qr.restoreQr / generateQr）在本测试中保持 null——happy-dom 的
 *   canvas 无 2d context，qrcode 库渲染会抛错；QR 不属于本场景（配对码）范围，
 *   用 mock invoke 返回 null 绕开即可，不影响被测链路
 * - 异步推进统一用 flushAsync：setTimeout(0) 只在微任务队列排空后触发，mock
 *   invoke 的纯微任务 async 链一次调用即可全部推进；fake timers 下改用
 *   advanceTimersByTimeAsync(0)（flushPromises 的 setImmediate 会被伪造挂起）
 * - usePairing 的 pairingCode 为每实例 ref，测试从视图实例（wrapper.vm 的
 *   setup 绑定）读取视图内同一 composable 实例的状态
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createRouter, createMemoryHistory } from 'vue-router'
import i18n from '@/locales'
import DevicesView from '@/views/DevicesView.vue'
import { usePairing } from '@/composables/usePairing'
import { useDeviceStore } from '@/stores/device'
import { useSettingsStore } from '@/stores/settings'
import {
  makePairing,
  makePairingCodeInfo,
  makeDeviceConnectionInfo,
  makeAppConfig,
} from '@/__tests__/fixtures/index'

// ==================== mock Tauri 边界 ====================

const mockInvoke = vi.fn()

// event.listen 按事件名捕获回调：测试内手动触发，模拟后端事件推送
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
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: any[]) => mockListen(...args),
}))

// ==================== 测试基建 ====================

/** 后端状态（可变的模拟 DB） */
let pairedDevices: any[]
/** 下一次 generate_pairing_code 的返回（过期场景按用例改写） */
let generatedCode: ReturnType<typeof makePairingCodeInfo>

/** invoke 分发：配对流涉及的 8 个命令 */
function installInvokeMock() {
  mockInvoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case 'get_local_ip_addresses':
        // 空 IP 列表 → 视图不触发 qr_host 自动补写保存，保持链路最小
        return Promise.resolve([])
      case 'get_app_settings':
        return Promise.resolve(makeAppConfig({ network: { ...makeAppConfig().network, port: 9000 } }))
      case 'list_paired_devices':
        return Promise.resolve([...pairedDevices])
      case 'get_connected_devices':
        return Promise.resolve([makeDeviceConnectionInfo({ device_id: 'device-1', addr: '192.168.1.50', fingerprint: 'fp-1' })])
      case 'get_current_pairing_code':
        return Promise.resolve(null)
      case 'get_qr_connection_info':
        // happy-dom canvas 无 2d context，QR 渲染不可用；保持 QR 为 null 绕开
        return Promise.resolve(null)
      case 'generate_pairing_code':
        return Promise.resolve(generatedCode)
      case 'clear_pairing_code':
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
 * async 链（onMounted 多级 await）一次调用即可全部推进；fake timers 下
 * setTimeout 被伪造，用 advanceTimersByTimeAsync(0) 等价推进
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
    routes: [{ path: '/devices/:id/history', component: { template: '<div />' } }],
  })
}

let wrapper: ReturnType<typeof mount> | null = null
let pinia: ReturnType<typeof createPinia>

beforeEach(() => {
  vi.clearAllMocks()
  // 测试与挂载共享同一 pinia：测试内 useDeviceStore/useSettingsStore 拿到的
  // 就是视图内部消费的同一 store 实例（否则断言的是另一份状态）
  pinia = createPinia()
  setActivePinia(pinia)
  pairedDevices = [makePairing({ id: 'device-1', deviceName: 'Phone 1', deviceFingerprint: 'fp-1', address: '192.168.1.50' })]
  generatedCode = makePairingCodeInfo({ code: '654321', expires_in: 60 })
  installInvokeMock()
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
  vi.useRealTimers()
  // 清空事件捕获，避免跨用例残留
  for (const k of Object.keys(eventHandlers)) delete eventHandlers[k]
})

async function mountView() {
  wrapper = mount(DevicesView, {
    global: {
      plugins: [pinia, makeRouter(), i18n],
    },
  })
  await flushAsync()
}

/** 配对卡内的「生成配对码」按钮（工具栏与卡片各一个，取卡片内的） */
function pairingCardButton() {
  return wrapper!
    .findAll('button')
    .filter((b) => b.text().includes('生成配对码'))
    .pop()!
}

// ==================== 场景 ====================

describe('配对流：usePairing × useDeviceStore × DevicesView', () => {
  it('初始加载：设置端口回显 + 已配对设备按在线指纹联动渲染为「在线」', async () => {
    // 模拟应用启动流程：main.ts 在挂载视图前加载设置（共享 pinia 同一实例）
    const settingsStore = useSettingsStore()
    await settingsStore.loadSettings()
    await flushAsync()
    await mountView()

    // useSettingsStore（get_app_settings 端口 9000）→ 视图网络条端口回显
    expect(settingsStore.settings.network.port).toBe(9000)
    expect(wrapper!.text()).toContain(':9000')

    // useDeviceStore.loadPairedDevices + useConnectedDevices 的指纹集合协作：
    // fp-1 在已配对列表与在线连接中同时出现 → 在线分区渲染（设备列表 Tab）
    await wrapper!
      .findAll('button')
      .find((b) => b.text().trim() === '设备列表')!
      .trigger('click')
    await flushAsync()
    const deviceStore = useDeviceStore()
    expect(deviceStore.pairedDevices).toHaveLength(1)
    expect(wrapper!.text()).toContain('Phone 1')
    expect(wrapper!.text()).toContain('已连接')
    expect(wrapper!.text()).toContain('在线')
  })

  it('生成配对码 → usePairing 状态与界面展示联动（码 + 倒计时）', async () => {
    await mountView()
    const pairing = usePairing()

    await pairingCardButton().trigger('click')
    await flushAsync()

    expect(invokeCalls('generate_pairing_code')).toHaveLength(1)
    // usePairing（composable）与视图 ref 双向一致：pairingCode 为每实例 ref，
    // 经视图实例（wrapper.vm 的 setup 绑定）读取视图内同一 composable 实例
    const viewPairing = (wrapper!.vm as unknown as { pairing: ReturnType<typeof usePairing> }).pairing
    expect(viewPairing.pairingCode.value?.code).toBe('654321')
    // 界面展示：大字配对码 + 倒计时徽标
    expect(wrapper!.text()).toContain('654321')
    expect(wrapper!.text()).toContain('60秒')
  })

  it('移动端接入（device-connected 事件）→ 配对码自动清除 + 设备列表刷新联动', async () => {
    await mountView()
    const deviceStore = useDeviceStore()
    const viewPairing = (wrapper!.vm as unknown as { pairing: ReturnType<typeof usePairing> }).pairing

    // 先配对成功展示码
    await pairingCardButton().trigger('click')
    await flushAsync()
    expect(viewPairing.pairingCode.value?.code).toBe('654321')
    expect(wrapper!.text()).toContain('654321')

    // 后端推送 device-connected：模拟第二台设备完成配对
    pairedDevices = [
      makePairing({ id: 'device-1', deviceName: 'Phone 1', deviceFingerprint: 'fp-1', address: '192.168.1.50' }),
      makePairing({ id: 'device-2', deviceName: 'Phone 2', deviceFingerprint: 'fp-2', address: '192.168.1.51' }),
    ]
    for (const handler of eventHandlers['device-connected'] || []) {
      await handler({ payload: { fingerprint: 'fp-1', device_id: 'device-1', addr: '192.168.1.50', session_count: 1 } })
    }
    await flushAsync()

    // 状态流转断言：
    // 1. 配对码已清除（视图内 composable 实例 + 界面）——配对完成即失效，防止复用
    expect(invokeCalls('clear_pairing_code')).toHaveLength(1)
    expect(viewPairing.pairingCode.value).toBeNull()
    expect(wrapper!.text()).not.toContain('654321')
    // 2. 设备列表经 store 重载后渲染两设备。list_paired_devices 精确 3 次：
    //    onMounted 直接加载 1 次 + refreshDevices 内加载 1 次 + device-connected
    //    重载 1 次（弱界断言曾掩盖 mount 时列表加载两次的冗余调用，锁精确值）
    expect(invokeCalls('list_paired_devices')).toHaveLength(3)
    expect(deviceStore.pairedDevices).toHaveLength(2)
    await wrapper!
      .findAll('button')
      .find((b) => b.text().trim() === '设备列表')!
      .trigger('click')
    await flushAsync()
    expect(wrapper!.text()).toContain('Phone 2')
  })

  it('配对码过期（倒计时归零）→ 后端清除 + 界面与 composable 同步置空', async () => {
    // 倒计时依赖 setInterval：先切 fake timers 再触发生成，才能推进时间
    vi.useFakeTimers()
    // 1 秒有效期：advance 2s 后必然越过过期点
    generatedCode = makePairingCodeInfo({ code: '123456', expires_in: 1 })
    await mountView()

    // 正向断言先行：生成后视图内实例与界面均已展示配对码（防恒真断言）
    const viewPairing = (wrapper!.vm as unknown as { pairing: ReturnType<typeof usePairing> }).pairing
    await pairingCardButton().trigger('click')
    await flushAsync() // fake timers 下推进 invoke 异步链，generate 完成后 pairingCode 才落地
    expect(viewPairing.pairingCode.value).not.toBeNull()
    expect(wrapper!.text()).toContain('123456')

    await vi.advanceTimersByTimeAsync(2000)

    // 过期后：倒计时归零触发一次后端清除，视图内实例与界面同步置空
    expect(viewPairing.pairingCode.value).toBeNull()
    expect(wrapper!.text()).not.toContain('123456')
    expect(invokeCalls('clear_pairing_code')).toHaveLength(1)
  })
})
