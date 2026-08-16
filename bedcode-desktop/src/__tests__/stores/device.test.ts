import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useDeviceStore } from '@/stores/device'
import { makePairingCodeInfo, makePairing } from '@/__tests__/fixtures/pairing'

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
    // 取数自 fixtures 工厂（对齐 pairing.rs 的 code/created_at/expires_in）
    generatePairingCode: vi.fn(async () =>
      makePairingCodeInfo({ created_at: new Date().toISOString() }),
    ),
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
    // Set up mock devices（取数自 fixtures 工厂，对齐 db/models.rs Pairing 线协议）
    mocks.mockDevices.push(
      makePairing({ id: 'device-1', deviceName: 'Phone 1', deviceFingerprint: 'fp1', publicKey: 'pk1', pairedAt: '', lastSeen: null, isActive: true }),
      makePairing({ id: 'device-2', deviceName: 'Phone 2', deviceFingerprint: 'fp2', publicKey: 'pk2', pairedAt: '', lastSeen: null, isActive: true }),
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
      makePairing({ id: 'device-1', deviceName: 'Phone 1', deviceFingerprint: 'fp1', publicKey: 'pk1', pairedAt: '', lastSeen: null, isActive: true }),
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
