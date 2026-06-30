/**
 * i18n Store
 *
 * 管理语言切换和持久化，语言偏好存储在 Settings.ui.language
 */
import { defineStore } from 'pinia'
import i18n from '@/locales'
import { useSettingsStore } from './settings'

export const useI18nStore = defineStore('i18n', () => {
  /** 初始化语言偏好，应用启动时调用 */
  async function initLanguage() {
    const settingsStore = useSettingsStore()
    const savedLang = settingsStore.settings.ui.language
    if (savedLang && savedLang !== i18n.global.locale.value) {
      i18n.global.locale.value = savedLang as 'zh-CN' | 'en'
    }
  }

  /** 切换语言并持久化 */
  async function setLanguage(lang: string) {
    if (lang === i18n.global.locale.value) return
    i18n.global.locale.value = lang as 'zh-CN' | 'en'
    const settingsStore = useSettingsStore()
    await settingsStore.saveSettings({
      ui: { ...settingsStore.settings.ui, language: lang },
    })
  }

  return { initLanguage, setLanguage }
})
