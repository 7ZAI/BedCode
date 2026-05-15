import { describe, it, expect, vi, beforeEach } from 'vitest'
import { useRemoteConnection } from '@/modules/shared/composables/useRemoteConnection'

// Mock Tauri invoke
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

describe('useRemoteConnection', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('should initialize with disconnected state', () => {
    const { state } = useRemoteConnection()
    expect(state.value.status).toBe('disconnected')
  })

  it('should have empty paired devices initially', () => {
    const { pairedDevices } = useRemoteConnection()
    expect(pairedDevices.value).toEqual([])
  })
})
