/**
 * useMobileSettings - 移动端本地设置共享状态
 *
 * 设置主页（SettingsView）与各分类二级页面共享同一份模块级单例状态，
 * 保证跨路由页面数据一致。设置变更自动保存到 localStorage 与后端数据库。
 */
import { ref, computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '@/stores/settings'
import { useI18nStore } from '@/stores/i18n'

/** 移动端本地设置（用于 UI 控制） */
export interface MobileSettings {
  autoReconnect: boolean
  keepAlive: boolean
  reconnectInterval: number
  defaultPort: number
  notifyOnWaiting: boolean
  notifyOnConnection: boolean
  notifyInBackground: boolean
  vibrate: boolean
  soundOnTaskComplete: boolean
  fontSize: 'small' | 'medium' | 'large'
  maxCachedTerminals: number
  /** 优先认证方式：配对码 / 生物认证 */
  preferredAuthMethod: 'pairing_code' | 'biometric'
}

export const defaultMobileSettings: MobileSettings = {
  autoReconnect: true,
  keepAlive: true,
  reconnectInterval: 5,
  defaultPort: 8765,
  notifyOnWaiting: true,
  notifyOnConnection: true,
  notifyInBackground: true,
  vibrate: true,
  soundOnTaskComplete: true,
  fontSize: 'medium',
  maxCachedTerminals: 10,
  preferredAuthMethod: 'pairing_code',
}

/** 字体大小映射到终端字体大小 */
const fontSizeMap = {
  small: 12,
  medium: 14,
  large: 16,
}

// 模块级单例状态，跨设置主页与二级页面共享
const settings = ref<MobileSettings>({ ...defaultMobileSettings })
// 加载 Promise 缓存，保证 loadSettings 幂等且并发安全
let loadPromise: Promise<void> | null = null

/** 将移动端设置同步到全局 settingsStore（使设置生效） */
function syncToSettingsStore() {
  const settingsStore = useSettingsStore()
  const terminalFontSize = fontSizeMap[settings.value.fontSize]

  settingsStore.saveSettings({
    ui: {
      ...settingsStore.settings.ui,
      terminal_font_size: terminalFontSize,
    }
  })
}

/** 保存设置到 localStorage 与后端数据库 */
function saveSettings() {
  localStorage.setItem('mobile-settings', JSON.stringify(settings.value))
  syncToSettingsStore()

  for (const [key, value] of Object.entries(settings.value)) {
    invoke('set_db_setting', {
      key: `mobile.${key}`,
      value: String(value),
    }).catch(() => {})
  }
}

/** 加载设置（幂等）：localStorage → 后端数据库 → 同步到全局 settingsStore */
async function loadSettings(): Promise<void> {
  if (loadPromise) return loadPromise

  loadPromise = (async () => {
    const settingsStore = useSettingsStore()
    // 先等待 settingsStore 加载完成
    await settingsStore.loadSettings()

    // 加载已保存的设置
    const saved = localStorage.getItem('mobile-settings')
    if (saved) {
      try {
        const parsed = JSON.parse(saved)
        settings.value = { ...defaultMobileSettings, ...parsed }
      } catch (e) {
        console.error('Failed to load settings:', e)
      }
    }

    // 尝试从后端加载移动端设置并同步
    try {
      const dbSettings = await invoke<Array<{ key: string; value: string }>>('get_all_db_settings')
      for (const s of dbSettings) {
        if (s.key.startsWith('mobile.')) {
          const settingKey = s.key.replace('mobile.', '')
          const value = s.value === 'true' ? true : s.value === 'false' ? false : isNaN(Number(s.value)) ? s.value : Number(s.value)
          ;(settings.value as any)[settingKey] = value
        }
      }
    } catch {
      // Backend may not be available
    }

    // 同步到 settingsStore（使设置生效）
    syncToSettingsStore()
  })()

  return loadPromise
}

/** 重置为默认设置（主题恢复跟随系统、语言恢复中文），并立即持久化 */
async function resetSettings(): Promise<void> {
  const settingsStore = useSettingsStore()
  const i18nStore = useI18nStore()

  settings.value = { ...defaultMobileSettings }
  await settingsStore.saveSettings({
    ui: {
      ...settingsStore.settings.ui,
      theme: 'system',
    }
  })
  await i18nStore.setLanguage('zh-CN')
  syncToSettingsStore()
  saveSettings()
}

// 设置变更自动保存
watch(settings, saveSettings, { deep: true })

export function useMobileSettings() {
  const settingsStore = useSettingsStore()
  const i18nStore = useI18nStore()

  /** 主题模式 - 直接绑定到全局 settingsStore */
  const themeMode = computed({
    get: () => settingsStore.settings.ui.theme,
    set: (value: string) => {
      settingsStore.saveSettings({
        ui: {
          ...settingsStore.settings.ui,
          theme: value
        }
      })
    }
  })

  /** 当前语言 */
  const currentLanguage = computed({
    get: () => settingsStore.settings.ui.language || 'zh-CN',
    set: (value: string) => i18nStore.setLanguage(value),
  })

  return {
    settings,
    themeMode,
    currentLanguage,
    loadSettings,
    saveSettings,
    resetSettings,
  }
}
