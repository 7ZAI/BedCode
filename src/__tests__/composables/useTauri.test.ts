import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { defineComponent, nextTick } from 'vue'
import {
  useWsl,
  useTmux,
  useSessionConfig,
  useSession,
  useQuickActions,
  usePairing,
  useDiscovery,
  useNetwork,
} from '@/composables/useTauri'

// Mock Tauri APIs
const mockInvoke = vi.fn()
const mockListen = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: any[]) => mockListen(...args),
}))

// Helper to use composable in component context
function withComposable<T>(composable: () => T) {
  let result: T

  const TestComponent = defineComponent({
    setup() {
      result = composable()
      return {}
    },
    template: '<div></div>',
  })

  const wrapper = mount(TestComponent)

  return {
    get result() {
      return result!
    },
    wrapper,
  }
}

describe('useTauri Composables', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  describe('useWsl', () => {
    it('should initialize with empty distros', () => {
      const { result, wrapper } = withComposable(() => useWsl())

      expect(result.distros.value).toEqual([])
      expect(result.isAvailable.value).toBe(false)

      wrapper.unmount()
    })

    it('should load distros when WSL is available', async () => {
      mockInvoke
        .mockResolvedValueOnce(true) // is_wsl_available
        .mockResolvedValueOnce([
          { name: 'Ubuntu', isDefault: true, state: 'Running', version: 2 },
          { name: 'Debian', isDefault: false, state: 'Stopped', version: 2 },
        ])

      const { result, wrapper } = withComposable(() => useWsl())
      await result.loadDistros()

      expect(mockInvoke).toHaveBeenCalledWith('is_wsl_available')
      expect(mockInvoke).toHaveBeenCalledWith('list_wsl_distributions')
      expect(result.distros.value).toHaveLength(2)
      expect(result.distros.value[0].name).toBe('Ubuntu')
      expect(result.isAvailable.value).toBe(true)

      wrapper.unmount()
    })

    it('should not load distros when WSL is not available', async () => {
      mockInvoke.mockResolvedValueOnce(false) // is_wsl_available

      const { result, wrapper } = withComposable(() => useWsl())
      await result.loadDistros()

      expect(mockInvoke).toHaveBeenCalledTimes(1)
      expect(result.distros.value).toEqual([])
      expect(result.isAvailable.value).toBe(false)

      wrapper.unmount()
    })

    it('should handle load error gracefully', async () => {
      const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
      mockInvoke.mockRejectedValueOnce(new Error('WSL check failed'))

      const { result, wrapper } = withComposable(() => useWsl())
      await result.loadDistros()

      expect(consoleSpy).toHaveBeenCalledWith('Failed to load WSL distros:', expect.any(Error))

      consoleSpy.mockRestore()
      wrapper.unmount()
    })
  })

  describe('useTmux', () => {
    it('should initialize with empty sessions', () => {
      const { result, wrapper } = withComposable(() => useTmux())

      expect(result.sessions.value).toEqual([])
      expect(result.isAvailable.value).toBe(false)

      wrapper.unmount()
    })

    it('should load tmux sessions when available', async () => {
      mockInvoke
        .mockResolvedValueOnce(true) // is_tmux_available
        .mockResolvedValueOnce([
          { name: 'main', windows: 2, isAttached: true },
          { name: 'dev', windows: 1, isAttached: false },
        ])

      const { result, wrapper } = withComposable(() => useTmux())
      await result.loadSessions()

      expect(result.sessions.value).toHaveLength(2)
      expect(result.sessions.value[0].name).toBe('main')
      expect(result.isAvailable.value).toBe(true)

      wrapper.unmount()
    })

    it('should create a new tmux session', async () => {
      mockInvoke
        .mockResolvedValueOnce(undefined) // create_tmux_session
        .mockResolvedValueOnce(true) // is_tmux_available (for loadSessions)
        .mockResolvedValueOnce([{ name: 'new-session', windows: 1, isAttached: false }])

      const { result, wrapper } = withComposable(() => useTmux())
      await result.createSession('new-session', 'vim')

      expect(mockInvoke).toHaveBeenCalledWith('create_tmux_session', {
        name: 'new-session',
        command: 'vim',
      })

      wrapper.unmount()
    })
  })

  describe('useSessionConfig', () => {
    it('should initialize with empty configs', () => {
      const { result, wrapper } = withComposable(() => useSessionConfig())

      expect(result.configs.value).toEqual([])

      wrapper.unmount()
    })

    it('should load session configs', async () => {
      mockInvoke.mockResolvedValueOnce([
        { id: '1', name: 'Config 1', environment: 'windows' },
        { id: '2', name: 'Config 2', environment: 'wsl2' },
      ])

      const { result, wrapper } = withComposable(() => useSessionConfig())
      await result.loadConfigs()

      expect(mockInvoke).toHaveBeenCalledWith('list_session_configs')
      expect(result.configs.value).toHaveLength(2)

      wrapper.unmount()
    })

    it('should create a new config', async () => {
      const newConfig = {
        id: 'new-id',
        name: 'New Config',
        environment: 'windows',
        workingDir: '/home',
        command: 'claude',
        autoStart: false,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      }

      mockInvoke
        .mockResolvedValueOnce(newConfig) // create returns config
        .mockResolvedValueOnce([newConfig]) // reload

      const { result, wrapper } = withComposable(() => useSessionConfig())
      const created = await result.createConfig('New Config', 'windows', '/home', 'claude')

      expect(created.name).toBe('New Config')
      expect(mockInvoke).toHaveBeenCalledWith('create_session_config', {
        name: 'New Config',
        environment: 'windows',
        workingDir: '/home',
        command: 'claude',
        wslDistro: undefined,
        tmuxSession: undefined,
      })

      wrapper.unmount()
    })

    it('should delete a config', async () => {
      mockInvoke
        .mockResolvedValueOnce(undefined) // delete
        .mockResolvedValueOnce([]) // reload

      const { result, wrapper } = withComposable(() => useSessionConfig())
      await result.deleteConfig('config-id')

      expect(mockInvoke).toHaveBeenCalledWith('delete_session_config', { id: 'config-id' })

      wrapper.unmount()
    })
  })

  describe('useSession', () => {
    it('should initialize with empty sessions', () => {
      const { result, wrapper } = withComposable(() => useSession())

      expect(result.sessions.value).toEqual([])
      expect(result.outputs.value.size).toBe(0)

      wrapper.unmount()
    })

    it('should load sessions', async () => {
      mockInvoke.mockResolvedValueOnce([
        { id: 's1', name: 'Session 1', status: 'Running' },
      ])

      const { result, wrapper } = withComposable(() => useSession())
      await result.loadSessions()

      expect(result.sessions.value).toHaveLength(1)

      wrapper.unmount()
    })

    it('should start a session', async () => {
      mockInvoke
        .mockResolvedValueOnce('session-id') // start_session returns ID
        .mockResolvedValueOnce([{ id: 'session-id', name: 'Test' }]) // reload

      const { result, wrapper } = withComposable(() => useSession())
      const id = await result.startSession('config-id')

      expect(id).toBe('session-id')
      expect(mockInvoke).toHaveBeenCalledWith('start_session', { configId: 'config-id' })

      wrapper.unmount()
    })

    it('should kill a session', async () => {
      mockInvoke
        .mockResolvedValueOnce(undefined) // kill
        .mockResolvedValueOnce([]) // reload

      const { result, wrapper } = withComposable(() => useSession())
      await result.killSession('session-id')

      expect(mockInvoke).toHaveBeenCalledWith('kill_session', { sessionId: 'session-id' })

      wrapper.unmount()
    })

    it('should write to session', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const { result, wrapper } = withComposable(() => useSession())
      await result.writeToSession('session-id', 'input text')

      expect(mockInvoke).toHaveBeenCalledWith('write_to_session', {
        sessionId: 'session-id',
        data: 'input text',
      })

      wrapper.unmount()
    })

    it('should send special key', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const { result, wrapper } = withComposable(() => useSession())
      await result.sendSpecialKey('session-id', 'Enter')

      expect(mockInvoke).toHaveBeenCalledWith('send_special_key', {
        sessionId: 'session-id',
        key: 'Enter',
      })

      wrapper.unmount()
    })

    it('should resize session', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const { result, wrapper } = withComposable(() => useSession())
      await result.resizeSession('session-id', 120, 40)

      expect(mockInvoke).toHaveBeenCalledWith('resize_session', {
        sessionId: 'session-id',
        cols: 120,
        rows: 40,
      })

      wrapper.unmount()
    })
  })

  describe('useQuickActions', () => {
    it('should initialize with empty actions', () => {
      const { result, wrapper } = withComposable(() => useQuickActions())

      expect(result.actions.value).toEqual([])

      wrapper.unmount()
    })

    it('should load actions', async () => {
      mockInvoke.mockResolvedValueOnce([
        { id: '1', name: 'Action 1', content: 'echo hello' },
      ])

      const { result, wrapper } = withComposable(() => useQuickActions())
      await result.loadActions()

      expect(mockInvoke).toHaveBeenCalledWith('list_quick_actions')
      expect(result.actions.value).toHaveLength(1)

      wrapper.unmount()
    })

    it('should create an action', async () => {
      mockInvoke
        .mockResolvedValueOnce({ id: '1', name: 'Test', content: 'test' })
        .mockResolvedValueOnce([{ id: '1', name: 'Test', content: 'test' }])

      const { result, wrapper } = withComposable(() => useQuickActions())
      await result.createAction('Test', 'test', 'icon', 'blue')

      expect(mockInvoke).toHaveBeenCalledWith('create_quick_action', {
        name: 'Test',
        content: 'test',
        icon: 'icon',
        color: 'blue',
      })

      wrapper.unmount()
    })
  })

  describe('usePairing', () => {
    it('should initialize with empty devices', () => {
      const { result, wrapper } = withComposable(() => usePairing())

      expect(result.devices.value).toEqual([])
      expect(result.pairingCode.value).toBeNull()

      wrapper.unmount()
    })

    it('should load paired devices', async () => {
      mockInvoke.mockResolvedValueOnce([
        { id: '1', deviceName: 'Phone', isActive: true },
      ])

      const { result, wrapper } = withComposable(() => usePairing())
      await result.loadDevices()

      expect(mockInvoke).toHaveBeenCalledWith('list_paired_devices')
      expect(result.devices.value).toHaveLength(1)

      wrapper.unmount()
    })

    it('should generate pairing code', async () => {
      mockInvoke.mockResolvedValueOnce({ code: '123456', expiresIn: 60 })

      const { result, wrapper } = withComposable(() => usePairing())
      await result.generateCode()

      expect(mockInvoke).toHaveBeenCalledWith('generate_pairing_code')
      expect(result.pairingCode.value).toEqual({ code: '123456', expiresIn: 60 })

      wrapper.unmount()
    })

    it('should verify pairing code', async () => {
      mockInvoke.mockResolvedValueOnce(true)

      const { result, wrapper } = withComposable(() => usePairing())
      const isValid = await result.verifyCode('123456')

      expect(isValid).toBe(true)
      expect(mockInvoke).toHaveBeenCalledWith('verify_pairing_code', { code: '123456' })

      wrapper.unmount()
    })

    it('should remove paired device', async () => {
      mockInvoke
        .mockResolvedValueOnce(undefined) // remove
        .mockResolvedValueOnce([]) // reload

      const { result, wrapper } = withComposable(() => usePairing())
      await result.removeDevice('device-id')

      expect(mockInvoke).toHaveBeenCalledWith('remove_paired_device', { id: 'device-id' })

      wrapper.unmount()
    })
  })

  describe('useDiscovery', () => {
    it('should initialize with empty discovered devices', () => {
      const { result, wrapper } = withComposable(() => useDiscovery())

      expect(result.discoveredDevices.value).toEqual([])

      wrapper.unmount()
    })

    it('should start discovery', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const { result, wrapper } = withComposable(() => useDiscovery())
      await result.startDiscovery()

      expect(mockInvoke).toHaveBeenCalledWith('start_discovery')

      wrapper.unmount()
    })

    it('should load discovered devices', async () => {
      mockInvoke.mockResolvedValueOnce([
        { name: 'Device 1', address: '192.168.1.100', port: 8765 },
      ])

      const { result, wrapper } = withComposable(() => useDiscovery())
      await result.loadDiscoveredDevices()

      expect(mockInvoke).toHaveBeenCalledWith('get_discovered_devices')
      expect(result.discoveredDevices.value).toHaveLength(1)

      wrapper.unmount()
    })

    it('should start broadcast', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const { result, wrapper } = withComposable(() => useDiscovery())
      await result.startBroadcast('my-service', 8765)

      expect(mockInvoke).toHaveBeenCalledWith('start_broadcast', {
        serviceName: 'my-service',
        port: 8765,
      })

      wrapper.unmount()
    })
  })

  describe('useNetwork', () => {
    it('should initialize with empty addresses', () => {
      const { result, wrapper } = withComposable(() => useNetwork())

      expect(result.localAddresses.value).toEqual([])

      wrapper.unmount()
    })

    it('should load local IP addresses', async () => {
      mockInvoke.mockResolvedValueOnce(['192.168.1.100', '10.0.0.1'])

      const { result, wrapper } = withComposable(() => useNetwork())
      await result.loadLocalAddresses()

      expect(mockInvoke).toHaveBeenCalledWith('get_local_ip_addresses')
      expect(result.localAddresses.value).toHaveLength(2)
      expect(result.localAddresses.value).toContain('192.168.1.100')

      wrapper.unmount()
    })
  })
})
