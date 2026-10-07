/**
 * DevicesView 连接页：连接历史显隐 + 扫描发现区切换/返回行为契约
 *
 * 被测契约（来源：用户需求「mDNS 发现设备时连接历史为空就不显示连接历史」+「优化扫描显示
 * 切换 / 返回连接页面的体验」）：
 * - C-101 历史为空 → 整个历史区块（标题/条数/清除/空态文案）不渲染，底部入口仍在
 * - C-102 历史非空 → 标题 + 条数 + 清除入口 + 每条地址都渲染
 * - C-103 历史为空但有扫码结果 → 只渲染扫码结果卡片（不出现历史标题）
 * - C-104 打开扫描发现区 → 底部两枚 CTA 不变（扫码/手动常驻），扫描控制出现在面板头部
 * - C-105 打开发现区触发 startDiscovery；面板 × 关闭触发 stopDiscovery 且面板消失
 * - C-106 连接成功 → 收起发现区并停扫描（只收起会让 mDNS 继续跑）
 * - C-107 从 keep-alive 页返回且发现区仍展开 → 续扫带 keepResults（不清空已发现设备）
 * - C-109 模式切换 → 内容区滚动位置复位到顶部
 *
 * 替身边界：只 mock 跨进程/全局单例依赖（mDNS composable、连接 composable、Tauri 命令、
 * i18n、相机库）；被测的视图分支与切换逻辑全部走真实渲染。
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { ref, defineComponent, nextTick } from 'vue'
import { mount } from '@vue/test-utils'

// ==================== 替身 ====================

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (k: string) => k }),
}))

vi.mock('@/utils/frontendLogger', () => ({
  logger: { log: vi.fn(), debug: vi.fn(), warn: vi.fn(), error: vi.fn() },
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({ error: vi.fn(), warning: vi.fn(), success: vi.fn(), info: vi.fn() }),
}))

vi.mock('@/composables/useMobileCommands', () => ({
  wsGetBiometricKeyStatus: vi.fn(async () => ({ deviceSupported: false, hasKey: false })),
  wsAuthenticateWithQr: vi.fn(async () => null),
}))

/** 相机库：ScanPanel 在本用例中被 stub，模块本身仍需可导入 */
vi.mock('html5-qrcode', () => ({ Html5Qrcode: class {} }))

// 被测视图依赖的全局单例状态（用例内可变，用来模拟连接/扫描状态变化）
const connectionHistory = ref<Array<{ address: string; name?: string; lastConnected?: string }>>([])
const connectionStatus = ref<'disconnected' | 'connected' | 'paired'>('disconnected')
const isConnected = ref(false)
const discoveredServices = ref<any[]>([])
const isScanning = ref(false)
const startDiscovery = vi.fn(async () => {})
const stopDiscovery = vi.fn(async () => {})
const loadConnectionHistory = vi.fn()

vi.mock('@/composables/useMobileConnection', () => ({
  useMobileConnection: () => ({
    connectionHistory,
    connectionStatus,
    isConnected,
    currentDevice: ref({ id: '1', name: 'Desk', address: '10.0.0.5', port: 8765, isPaired: true }),
    activeSessionId: ref<string | null>(null),
    isConnecting: ref(false),
    loadConnectionHistory,
    loadSessionConfigs: vi.fn(async () => {}),
    loadActiveSessions: vi.fn(async () => {}),
    clearSessionConfigs: vi.fn(),
    clearActiveSessions: vi.fn(),
    connect: vi.fn(async () => {}),
    authenticate: vi.fn(async () => false),
    authenticateWithBiometric: vi.fn(async () => false),
    requestPairing: vi.fn(async () => {}),
    verifyPairingCode: vi.fn(async () => false),
    cancelConnection: vi.fn(async () => {}),
    disconnect: vi.fn(async () => {}),
    saveCredentials: vi.fn(),
    addToConnectionHistory: vi.fn(),
    removeFromConnectionHistory: vi.fn(),
    clearConnectionHistory: vi.fn(),
  }),
}))

vi.mock('@/composables/useMobileSettings', () => ({
  useMobileSettings: () => ({ settings: ref({ defaultPort: 8765, preferredAuthMethod: 'pairing' }) }),
}))

vi.mock('@/composables/useMdnsDiscovery', () => ({
  useMdnsDiscovery: () => ({ discoveredServices, isScanning, startDiscovery, stopDiscovery }),
}))

import DevicesView from '@/views/DevicesView.vue'

// ==================== 测试基建 ====================

const RADAR = 'mobile.connection.discoverDevices'
const CLOSE = 'common.button.close'
const HISTORY_TITLE = 'mobile.connection.connectionHistory'
const SCAN_CONNECT = 'mobile.connection.scanConnect'
const MANUAL_CONNECT = 'mobile.connection.manualConnect'
const STOP_SCAN = 'mobile.discover.stopScan'
const RESTART_SCAN = 'mobile.discover.restartScan'

/** 挂载连接页（ScanPanel 依赖相机，测试只关心页面自身的模式切换，用 stub 占位） */
function mountDevices() {
  return mount(DevicesView, {
    global: {
      stubs: {
        ScanPanel: {
          name: 'ScanPanel',
          emits: ['scan-result'],
          template: '<div data-testid="scan-panel-stub" />',
        },
      },
    },
  })
}

/** 按渲染文本（i18n key 直出）定位按钮 */
function buttonByText(wrapper: ReturnType<typeof mountDevices>, text: string) {
  return wrapper.findAll('button').find(b => b.text() === text)
}

/** 页头雷达按钮（唯一带 discoverDevices title 的按钮） */
function radarButton(wrapper: ReturnType<typeof mountDevices>) {
  return wrapper.find(`button[title="${RADAR}"]`)
}

/** 内容区滚动容器 */
function scrollContainer(wrapper: ReturnType<typeof mountDevices>) {
  return wrapper.find('.scrollbar-gutter-stable').element as HTMLElement
}

/** 展开扫描发现区并等待过渡后的 DOM 稳定 */
async function openDiscovery(wrapper: ReturnType<typeof mountDevices>) {
  await radarButton(wrapper).trigger('click')
  await nextTick()
}

/** 复位全局单例到初始态（用例间无顺序依赖） */
function resetState() {
  connectionHistory.value = []
  connectionStatus.value = 'disconnected'
  isConnected.value = false
  discoveredServices.value = []
  isScanning.value = false
  startDiscovery.mockClear()
  stopDiscovery.mockClear()
  loadConnectionHistory.mockClear()
}

beforeEach(resetState)

// ==================== C-101 / C-102 连接历史显隐 ====================

describe('连接历史显隐', () => {
  it('should_不渲染历史区块_when_历史为空', async () => {
    const wrapper = mountDevices()

    // 反例面：空历史既不出现标题，也不出现「暂无连接历史」空态文案
    expect(wrapper.text()).not.toContain(HISTORY_TITLE)
    expect(wrapper.text()).not.toContain('mobile.connection.noHistory')
    expect(wrapper.find('.config-list').exists()).toBe(false)

    // 隐藏历史后底部入口仍在（不是空白页）
    expect(wrapper.text()).toContain(SCAN_CONNECT)
    expect(wrapper.text()).toContain(MANUAL_CONNECT)
  })

  it('should_渲染历史区块_when_历史非空', async () => {
    connectionHistory.value = [
      { address: '10.0.0.5:8765', name: 'Desk A', lastConnected: new Date().toISOString() },
      { address: '10.0.0.6:8765', name: 'Desk B' },
    ]
    const wrapper = mountDevices()

    expect(wrapper.text()).toContain(HISTORY_TITLE)
    expect(wrapper.text()).toContain('mobile.connection.clearHistory')
    expect(wrapper.text()).toContain('Desk A')
    expect(wrapper.text()).toContain('10.0.0.6:8765')
    // 条数徽标：历史条数原样渲染
    expect(wrapper.text()).toContain('2')
  })

  it('should_只渲染扫码结果_when_历史为空但已有扫码结果', async () => {
    // 通过 ScanPanel 的 scan-result 通道造扫码结果（真实交互路径）
    const wrapper = mountDevices()
    await buttonByText(wrapper, SCAN_CONNECT)!.trigger('click')
    await nextTick()
    wrapper.findComponent({ name: 'ScanPanel' }).vm.$emit('scan-result', {
      host: '10.0.0.9',
      port: 8765,
      token: 'tok',
    })
    await nextTick()
    await nextTick()

    expect(wrapper.text()).toContain('mobile.scan.scanResult')
    expect(wrapper.text()).toContain('10.0.0.9:8765')
    expect(wrapper.text()).not.toContain(HISTORY_TITLE)
  })
})

// ==================== C-104 / C-105 扫描发现区切换 ====================

describe('扫描发现区切换', () => {
  it('should_底部CTA保持常驻并把扫描控制放进面板_when_打开发现区', async () => {
    const wrapper = mountDevices()
    await openDiscovery(wrapper)

    // 底部两枚 CTA 不变（不再随 mDNS 开关换布局）
    expect(wrapper.text()).toContain(SCAN_CONNECT)
    expect(wrapper.text()).toContain(MANUAL_CONNECT)
    // 扫描控制出现在面板头部：未扫描 → 重新扫描；扫描中 → 停止扫描
    expect(buttonByText(wrapper, RESTART_SCAN)).toBeTruthy()
    expect(buttonByText(wrapper, STOP_SCAN)).toBeUndefined()
  })

  it('should_停止扫描_when_点面板关闭按钮', async () => {
    const wrapper = mountDevices()
    await openDiscovery(wrapper)
    // 前置自检：面板确实已展开（否则下面的「消失」断言是恒真的）
    expect(wrapper.text()).toContain('mobile.discover.title')
    expect(startDiscovery).toHaveBeenCalledTimes(1)
    stopDiscovery.mockClear()

    await wrapper.find(`button[title="${CLOSE}"]`).trigger('click')
    await nextTick()

    expect(stopDiscovery).toHaveBeenCalledTimes(1)
    expect(wrapper.text()).not.toContain('mobile.discover.title')
  })

  it('should_展示已发现设备_when_发现区展开', async () => {
    discoveredServices.value = [
      {
        instance_name: 'desk-a',
        host_name: 'desk-a.local',
        address: '10.0.0.7',
        port: 8765,
        txt_records: {},
        platform: 'desktop',
        device_name: 'Desk A',
      },
    ]
    isScanning.value = true
    const wrapper = mountDevices()
    await openDiscovery(wrapper)

    expect(wrapper.text()).toContain('Desk A')
    expect(wrapper.text()).toContain('10.0.0.7:8765')
    // 扫描控制按当前扫描态显示「停止扫描」
    expect(buttonByText(wrapper, STOP_SCAN)).toBeTruthy()
    expect(buttonByText(wrapper, RESTART_SCAN)).toBeUndefined()
  })

  it('should_停止mDNS扫描_when_切到二维码扫描', async () => {
    const wrapper = mountDevices()
    await openDiscovery(wrapper)
    expect(wrapper.text()).toContain('mobile.discover.title')
    stopDiscovery.mockClear()

    await buttonByText(wrapper, SCAN_CONNECT)!.trigger('click')
    await nextTick()

    // 二维码与发现区互斥：切走必须停掉 mDNS，否则后台继续扫
    expect(stopDiscovery).toHaveBeenCalledTimes(1)
    expect(wrapper.text()).not.toContain('mobile.discover.title')
    expect(wrapper.find('[data-testid="scan-panel-stub"]').exists()).toBe(true)
  })
})

// ==================== C-106 连接成功 ====================

describe('连接成功后收起发现区', () => {
  it('should_收起发现区并停止扫描_when_连接成功', async () => {
    const wrapper = mountDevices()
    await openDiscovery(wrapper)
    expect(wrapper.text()).toContain('mobile.discover.title')

    isConnected.value = true
    connectionStatus.value = 'connected'
    await nextTick()

    expect(wrapper.text()).not.toContain('mobile.discover.title')
    expect(stopDiscovery).toHaveBeenCalled()
  })
})

// ==================== C-107 返回连接页 ====================

describe('keep-alive 返回连接页', () => {
  it('should_保留已发现设备续扫_when_返回且发现区仍展开', async () => {
    // KeepAlive 宿主：停用/激活用 v-if 切换，触发视图的 onDeactivated/onActivated
    const mounted = ref(true)
    const Host = defineComponent({
      components: { DevicesView },
      setup: () => ({ mounted }),
      template: '<KeepAlive><DevicesView v-if="mounted" /></KeepAlive>',
    })
    const host = mount(Host, {
      global: {
        stubs: { ScanPanel: { template: '<div />' } },
      },
    })

    // 展开发现区：先发生一次普通启动（不保留旧列表）
    const devices = host.findComponent(DevicesView)
    await devices.find(`button[title="${RADAR}"]`).trigger('click')
    await nextTick()
    expect(startDiscovery).toHaveBeenCalledWith(undefined)

    // 离开连接页（停用）→ 返回（激活）
    mounted.value = false
    await nextTick()
    mounted.value = true
    await nextTick()

    expect(stopDiscovery).toHaveBeenCalled()
    // 返回时续扫必须保留已发现列表：keepResults 为真，否则每次返回都清空重来
    expect(startDiscovery).toHaveBeenLastCalledWith({ keepResults: true })
  })
})

// ==================== C-109 模式切换复位滚动 ====================

describe('模式切换复位滚动', () => {
  it('should_滚动复位到顶部_when_切换扫描模式', async () => {
    connectionHistory.value = [{ address: '10.0.0.5:8765', name: 'Desk A' }]
    const wrapper = mountDevices()
    const scroller = scrollContainer(wrapper)

    // 前置自检：happy-dom 必须真的持有 scrollTop，否则下面的断言是恒真的
    scroller.scrollTop = 240
    expect(scroller.scrollTop).toBe(240)

    await openDiscovery(wrapper)

    expect(scroller.scrollTop).toBe(0)
  })
})