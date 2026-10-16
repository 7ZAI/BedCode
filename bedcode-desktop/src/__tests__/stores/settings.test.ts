/**
 * Settings Store 行为契约
 *
 * 契约清单（收敛后）：
 * - C1 初始默认：network.port = 8765；ui 主题为 system、终端字体 12/Consolas/dracula
 * - C2 loadSettings：取 `get_app_settings` 回执与默认值浅合并（缺字段保留默认）
 * - C3 loadSettings 失败：保留默认值并记错误日志（不抛）
 * - C4 saveSettings：`save_app_settings` 收到与现有状态合并后的整表
 * - C5 saveSettings 局部更新：未提及的小节保持原值（network.port 不被覆盖）
 * - C6 saveSettings 成功回写本地状态
 * - C7 saveSettings 失败：记错误日志且不回写（不制造「已保存」假象）
 * - C8 主题可写（light/dark/system）
 *
 * 注：原先断言的 `session.*`（默认执行环境/命令/超时）、`ui.show_preview`、
 * `network.qr_host`、`ui.max_cached_terminals`、`ui.notify_in_background` 已随
 * 归域迁移删除（会话默认值在 `com.bedcode.terminal-session` 插件存储），对应断言一并移除。
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import { logger } from '@/utils/frontendLogger'
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
    it('C1 should initialize with default network settings', () => {
      const store = useSettingsStore()

      expect(store.settings.network.port).toBe(8765)
    })

    it('C1 should have correct default UI settings', () => {
      const store = useSettingsStore()

      expect(store.settings.ui.theme).toBe('system')
      expect(store.settings.ui.theme_palette).toBe('warm')
      expect(store.settings.ui.terminal_font_size).toBe(12)
      expect(store.settings.ui.terminal_font_family).toBe('Consolas')
      expect(store.settings.ui.terminal_theme).toBe('dracula')
      expect(store.settings.ui.terminal_bg_opacity).toBe(30)
    })
  })

  describe('loadSettings', () => {
    it('C2 should load settings from Tauri backend and merge with defaults', async () => {
      const mockSettings = {
        network: {
          port: 9000,
        },
        ui: {
          theme: 'dark' as const,
          terminal_font_size: 16,
          terminal_font_family: 'FiraCode',
        },
      }

      mockInvoke.mockResolvedValueOnce(mockSettings)

      const store = useSettingsStore()
      await store.loadSettings()

      expect(mockInvoke).toHaveBeenCalledWith('get_app_settings')
      expect(store.settings.network.port).toBe(9000)
      expect(store.settings.ui.theme).toBe('dark')
      expect(store.settings.ui.terminal_font_size).toBe(16)
      expect(store.settings.ui.terminal_font_family).toBe('FiraCode')
      // 回执未包含的字段保留默认值（浅合并不丢字段）
      expect(store.settings.ui.terminal_theme).toBe('dracula')
    })

    it('C2 should keep defaults when backend returns partial network only', async () => {
      mockInvoke.mockResolvedValueOnce({
        network: {
          port: 9999,
        },
      })

      const store = useSettingsStore()
      await store.loadSettings()

      expect(store.settings.network.port).toBe(9999)
      expect(store.settings.ui.terminal_font_family).toBe('Consolas')
    })

    it('C3 should handle load error gracefully', async () => {
      const consoleSpy = vi.spyOn(logger, 'error').mockImplementation(() => {})
      mockInvoke.mockRejectedValueOnce(new Error('Load failed'))

      const store = useSettingsStore()
      await store.loadSettings()

      // 出错保留默认设置
      expect(store.settings.network.port).toBe(8765)
      expect(consoleSpy).toHaveBeenCalledWith(
        '[Settings] Failed to load settings:',
        expect.any(Error),
      )

      consoleSpy.mockRestore()
    })
  })

  describe('saveSettings', () => {
    it('C4 should save settings to Tauri backend', async () => {
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

    it('C5 should merge new settings with existing', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const store = useSettingsStore()

      store.settings.network.port = 9000

      await store.saveSettings({
        ui: {
          theme: 'light',
          terminal_font_size: 18,
          terminal_font_family: 'Monaco',
        },
      })

      // 未提及的小节保持原值
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

    it('C6 should update local state after save', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const store = useSettingsStore()
      await store.saveSettings({
        ui: {
          theme: 'dark',
          terminal_font_size: 20,
          terminal_font_family: 'JetBrainsMono',
        },
      })

      expect(store.settings.ui.theme).toBe('dark')
      expect(store.settings.ui.terminal_font_size).toBe(20)
      expect(store.settings.ui.terminal_font_family).toBe('JetBrainsMono')
    })

    it('C7 should handle save error gracefully without writing back', async () => {
      const consoleSpy = vi.spyOn(logger, 'error').mockImplementation(() => {})
      mockInvoke.mockRejectedValueOnce(new Error('Save failed'))

      const store = useSettingsStore()
      await store.saveSettings({
        network: {
          port: 1111,
        },
      })

      expect(consoleSpy).toHaveBeenCalledWith(
        '[Settings] Failed to save settings:',
        expect.any(Error),
      )
      // 失败不回写：界面不应显示未落盘的端口
      expect(store.settings.network.port).toBe(8765)

      consoleSpy.mockRestore()
    })
  })

  describe('theme validation', () => {
    it('C8 should accept valid theme values', () => {
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
      setActivePinia(createPinia())
      const store = useSettingsStore()

      const initialPort = store.settings.network.port

      store.settings.network.port = 1234

      expect(store.settings.network.port).toBe(1234)
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

      const store2 = useSettingsStore()

      expect(store2.settings.network.port).toBe(5555)
    })
  })
})
