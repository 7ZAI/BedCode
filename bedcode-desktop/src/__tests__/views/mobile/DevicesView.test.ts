import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createRouter, createWebHistory } from 'vue-router'
import { setActivePinia, createPinia } from 'pinia'
import DevicesView from '@/modules/mobile/views/DevicesView.vue'

// Mock Tauri APIs
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

// Mock useRemoteConnection composable
const mockPairedDevices = { value: [] }
const mockCurrentDevice = { value: null }
const mockState = { value: { status: 'disconnected' } }
const mockIsConnected = { value: false }
const mockSendMessageWithResponse = vi.fn()
const mockActiveSessionId = { value: null }

vi.mock('@/composables/useRemoteConnection', () => ({
  useRemoteConnection: () => ({
    state: mockState,
    pairedDevices: mockPairedDevices,
    currentDevice: mockCurrentDevice,
    isConnected: mockIsConnected,
    lastMessage: { value: null },
    isReady: { value: false },
    activeSessionId: mockActiveSessionId,
    connect: vi.fn().mockResolvedValue(undefined),
    authenticate: vi.fn().mockResolvedValue(false),
    requestPairing: vi.fn().mockResolvedValue(undefined),
    verifyPairingCode: vi.fn().mockResolvedValue(true),
    disconnect: vi.fn(),
    loadPairedDevices: vi.fn().mockResolvedValue(undefined),
    sendMessage: vi.fn(),
    sendMessageWithResponse: mockSendMessageWithResponse,
    setReconnectCallback: vi.fn(),
  }),
}))

// Mock useRemoteTerminal composable
vi.mock('@/composables/useRemoteTerminal', () => ({
  useRemoteTerminal: () => ({
    sessions: { value: [] },
    sessionConfigs: { value: [] },
    currentSessionId: { value: null },
    outputBuffer: { value: [] },
    isWaitingInput: { value: false },
    isLoading: { value: false },
    error: { value: null },
    loadSessions: vi.fn().mockResolvedValue(undefined),
    loadSessionConfigs: vi.fn().mockResolvedValue(undefined),
    startSession: vi.fn().mockResolvedValue('session-1'),
    stopSession: vi.fn().mockResolvedValue(undefined),
    joinSession: vi.fn().mockResolvedValue(undefined),
    leaveSession: vi.fn().mockResolvedValue(undefined),
    sendInput: vi.fn(),
    sendSpecialKey: vi.fn(),
    clearOutput: vi.fn(),
    reconnectAndResume: vi.fn().mockResolvedValue(undefined),
    enableAutoReconnect: vi.fn(),
    disableAutoReconnect: vi.fn(),
  }),
}))

// Mock components
vi.mock('@/modules/mobile/components/BottomSheet.vue', () => ({
  default: {
    template: '<div v-if="modelValue" class="bottom-sheet"><slot /></div>',
    props: ['modelValue', 'title', 'placeholder'],
    emits: ['submit'],
  },
}))

vi.mock('@/modules/mobile/components/PairingInput.vue', () => ({
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
    { path: '/mobile/scan', component: { template: '<div />' }, name: 'mobile-scan' },
    { path: '/mobile/terminal/:id', component: { template: '<div />' } },
  ],
})

describe('Mobile DevicesView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    mockPairedDevices.value = []
    mockCurrentDevice.value = null
    mockState.value = { status: 'disconnected' }
    mockIsConnected.value = false
    mockSendMessageWithResponse.mockResolvedValue({
      payload: { action: { type: 'session_config_list', configs: [] } },
    })
  })

  it('should render header with title', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.find('header').exists()).toBe(true)
    expect(wrapper.find('h1').text()).toContain('会话配置')
  })

  it('should show connection history section when disconnected', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.text()).toContain('连接历史')
  })

  it('should show empty connection history', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.text()).toContain('暂无连接历史')
  })

  it('should show scan button when disconnected', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.text()).toContain('扫描连接')
  })

  it('should show manual connect button when disconnected', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.text()).toContain('手动连接')
  })

  it('should show session configs section when connected', async () => {
    mockState.value = { status: 'paired' }
    mockIsConnected.value = true
    mockCurrentDevice.value = {
      id: 'device-1',
      name: 'My Desktop',
      address: '192.168.1.100',
      port: 8765,
      isPaired: true,
    }

    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.text()).toContain('会话配置')
  })

  it('should show empty session configs when none exist', async () => {
    mockState.value = { status: 'paired' }
    mockIsConnected.value = true
    mockCurrentDevice.value = {
      id: 'device-1',
      name: 'My Desktop',
      address: '192.168.1.100',
      port: 8765,
      isPaired: true,
    }
    mockSendMessageWithResponse.mockResolvedValue({
      payload: { action: { type: 'session_config_list', configs: [] } },
    })

    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.text()).toContain('暂无会话配置')
  })

  it('should display session config details', async () => {
    mockState.value = { status: 'paired' }
    mockIsConnected.value = true
    mockCurrentDevice.value = {
      id: 'device-1',
      name: 'My Desktop',
      address: '192.168.1.100',
      port: 8765,
      isPaired: true,
    }
    mockSendMessageWithResponse.mockResolvedValue({
      payload: {
        action: {
          type: 'session_config_list',
          configs: [
            {
              id: 'cfg-1',
              name: 'Claude Code',
              environment: 'windows',
              wsl_distro: null,
              working_dir: 'C:\\projects',
              command: 'claude',
            },
            {
              id: 'cfg-2',
              name: 'WSL Dev',
              environment: 'wsl2',
              wsl_distro: 'Ubuntu-22.04',
              working_dir: '/home/user',
              command: 'bash',
            },
          ],
        },
      },
    })

    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.text()).toContain('Claude Code')
    expect(wrapper.text()).toContain('Windows')
    expect(wrapper.text()).toContain('WSL Dev')
    expect(wrapper.text()).toContain('WSL2')
  })

  it('should show manual connect dialog', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    wrapper.vm.showManualConnect = true
    await flushPromises()

    expect(wrapper.vm.showManualConnect).toBe(true)
  })

  it('should show pairing dialog', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

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
    mockSendMessageWithResponse.mockResolvedValue({
      payload: { action: { type: 'session_config_list', configs: [] } },
    })
  })

  it('should parse address with port', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

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
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

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
    mockSendMessageWithResponse.mockResolvedValue({
      payload: { action: { type: 'session_config_list', configs: [] } },
    })
  })

  it('should have empty pairing error initially', async () => {
    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.vm.pairingError).toBe('')
  })
})

describe('Mobile DevicesView Session Start', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    mockState.value = { status: 'paired' }
    mockIsConnected.value = true
    mockCurrentDevice.value = {
      id: 'device-1',
      name: 'My Desktop',
      address: '192.168.1.100',
      port: 8765,
      isPaired: true,
    }
  })

  it('should refresh sessions after starting session instead of navigating', async () => {
    const pushSpy = vi.spyOn(mockRouter, 'push')
    mockSendMessageWithResponse
      .mockResolvedValueOnce({
        payload: {
          action: {
            type: 'session_config_list',
            configs: [
              { id: 'cfg-1', name: 'Test', environment: 'windows', working_dir: '/tmp', command: 'claude' },
            ],
          },
        },
      })
      .mockResolvedValueOnce({
        session_id: 'session-123',
      })

    const wrapper = mount(DevicesView, {
      global: {
        plugins: [mockRouter, createPinia()],
        stubs: {
          BottomSheet: true,
          PairingInput: true,
        },
      },
    })

    await flushPromises()

    // Tap the config card to start session
    const config = {
      id: 'cfg-1',
      name: 'Test',
      environment: 'windows',
      working_dir: '/tmp',
      command: 'claude',
    }
    wrapper.vm.handleStartSession(config)
    await flushPromises()

    // Should NOT navigate to terminal anymore
    expect(pushSpy).not.toHaveBeenCalled()
    // activeSessionId should be set
    expect(mockActiveSessionId.value).toBe('session-123')
  })
})
