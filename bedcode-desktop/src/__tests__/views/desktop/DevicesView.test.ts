import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils'
import { createRouter, createWebHistory } from 'vue-router'
import { createPinia, setActivePinia } from 'pinia'
import i18n from '@/locales'
import DevicesView from '@/views/DevicesView.vue'
import { useDeviceStore } from '@/stores/device'

// Mock Tauri APIs
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === 'get_app_settings') {
      return Promise.resolve({
        network: { port: 8765 },
        session: { default_environment: 'windows', default_command: 'claude', session_timeout: 3600 },
        ui: { theme: 'system', terminal_font_size: 12, terminal_font_family: 'Consolas', terminal_theme: 'dracula', show_preview: true },
      })
    }
    return Promise.resolve(undefined)
  }),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

// Mock useDesktopCommands（device store / settings 的数据源）
vi.mock('@/composables/useDesktopCommands', () => ({
  listPairedDevices: vi.fn(async () => []),
  removePairedDevice: vi.fn(async () => {}),
  generatePairingCode: vi.fn(),
  verifyPairingCode: vi.fn(),
  generateQrCode: vi.fn(),
  clearQrCode: vi.fn(),
  getQrConnectionInfo: vi.fn(),
  getQrTokenTtl: vi.fn(),
  setQrTokenTtl: vi.fn(),
}))

// Mock composables
const pairingState = vi.hoisted(() => ({
  code: null as { code: string; expires_in: number; created_at: string } | null,
}))

vi.mock('@/composables/useTauri', () => ({
  usePairing: () => ({
    pairingCode: {
      get value() { return pairingState.code },
      set value(v) { pairingState.code = v },
    },
    generateCode: vi.fn(async () => {
      pairingState.code = { code: '123456', expires_in: 60, created_at: new Date().toISOString() }
    }),
    clearCode: vi.fn(async () => { pairingState.code = null }),
    checkCurrentCode: vi.fn().mockResolvedValue(false),
  }),
  useNetwork: () => ({
    localAddresses: { value: ['192.168.1.100'] },
    loadLocalAddresses: vi.fn().mockResolvedValue(undefined),
  }),
  useConnectedDevices: () => ({
    connectedDevices: { value: [] },
    loadConnectedDevices: vi.fn().mockResolvedValue(undefined),
  }),
}))

vi.mock('@/composables/useQrCode', () => ({
  useQrCode: () => ({
    qrData: { value: null },
    remainingSeconds: { value: 0 },
    isLoading: { value: false },
    isExpired: { value: true },
    hasQr: { value: false },
    generateQr: vi.fn(),
    restoreQr: vi.fn().mockResolvedValue(false),
    clearQr: vi.fn(),
  }),
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  }),
}))

vi.mock('qrcode', () => ({
  default: {
    toCanvas: vi.fn().mockResolvedValue(undefined),
  },
}))

// Mock components
vi.mock('@/components/Button.vue', () => ({
  default: {
    template: '<button @click="$emit(\'click\')"><slot /><slot name="icon" /></button>',
    props: ['variant', 'size', 'loading', 'disabled'],
  },
}))

vi.mock('@/components/Modal.vue', () => ({
  default: {
    template: '<div v-if="modelValue" class="modal"><slot /><slot name="footer" /></div>',
    props: ['modelValue', 'title', 'size'],
  },
}))

const mockRouter = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', component: { template: '<div />' } },
    { path: '/devices', component: { template: '<div />' } },
    { path: '/devices/:id/history', component: { template: '<div />' } },
  ],
})

function mountView() {
  const pinia = createPinia()
  setActivePinia(pinia)
  const wrapper = mount(DevicesView, {
    global: {
      plugins: [mockRouter, pinia, i18n],
    },
  })
  return { wrapper, pinia }
}

/** 切换到"设备列表"tab（在线/离线设备列表） */
async function switchToDevicesTab(wrapper: VueWrapper) {
  const btn = wrapper.findAll('button').find((b) => b.text().includes('设备列表'))
  expect(btn).toBeDefined()
  await btn!.trigger('click')
  await flushPromises()
}

describe('DevicesView', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    pairingState.code = null
  })

  it('should render header with title', async () => {
    const { wrapper } = mountView()
    await flushPromises()

    expect(wrapper.find('.wb-toolbar').exists()).toBe(true)
    expect(wrapper.find('h2').text()).toContain('设备配对')
  })

  it('should show QR connection section', async () => {
    const { wrapper } = mountView()
    await flushPromises()

    expect(wrapper.text()).toContain('QR 码连接')
  })

  it('should show pairing section with generate button', async () => {
    const { wrapper } = mountView()
    await flushPromises()

    expect(wrapper.text()).toContain('配对码连接')
    expect(wrapper.text()).toContain('生成配对码')
  })

  it('should show pairing section with ip/port selector', async () => {
    const { wrapper } = mountView()
    await flushPromises()

    // 自动选择首个 IPv4 并展示端口
    expect(wrapper.text()).toContain('设备配对')
    expect(wrapper.text()).toContain('192.168.1.100')
    expect(wrapper.text()).toContain(':8765')
  })

  it('should show empty paired devices state', async () => {
    const { wrapper } = mountView()
    await flushPromises()

    // 设备列表在"设备列表"tab 中，先切换过去
    await switchToDevicesTab(wrapper)

    expect(wrapper.text()).toContain('在线 · 0')
    expect(wrapper.text()).toContain('暂无数据')
  })

  it('should display pairing code when generated', async () => {
    const { wrapper } = mountView()
    await flushPromises()

    // 点击"生成配对码"按钮
    const generateBtn = wrapper.findAll('button').find((b) => b.text().includes('生成配对码'))
    expect(generateBtn).toBeDefined()

    await generateBtn!.trigger('click')
    await flushPromises()

    // 配对码应显示在页面中
    expect(wrapper.text()).toContain('123456')
    expect(wrapper.text()).toContain('60')

    wrapper.unmount()
  })

  it('should cancel pairing when cancel button clicked', async () => {
    const { wrapper } = mountView()
    await flushPromises()

    const generateBtn = wrapper.findAll('button').find((b) => b.text().includes('生成配对码'))
    await generateBtn!.trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('123456')

    // 点击取消按钮清除配对码
    const cancelBtn = wrapper.findAll('button').find((b) => b.text().includes('取消'))
    expect(cancelBtn).toBeDefined()
    await cancelBtn!.trigger('click')
    await flushPromises()

    expect(wrapper.text()).not.toContain('123456')

    wrapper.unmount()
  })

  it('should render paired devices when available', async () => {
    const { wrapper, pinia } = mountView()
    await flushPromises()

    // 设备列表在"设备列表"tab 中，先切换过去
    await switchToDevicesTab(wrapper)

    const deviceStore = useDeviceStore(pinia)
    deviceStore.pairedDevices = [
      {
        id: 'device-1',
        deviceName: 'My Phone',
        deviceFingerprint: 'fp123',
        address: '192.168.1.5',
        pairedAt: new Date().toISOString(),
        lastSeen: undefined,
        connectCount: 3,
      },
    ]

    await flushPromises()
    wrapper.vm.$forceUpdate()
    await flushPromises()

    expect(wrapper.text()).toContain('My Phone')
  })

  it('should open remove device dialog and confirm removal', async () => {
    const { wrapper, pinia } = mountView()
    await flushPromises()

    // 设备列表在"设备列表"tab 中，先切换过去
    await switchToDevicesTab(wrapper)

    const deviceStore = useDeviceStore(pinia)
    deviceStore.pairedDevices = [
      {
        id: 'device-1',
        deviceName: 'My Phone',
        deviceFingerprint: 'fp123',
        address: '192.168.1.5',
        pairedAt: new Date().toISOString(),
        lastSeen: undefined,
        connectCount: 0,
      },
    ]
    await flushPromises()
    wrapper.vm.$forceUpdate()
    await flushPromises()

    // 设备行的"移除"按钮（带文本）
    const removeRowBtn = wrapper.findAll('button').find((b) => b.text().includes('移除'))
    expect(removeRowBtn).toBeDefined()
    await removeRowBtn!.trigger('click')
    await flushPromises()

    // 确认移除对话框打开
    expect(wrapper.find('.modal').exists()).toBe(true)

    // 点击确认移除（modal 内的"移除"按钮）
    const commands = await import('@/composables/useDesktopCommands')
    const confirmBtn = wrapper.findAll('.modal button').find((b) => b.text().includes('移除'))
    expect(confirmBtn).toBeDefined()
    await confirmBtn!.trigger('click')
    await flushPromises()

    expect(commands.removePairedDevice).toHaveBeenCalledWith('device-1')
    expect(wrapper.find('.modal').exists()).toBe(false)
  })
})
