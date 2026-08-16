import { describe, it, expect, beforeEach, vi } from 'vitest'
import { useWsl } from '@/composables/useWsl'

// Mock WSL 探测命令
const mocks = vi.hoisted(() => ({
  isWslAvailable: vi.fn(),
  listWslDistributions: vi.fn(),
}))
vi.mock('@/composables/useDesktopCommands', () => mocks)

describe('useWsl', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('should initialize with empty state', () => {
    const wsl = useWsl()

    expect(wsl.distros.value).toEqual([])
    expect(wsl.isAvailable.value).toBe(false)
  })

  describe('loadDistros', () => {
    it('should load distributions when WSL is available', async () => {
      const distros = [
        { name: 'Ubuntu', state: 'Running' },
        { name: 'Kali', state: 'Stopped' },
      ]
      mocks.isWslAvailable.mockResolvedValueOnce(true)
      mocks.listWslDistributions.mockResolvedValueOnce(distros)

      const wsl = useWsl()
      await wsl.loadDistros()

      expect(wsl.isAvailable.value).toBe(true)
      expect(wsl.distros.value).toEqual(distros)
      expect(mocks.listWslDistributions).toHaveBeenCalledTimes(1)
    })

    it('should not list distributions when WSL is unavailable', async () => {
      mocks.isWslAvailable.mockResolvedValueOnce(false)

      const wsl = useWsl()
      await wsl.loadDistros()

      expect(wsl.isAvailable.value).toBe(false)
      expect(wsl.distros.value).toEqual([])
      expect(mocks.listWslDistributions).not.toHaveBeenCalled()
    })

    it('should keep previous distros when reloading as unavailable', async () => {
      // 实际契约：重新探测为不可用时不清空旧 distros，消费方以 isAvailable 为门控
      mocks.isWslAvailable.mockResolvedValueOnce(true)
      mocks.listWslDistributions.mockResolvedValueOnce([{ name: 'Ubuntu', state: 'Running' }])

      const wsl = useWsl()
      await wsl.loadDistros()
      expect(wsl.distros.value).toHaveLength(1)

      mocks.isWslAvailable.mockResolvedValueOnce(false)
      await wsl.loadDistros()

      expect(wsl.isAvailable.value).toBe(false)
      expect(wsl.distros.value).toHaveLength(1)
      expect(mocks.listWslDistributions).toHaveBeenCalledTimes(1)
    })

    it('should propagate command failures to the caller', async () => {
      // composable 未捕获错误，reject 应向上抛
      mocks.isWslAvailable.mockRejectedValueOnce(new Error('wsl probe failed'))

      const wsl = useWsl()
      await expect(wsl.loadDistros()).rejects.toThrow('wsl probe failed')
      expect(wsl.isAvailable.value).toBe(false)
    })
  })
})
