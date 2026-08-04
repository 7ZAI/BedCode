import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useDeviceStore } from '@/stores/device'

// Mock Tauri invoke
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

// Mock useDesktopCommands（store 的数据源）
const mocks = vi.hoisted(() => {
  const mockDevices: any[] = []
  return {
    mockDevices,
    listPairedDevices: vi.fn(async () => [...mockDevices]),
    removePairedDevice: vi.fn(async (id: string) => {
      const idx = mockDevices.findIndex((d) => d.id === id)
      if (idx !== -1) mockDevices.splice(idx, 1)
    }),
    generatePairingCode: vi.fn(async () => ({
      code: '123456',
      expires_in: 60,
      created_at: new Date().toISOString(),
    })),
    verifyPairingCode: vi.fn(async () => true),
  }
})

vi.mock('@/composables/useDesktopCommands', () => mocks)

describe('Device Store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    // Reset mock devices before each test
    mocks.mockDevices.length = 0
  })

  it('should initialize with empty state', () => {
    const store = useDeviceStore()

    expect(store.pairedDevices).toEqual([])
    expect(store.pairingCode).toBeNull()
    expect(store.pairingExpiry).toBe(0)
  })

  it('should clear pairing code', () => {
    const store = useDeviceStore()

    store.pairingCode = { code: '123456', expires_in: 60, created_at: new Date().toISOString() }
    store.pairingExpiry = 60

    store.clearPairingCode()

    expect(store.pairingCode).toBeNull()
    expect(store.pairingExpiry).toBe(0)
  })

  it('should add paired device', () => {
    const store = useDeviceStore()

    const device = {
      id: 'device-1',
      deviceName: 'My Phone',
      deviceFingerprint: 'fp123',
      address: '192.168.1.5',
      pairedAt: new Date().toISOString(),
      lastSeen: undefined,
      connectCount: 0,
    }

    store.pairedDevices.push(device)

    expect(store.pairedDevices).toHaveLength(1)
    expect(store.pairedDevices[0].deviceName).toBe('My Phone')
  })

  it('should remove paired device', async () => {
    // Set up mock devices
    mocks.mockDevices.push(
      { id: 'device-1', device_name: 'Phone 1', device_fingerprint: 'fp1', public_key: 'pk1', paired_at: '', last_seen: null, is_active: true },
      { id: 'device-2', device_name: 'Phone 2', device_fingerprint: 'fp2', public_key: 'pk2', paired_at: '', last_seen: null, is_active: true },
    )

    const store = useDeviceStore()
    await store.loadPairedDevices()

    expect(store.pairedDevices).toHaveLength(2)

    await store.removeDevice('device-1')

    // 删除后重新拉取列表，device-1 应被移除
    expect(store.pairedDevices).toHaveLength(1)
    expect(store.pairedDevices[0].id).toBe('device-2')
  })

  it('should load paired devices from backend', async () => {
    mocks.mockDevices.push(
      { id: 'device-1', device_name: 'Phone 1', device_fingerprint: 'fp1', public_key: 'pk1', paired_at: '', last_seen: null, is_active: true },
    )

    const store = useDeviceStore()
    await store.loadPairedDevices()

    expect(mocks.listPairedDevices).toHaveBeenCalled()
    expect(store.pairedDevices).toHaveLength(1)
  })

  it('should start pairing and generate code', async () => {
    vi.useFakeTimers()
    const store = useDeviceStore()
    await store.startPairing()

    expect(mocks.generatePairingCode).toHaveBeenCalled()
    expect(store.pairingCode?.code).toBe('123456')
    expect(store.pairingExpiry).toBe(60)

    // 推进倒计时使其归零，让内部 interval 自我清理，避免测试挂起
    vi.advanceTimersByTime(61000)
    vi.useRealTimers()
  })
})
