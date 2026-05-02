import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createRouter, createWebHistory } from 'vue-router'
import { setActivePinia, createPinia } from 'pinia'
import DevicesView from '@/views/mobile/DevicesView.vue'

// Mock Tauri APIs
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

// Mock useRemoteConnection composable
const mockDiscoveredDevices = { value: [] }
const mockPairedDevices = { value: [] }
const mockCurrentDevice = { value: null }
const mockState = { value: { status: 'disconnected' } }
const mockIsConnected = { value: false }

vi.mock('@/composables/useRemoteConnection', () => ({
  useRemoteConnection: () => ({
    state: mockState,
    discoveredDevices: mockDiscoveredDevices,
    pairedDevices: mockPairedDevices,
    currentDevice: mockCurrentDevice,
    isConnected: mockIsConnected,
    lastMessage: { value: null },
    isReady: { value: false },
    discoverDevices: vi.fn().mockResolvedValue(undefined),
    connect: vi.fn().mockResolvedValue(undefined),
    requestPairing: vi.fn().mockResolvedValue(undefined),
    verifyPairingCode: vi.fn().mockResolvedValue(true),
    disconnect: vi.fn(),
    loadPairedDevices: vi.fn().mockResolvedValue(undefined),
    sendMessage: vi.fn(),
    sendMessageWithResponse: vi.fn(),
  }),
}))

// Mock components
vi.mock('@/components/mobile/DeviceCard.vue', () => ({
  default: {
    template: '<div class="device-card" @click="$emit(\'click\')">{{ device.name }}</div>',
    props: ['device'],
    emits: ['click'],
  },
}))

vi.mock('@/components/mobile/BottomSheet.vue', () => ({
  default: {
    template: '<div v-if="modelValue" class="bottom-sheet"><slot /></div>',
    props: ['modelValue', 'title', 'placeholder'],
    emits: ['submit'],
  },
}))

vi.mock('@/components/mobile/PairingInput.vue', () => ({
  default: {
    template: '<div v-if="modelValue" class="pairing-input"><slot /></div>',
    props: ['modelValue', 'loading', 'error'],
    emits: ['submit'],
  },
}))

// Create mock router
const mockRouter = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', component: { template: '<div />' } },
    { path: '/mobile/devices', component: { template: '<div />' } },
    { path: '/mobile/terminal/:id', component: { template: '<div />' } },
  ],
})

describe('Mobile DevicesView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    // Reset mock state
    mockDiscoveredDevices.value = []
    mockPairedDevices.value = []
    mockCurrentDevice.value = null
    mockState.value = { status: 'disconnected' }
    mockIsConnected.value = false
  })

  it('should render header with title', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          DeviceCard: true,
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.find('header').exists()).toBe(true)
    expect(wrapper.find('h1').text()).toContain('设备')
  })

  it('should show scan button', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          DeviceCard: true,
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    const scanButton = wrapper.find('button')
    expect(scanButton.exists()).toBe(true)
  })

  it('should show discovered devices section', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          DeviceCard: true,
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.text()).toContain('发现设备')
  })

  it('should show paired devices section', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          DeviceCard: true,
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.text()).toContain('已配对设备')
  })

  it('should show empty state for discovered devices', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          DeviceCard: true,
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.text()).toContain('点击右上角扫描设备')
  })

  it('should show empty state for paired devices', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          DeviceCard: true,
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.text()).toContain('暂无已配对设备')
  })

  it('should show manual connect button', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          DeviceCard: true,
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.text()).toContain('手动输入地址连接')
  })

  it('should show scanning indicator when scanning', async () => {
    mockState.value = { status: 'connecting' }

    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          DeviceCard: true,
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.text()).toContain('扫描中...')
  })

  it('should show manual connect dialog', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          DeviceCard: true,
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    // Set showManualConnect
    wrapper.vm.showManualConnect = true
    await flushPromises()

    expect(wrapper.vm.showManualConnect).toBe(true)
  })

  it('should show pairing dialog', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          DeviceCard: true,
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    // Set showPairing
    wrapper.vm.showPairing = true
    await flushPromises()

    expect(wrapper.vm.showPairing).toBe(true)
  })
})

describe('Mobile DevicesView Manual Connect', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    mockState.value = { status: 'disconnected' }
  })

  it('should parse address with port', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          DeviceCard: true,
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    // Call handleConnectManual
    wrapper.vm.handleConnectManual('192.168.1.100:9000')

    expect(wrapper.vm.pendingDevice).toEqual({
      id: '192.168.1.100:9000',
      name: '192.168.1.100',
      address: '192.168.1.100',
      port: 9000,
      isPaired: false,
    })
  })

  it('should use default port when not specified', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          DeviceCard: true,
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    // Call handleConnectManual without port
    wrapper.vm.handleConnectManual('192.168.1.100')

    expect(wrapper.vm.pendingDevice).toEqual({
      id: '192.168.1.100:8765',
      name: '192.168.1.100',
      address: '192.168.1.100',
      port: 8765,
      isPaired: false,
    })
  })
})

describe('Mobile DevicesView Pairing', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    mockState.value = { status: 'disconnected' }
  })

  it('should show error on failed pairing', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          DeviceCard: true,
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.vm.pairingError).toBe('')
  })
})

describe('Mobile DevicesView Navigation', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    mockState.value = { status: 'disconnected' }
  })

  it('should navigate to terminal when opening device', async () => {
    const pushSpy = vi.spyOn(mockRouter, 'push')

    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          DeviceCard: true,
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    // Open terminal
    const device = {
      id: 'device-1',
      name: 'My Desktop',
      address: '192.168.1.100',
      port: 8765,
      isPaired: true,
    }

    wrapper.vm.handleOpenTerminal(device)

    expect(pushSpy).toHaveBeenCalledWith('/mobile/terminal/device-1')
  })
})

describe('Mobile DevicesView Connection', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    mockState.value = { status: 'disconnected' }
  })

  it('should set pending device when connecting', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          DeviceCard: true,
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    const device = {
      id: 'device-1',
      name: 'Test Device',
      address: '192.168.1.100',
      port: 8765,
      isPaired: false,
    }

    wrapper.vm.handleConnect(device)

    expect(wrapper.vm.pendingDevice).toEqual(device)
    expect(wrapper.vm.showPairing).toBe(true)
  })
})
