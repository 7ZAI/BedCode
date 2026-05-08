import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useDeviceStore } from '@/stores/device'

// Mock Tauri invoke
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

// Create a mutable devices array that will be managed by the mock
let mockDevicesValue: any[] = []

vi.mock('@/composables/useTauri', () => ({
  usePairing: () => ({
    devices: {
      get value() { return mockDevicesValue },
      set value(v) { mockDevicesValue = v }
    },
    loadDevices: vi.fn(async () => {}),
    removeDevice: vi.fn(async (id: string) => {
      mockDevicesValue = mockDevicesValue.filter(d => d.id !== id)
    }),
  }),
}))

describe('Device Store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    // Reset mock devices before each test
    mockDevicesValue = []
  })

  it('should initialize with empty state', () => {
    const store = useDeviceStore()

    expect(store.pairedDevices).toEqual([])
    expect(store.pairingCode).toBeNull()
    expect(store.pairingExpiry).toBe(0)
  })

  it('should clear pairing code', () => {
    const store = useDeviceStore()

    store.pairingCode = '123456'
    store.pairingExpiry = 30

    store.clearPairingCode()

    expect(store.pairingCode).toBeNull()
    expect(store.pairingExpiry).toBe(0)
  })

  it('should add paired device', () => {
    const store = useDeviceStore()

    const device = {
      id: 'device-1',
      device_name: 'My Phone',
      device_fingerprint: 'fp123',
      public_key: 'pk123',
      paired_at: new Date().toISOString(),
      last_seen: null,
      is_active: true,
    }

    store.pairedDevices.push(device)

    expect(store.pairedDevices).toHaveLength(1)
    expect(store.pairedDevices[0].device_name).toBe('My Phone')
  })

  it('should remove paired device', async () => {
    // Set up mock devices
    mockDevicesValue = [
      { id: 'device-1', device_name: 'Phone 1', device_fingerprint: 'fp1', public_key: 'pk1', paired_at: '', last_seen: null, is_active: true },
      { id: 'device-2', device_name: 'Phone 2', device_fingerprint: 'fp2', public_key: 'pk2', paired_at: '', last_seen: null, is_active: true },
    ]

    const store = useDeviceStore()

    // Initialize store's pairedDevices from mock
    store.pairedDevices = [...mockDevicesValue]

    await store.removeDevice('device-1')

    // After removeDevice, the store syncs with mockDevicesValue
    expect(store.pairedDevices).toHaveLength(1)
    expect(store.pairedDevices[0].id).toBe('device-2')
  })
})
