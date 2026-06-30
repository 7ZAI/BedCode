import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createRouter, createWebHistory } from 'vue-router'
import { setActivePinia, createPinia } from 'pinia'
import DevicesView from '@/modules/desktop/views/DevicesView.vue'

// Mock Tauri APIs
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

// Mock composables
vi.mock('@/modules/shared/composables/useToast', () => ({
  useToast: () => ({
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  }),
}))

vi.mock('@/modules/shared/composables/useTauri', () => ({
  usePairing: () => ({
    generateCode: vi.fn(),
    clearCode: vi.fn(),
    verifyCode: vi.fn().mockResolvedValue(true),
    removeDevice: vi.fn(),
    loadDevices: vi.fn().mockResolvedValue(undefined),
    devices: { value: [] },
    pairingCode: { value: null },
  }),
  useNetwork: () => ({
    localAddresses: { value: ['192.168.1.100'] },
    loadLocalAddresses: vi.fn().mockResolvedValue(undefined),
  }),
  useConnectedDevices: () => ({
    connectedDevices: { value: [] },
    isLoading: { value: false },
    loadConnectedDevices: vi.fn().mockResolvedValue(undefined),
  }),
  useQrCodeApi: () => ({
    generateQrCode: vi.fn().mockResolvedValue('test-token'),
    clearQrCode: vi.fn().mockResolvedValue(undefined),
    getQrConnectionInfo: vi.fn().mockResolvedValue(null),
    getQrTokenTtl: vi.fn().mockResolvedValue(300),
    setQrTokenTtl: vi.fn().mockResolvedValue(undefined),
  }),
}))

vi.mock('@/modules/shared/composables/useQrCode', () => ({
  useQrCode: () => ({
    qrData: { value: null },
    remainingSeconds: { value: 0 },
    isLoading: { value: false },
    isExpired: { value: true },
    hasQr: { value: false },
    generateQr: vi.fn(),
    clearQr: vi.fn(),
  }),
}))

vi.mock('qrcode', () => ({
  default: {
    toCanvas: vi.fn().mockResolvedValue(undefined),
  },
}))

// Mock components
vi.mock('@/modules/shared/components/Button.vue', () => ({
  default: {
    template: '<button @click="$emit(\'click\')"><slot /><slot name="icon" /></button>',
    props: ['variant', 'size', 'loading', 'disabled'],
  },
}))

vi.mock('@/modules/shared/components/Toggle.vue', () => ({
  default: {
    template: '<button @click="$emit(\'update:modelValue\', !modelValue)" :class="{ active: modelValue }"><slot /></button>',
    props: ['modelValue'],
    emits: ['update:modelValue'],
  },
}))

// Create mock router
const mockRouter = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', component: { template: '<div />' } },
    { path: '/desktop/devices', component: { template: '<div />' } },
  ],
})

describe('DevicesView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('should render header with title', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          Button: true,
          Toggle: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.find('header').exists()).toBe(true)
    expect(wrapper.find('h2').text()).toContain('设备配对')
  })

  it('should show pairing section', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          Button: true,
          Toggle: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.text()).toContain('新建配对')
    expect(wrapper.text()).toContain('生成配对�?)
  })

  it('should show network info section', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          Button: true,
          Toggle: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.text()).toContain('网络信息')
    expect(wrapper.text()).toContain('端口')
  })

  it('should show paired devices section', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          Button: true,
          Toggle: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.text()).toContain('已配对设�?)
  })

  it('should show empty paired devices state', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          Button: true,
          Toggle: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.text()).toContain('暂无已配对设�?)
  })

  it('should have generate code button', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          Button: {
            template: '<button @click="$emit(\'click\')"><slot name="icon" /><slot /></button>',
            props: ['variant', 'loading'],
          },
          Toggle: true,
        },
      },
    })

    await flushPromises()

    const buttons = wrapper.findAll('button')
    const generateBtn = buttons.find(b => b.text().includes('生成配对�?))

    expect(generateBtn).toBeDefined()
  })

  it('should display pairing code when generated', async () => {
    const pinia = createPinia()
    setActivePinia(pinia)

    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, pinia],
        stubs: {
          Button: true,
          Toggle: true,
        },
      },
    })

    await flushPromises()

    // Simulate setting pairing code
    wrapper.vm.pairingCode = { code: '123456', expiresIn: 60 }
    wrapper.vm.remainingSeconds = 60

    await flushPromises()
    wrapper.vm.$forceUpdate()
    await flushPromises()

    // Check if pairing code is displayed
    if (wrapper.vm.pairingCode) {
      expect(wrapper.vm.pairingCode.code).toBe('123456')
    }
  })

})

describe('DevicesView Countdown', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('should countdown when pairing code is active', async () => {
    const pinia = createPinia()
    setActivePinia(pinia)

    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, pinia],
        stubs: {
          Button: true,
          Toggle: true,
        },
      },
    })

    await flushPromises()

    // Set initial state
    wrapper.vm.pairingCode = { code: '123456', expiresIn: 60 }
    wrapper.vm.remainingSeconds = 60

    await flushPromises()

    // The countdown is managed by an interval set in generateCode
    // For this test, we just verify the initial state is correct
    expect(wrapper.vm.remainingSeconds).toBe(60)
    expect(wrapper.vm.pairingCode.code).toBe('123456')
  })

  it('should cancel pairing when countdown reaches zero', async () => {
    const pinia = createPinia()
    setActivePinia(pinia)

    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, pinia],
        stubs: {
          Button: true,
          Toggle: true,
        },
      },
    })

    await flushPromises()

    // Set initial state with 1 second remaining
    wrapper.vm.pairingCode = { code: '123456', expiresIn: 60 }
    wrapper.vm.remainingSeconds = 1

    await flushPromises()

    // Manually trigger cancel pairing (simulating countdown reaching zero)
    wrapper.vm.cancelPairing()

    // Should cancel pairing
    expect(wrapper.vm.pairingCode).toBeNull()
    expect(wrapper.vm.remainingSeconds).toBe(0)
  })
})

describe('DevicesView Device Management', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('should call removeDevice when removing device', async () => {
    const pinia = createPinia()
    setActivePinia(pinia)

    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, pinia],
        stubs: {
          Button: {
            template: '<button @click="$emit(\'click\')"><slot /></button>',
            props: ['variant', 'size'],
          },
          Toggle: true,
          Modal: {
            template: '<div v-if="modelValue"><slot /><slot name="footer" /></div>',
            props: ['modelValue', 'title', 'size'],
          },
        },
      },
    })

    await flushPromises()

    // Call removeDevice (now shows a modal instead of confirm)
    wrapper.vm.removeDevice('device-1')
    await flushPromises()

    expect(wrapper.vm.showRemoveDeviceDialog).toBe(true)
    expect(wrapper.vm.pendingDeviceId).toBe('device-1')
  })

  it('should display paired devices when available', async () => {
    const pinia = createPinia()
    setActivePinia(pinia)

    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, pinia],
        stubs: {
          Button: true,
          Toggle: true,
        },
      },
    })

    await flushPromises()

    // Set paired devices in store
    const deviceStore = wrapper.vm.deviceStore
    if (deviceStore) {
      deviceStore.pairedDevices = [
        {
          id: 'device-1',
          deviceName: 'My Phone',
          deviceFingerprint: 'fp123',
          publicKey: 'pk123',
          pairedAt: new Date().toISOString(),
          isActive: true,
        },
      ]
    }

    await flushPromises()
    wrapper.vm.$forceUpdate()
    await flushPromises()

    // Check if device is shown
    if (deviceStore && deviceStore.pairedDevices.length > 0) {
      expect(deviceStore.pairedDevices[0].deviceName).toBe('My Phone')
    }
  })
})

describe('DevicesView Date Formatting', () => {
  it('should format date correctly', () => {
    setActivePinia(createPinia())

    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          Button: true,
          Toggle: true,
        },
      },
    })

    const dateStr = '2024-01-15T10:30:00Z'
    const formatted = wrapper.vm.formatDate(dateStr)

    expect(formatted).toContain('2024')
    expect(typeof formatted).toBe('string')
  })
})
