import { describe, it, expect, beforeEach, vi } from 'vitest'
import { usePairing } from '@/composables/usePairing'
import type { PairingCodeInfo } from '@/composables/useDesktopCommands'

// Mock 配对码相关命令
const mocks = vi.hoisted(() => ({
  generatePairingCode: vi.fn(),
  clearPairingCode: vi.fn(),
  getCurrentPairingCode: vi.fn(),
}))
vi.mock('@/composables/useDesktopCommands', () => mocks)

const validCode: PairingCodeInfo = {
  code: '123456',
  created_at: '2025-01-01T00:00:00Z',
  expires_in: 60,
}

describe('usePairing', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('should initialize with null pairing code', () => {
    const { pairingCode } = usePairing()

    expect(pairingCode.value).toBeNull()
  })

  describe('generateCode', () => {
    it('should store the generated pairing code', async () => {
      mocks.generatePairingCode.mockResolvedValueOnce(validCode)

      const { pairingCode, generateCode } = usePairing()
      await generateCode()

      expect(mocks.generatePairingCode).toHaveBeenCalledTimes(1)
      expect(pairingCode.value).toEqual(validCode)
    })
  })

  describe('clearCode', () => {
    it('should call the backend and reset pairing code to null', async () => {
      mocks.clearPairingCode.mockResolvedValueOnce(undefined)

      const { pairingCode, generateCode, clearCode } = usePairing()
      mocks.generatePairingCode.mockResolvedValueOnce(validCode)
      await generateCode()
      expect(pairingCode.value).not.toBeNull()

      await clearCode()

      expect(mocks.clearPairingCode).toHaveBeenCalledTimes(1)
      expect(pairingCode.value).toBeNull()
    })
  })

  describe('checkCurrentCode', () => {
    it('should restore pairing code and return true when a valid code exists', async () => {
      mocks.getCurrentPairingCode.mockResolvedValueOnce(validCode)

      const { pairingCode, checkCurrentCode } = usePairing()
      const restored = await checkCurrentCode()

      expect(restored).toBe(true)
      expect(pairingCode.value).toEqual(validCode)
    })

    it('should return false when backend returns null', async () => {
      mocks.getCurrentPairingCode.mockResolvedValueOnce(null)

      const { pairingCode, checkCurrentCode } = usePairing()
      const restored = await checkCurrentCode()

      expect(restored).toBe(false)
      expect(pairingCode.value).toBeNull()
    })

    it('should return false when code is empty string', async () => {
      mocks.getCurrentPairingCode.mockResolvedValueOnce({ ...validCode, code: '' })

      const { pairingCode, checkCurrentCode } = usePairing()
      const restored = await checkCurrentCode()

      expect(restored).toBe(false)
      expect(pairingCode.value).toBeNull()
    })

    it('should overwrite previous code when checking returns a new one', async () => {
      mocks.generatePairingCode.mockResolvedValueOnce(validCode)
      mocks.getCurrentPairingCode.mockResolvedValueOnce({ ...validCode, code: '654321' })

      const { pairingCode, generateCode, checkCurrentCode } = usePairing()
      await generateCode()

      const restored = await checkCurrentCode()

      expect(restored).toBe(true)
      expect(pairingCode.value?.code).toBe('654321')
    })
  })
})
