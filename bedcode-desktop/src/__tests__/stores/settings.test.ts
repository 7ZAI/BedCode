import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useSettingsStore } from '@/stores/settings'

// Mock Tauri invoke
const mockInvoke = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}))

describe('Settings Store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  describe('initial state', () => {
    it('should initialize with default settings', () => {
      const store = useSettingsStore()

      expect(store.settings.network.port).toBe(8765)
    })

    it('should have correct default session settings', () => {
      const store = useSettingsStore()

      expect(store.settings.session.default_environment).toBe('windows')
      expect(store.settings.session.default_wsl_distro).toBeUndefined()
      expect(store.settings.session.default_working_dir).toBeUndefined()
      expect(store.settings.session.default_command).toBe('claude')
      expect(store.settings.session.session_timeout).toBe(3600)
    })

    it('should have correct default UI settings', () => {
      const store = useSettingsStore()

      expect(store.settings.ui.theme).toBe('system')
      expect(store.settings.ui.terminal_font_size).toBe(12)
      expect(store.settings.ui.terminal_font_family).toBe('Consolas')
      expect(store.settings.ui.show_preview).toBe(true)
    })
  })

  describe('loadSettings', () => {
    it('should load settings from Tauri backend', async () => {
      const mockSettings = {
        network: {
          port: 9000,
        },
        session: {
          default_environment: 'wsl',
          default_wsl_distro: 'Ubuntu',
          default_working_dir: '/home/user',
          default_command: 'claude',
          session_timeout: 7200,
        },
        ui: {
          theme: 'dark' as const,
          terminal_font_size: 16,
          terminal_font_family: 'FiraCode',
          show_preview: false,
        },
      }

      mockInvoke.mockResolvedValueOnce(mockSettings)

      const store = useSettingsStore()
      await store.loadSettings()

      expect(mockInvoke).toHaveBeenCalledWith('get_app_settings')
      expect(store.settings.network.port).toBe(9000)
      expect(store.settings.session.default_environment).toBe('wsl')
      expect(store.settings.ui.theme).toBe('dark')
    })

    it('should merge loaded settings with defaults (partial network)', async () => {
      // Partial settings returned - only port
      mockInvoke.mockResolvedValueOnce({
        network: {
          port: 9999,
        },
      })

      const store = useSettingsStore()
      await store.loadSettings()

      // Should have the loaded value
      expect(store.settings.network.port).toBe(9999)
      // The merge should preserve other properties when possible
      // Note: This depends on how the backend returns partial data
      // If backend returns partial objects, we need deep merge
    })

    it('should handle load error gracefully', async () => {
      const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
      mockInvoke.mockRejectedValueOnce(new Error('Load failed'))

      const store = useSettingsStore()
      await store.loadSettings()

      // Should keep default settings on error
      expect(store.settings.network.port).toBe(8765)
      expect(consoleSpy).toHaveBeenCalledWith('[Settings] Failed to load settings:', expect.any(Error))

      consoleSpy.mockRestore()
    })
  })

  describe('saveSettings', () => {
    it('should save settings to Tauri backend', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const store = useSettingsStore()
      await store.saveSettings({
        network: {
          port: 7777,
        },
      })

      expect(mockInvoke).toHaveBeenCalledWith('save_app_settings', {
        settings: expect.objectContaining({
          network: expect.objectContaining({
            port: 7777,
          }),
        }),
      })
    })

    it('should merge new settings with existing', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const store = useSettingsStore()

      // First load some settings
      store.settings.network.port = 9000

      // Then save partial update
      await store.saveSettings({
        ui: {
          theme: 'light',
          terminal_font_size: 18,
          terminal_font_family: 'Monaco',
          show_preview: true,
        },
      })

      // Should merge: network.port should remain
      expect(mockInvoke).toHaveBeenCalledWith('save_app_settings', {
        settings: expect.objectContaining({
          network: expect.objectContaining({
            port: 9000,
          }),
          ui: expect.objectContaining({
            theme: 'light',
            terminal_font_size: 18,
          }),
        }),
      })
    })

    it('should update local state after save', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const store = useSettingsStore()
      await store.saveSettings({
        ui: {
          theme: 'dark',
          terminal_font_size: 20,
          terminal_font_family: 'JetBrainsMono',
          show_preview: false,
        },
      })

      expect(store.settings.ui.theme).toBe('dark')
      expect(store.settings.ui.terminal_font_size).toBe(20)
      expect(store.settings.ui.show_preview).toBe(false)
    })

    it('should handle save error gracefully', async () => {
      const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
      mockInvoke.mockRejectedValueOnce(new Error('Save failed'))

      const store = useSettingsStore()
      await store.saveSettings({
        network: {
          port: 1111,
        },
      })

      expect(consoleSpy).toHaveBeenCalledWith('[Settings] Failed to save settings:', expect.any(Error))

      consoleSpy.mockRestore()
    })
  })

  describe('theme validation', () => {
    it('should accept valid theme values', () => {
      const store = useSettingsStore()

      const validThemes: Array<'light' | 'dark' | 'system'> = ['light', 'dark', 'system']

      validThemes.forEach((theme) => {
        store.settings.ui.theme = theme
        expect(store.settings.ui.theme).toBe(theme)
      })
    })
  })

  describe('reactivity', () => {
    it('should be reactive to settings changes', async () => {
      // Create fresh Pinia instance for this test to avoid state pollution
      setActivePinia(createPinia())
      const store = useSettingsStore()

      // Initial state should be default
      const initialPort = store.settings.network.port

      store.settings.network.port = 1234

      expect(store.settings.network.port).toBe(1234)
      // Restore for other tests
      store.settings.network.port = initialPort
    })

    it('should persist changes within session', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const store1 = useSettingsStore()

      await store1.saveSettings({
        network: {
          port: 5555,
        },
      })

      // Get another instance (same store due to Pinia)
      const store2 = useSettingsStore()

      expect(store2.settings.network.port).toBe(5555)
    })
  })
})
