import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useWslStore } from '@/stores/wsl'

// Mock useDesktopCommands（WSL 命令的数据源）
const mocks = vi.hoisted(() => ({
  isWslAvailable: vi.fn(),
  listWslDistributions: vi.fn(),
}))
vi.mock('@/composables/useDesktopCommands', () => mocks)

describe('WSL Store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  describe('initial state', () => {
    it('should initialize with loading state and empty data', () => {
      const store = useWslStore()

      expect(store.isLoading).toBe(true)
      expect(store.isAvailable).toBe(false)
      expect(store.distros).toEqual([])
      expect(store.error).toBeNull()
    })
  })

  describe('loadWslInfo', () => {
    it('should load distributions when WSL is available', async () => {
      const distros = [
        { name: 'Ubuntu', state: 'Running' },
        { name: 'Debian', state: 'Stopped' },
      ]
      mocks.isWslAvailable.mockResolvedValueOnce(true)
      mocks.listWslDistributions.mockResolvedValueOnce(distros)

      const store = useWslStore()
      await store.loadWslInfo()

      expect(mocks.isWslAvailable).toHaveBeenCalledTimes(1)
      expect(mocks.listWslDistributions).toHaveBeenCalledTimes(1)
      expect(store.isAvailable).toBe(true)
      expect(store.distros).toEqual(distros)
      expect(store.isLoading).toBe(false)
      expect(store.error).toBeNull()
    })

    it('should skip listing when WSL is unavailable', async () => {
      mocks.isWslAvailable.mockResolvedValueOnce(false)

      const store = useWslStore()
      await store.loadWslInfo()

      expect(store.isAvailable).toBe(false)
      expect(store.distros).toEqual([])
      expect(mocks.listWslDistributions).not.toHaveBeenCalled()
      expect(store.isLoading).toBe(false)
    })

    it('should record error message and stay not available on failure', async () => {
      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
      mocks.isWslAvailable.mockRejectedValueOnce(new Error('wsl is broken'))

      const store = useWslStore()
      await store.loadWslInfo()

      expect(store.isAvailable).toBe(false)
      expect(store.distros).toEqual([])
      expect(store.error).toBe('wsl is broken')
      expect(store.isLoading).toBe(false)
      expect(warnSpy).toHaveBeenCalledWith('[WslStore] Failed to load WSL info:', expect.any(Error))
      warnSpy.mockRestore()
    })

    it('should clear previous error before reloading', async () => {
      mocks.isWslAvailable.mockRejectedValueOnce(new Error('first failure'))
      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})

      const store = useWslStore()
      await store.loadWslInfo()
      expect(store.error).toBe('first failure')

      // 第二次加载成功：error 应被清空
      mocks.isWslAvailable.mockResolvedValueOnce(true)
      mocks.listWslDistributions.mockResolvedValueOnce([{ name: 'Ubuntu', state: 'Running' }])
      await store.loadWslInfo()

      expect(store.error).toBeNull()
      expect(store.isAvailable).toBe(true)
      expect(store.distros).toHaveLength(1)
      warnSpy.mockRestore()
    })
  })
})
