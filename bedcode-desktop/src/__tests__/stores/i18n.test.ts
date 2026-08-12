import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useI18nStore } from '@/stores/i18n'
import { useSettingsStore } from '@/stores/settings'

// Mock Tauri invoke（settings store 依赖后端持久化）
const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}))

// Mock i18n 实例：仅暴露 locale ref，避免引入真实 vue-i18n 全量依赖
const mocks = vi.hoisted(() => ({
  localeValue: { value: 'zh-CN' },
}))
vi.mock('@/locales', () => ({
  default: { global: { locale: mocks.localeValue } },
}))

describe('I18n Store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    mocks.localeValue.value = 'zh-CN'
  })

  describe('initLanguage', () => {
    it('should keep current locale when saved language matches it', async () => {
      // 默认 settings.ui.language 为 'zh-CN'，与当前 locale 一致
      const store = useI18nStore()
      await store.initLanguage()

      expect(mocks.localeValue.value).toBe('zh-CN')
      expect(mockInvoke).not.toHaveBeenCalled()
    })

    it('should apply saved language when it differs from current locale', async () => {
      const settingsStore = useSettingsStore()
      settingsStore.settings.ui.language = 'en'

      const store = useI18nStore()
      await store.initLanguage()

      expect(mocks.localeValue.value).toBe('en')
    })

    it('should do nothing when saved language is not set', async () => {
      const settingsStore = useSettingsStore()
      settingsStore.settings.ui.language = undefined

      const store = useI18nStore()
      await store.initLanguage()

      expect(mocks.localeValue.value).toBe('zh-CN')
    })
  })

  describe('setLanguage', () => {
    it('should return early when language is already active', async () => {
      const store = useI18nStore()
      await store.setLanguage('zh-CN')

      expect(mocks.localeValue.value).toBe('zh-CN')
      expect(mockInvoke).not.toHaveBeenCalled()
    })

    it('should switch locale and persist the preference', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const store = useI18nStore()
      await store.setLanguage('en')

      expect(mocks.localeValue.value).toBe('en')
      expect(mockInvoke).toHaveBeenCalledWith('save_app_settings', {
        settings: expect.objectContaining({
          ui: expect.objectContaining({
            language: 'en',
            theme: 'system',
            terminal_font_size: 12,
          }),
        }),
      })
    })

    it('should update the settings store state after save', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const settingsStore = useSettingsStore()
      const store = useI18nStore()
      await store.setLanguage('en')

      expect(settingsStore.settings.ui.language).toBe('en')
    })

    it('should not persist again when switching back to the same language', async () => {
      mockInvoke.mockResolvedValue(undefined)

      const store = useI18nStore()
      await store.setLanguage('en')
      await store.setLanguage('en')

      expect(mockInvoke).toHaveBeenCalledTimes(1)
    })
  })
})
