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
  fontSize: 'normal' | 'large' | 'xlarge'
  /** 最大可同时打开的终端数量 */
  maxOpenTerminals: number
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
  fontSize: 'normal',
  maxOpenTerminals: 5,
  preferredAuthMethod: 'pairing_code',
}

/** 字体大小映射到终端字体大小（正常 = 旧版“中”，向上提供大、超大两档） */
const fontSizeMap = {
  normal: 14,
  large: 16,
  xlarge: 18,
}

/** UI 字号缩放系数：作用于 --font-size-* 变量（与终端字号档位一致的比例） */
const uiFontScaleMap = {
  normal: 1,
  large: 1.125,
  xlarge: 1.25,
} as const

/** 旧版三档字体大小迁移到新档位 */
const legacyFontSizeMap: Record<string, MobileSettings['fontSize']> = {
  small: 'normal',
  medium: 'normal',
  large: 'large',
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

  // UI 字号缩放：html 内联变量优先级高于 .mobile-ui 块内定义，覆盖继承默认值 1
  document.documentElement.style.setProperty('--mobile-font-scale', String(uiFontScaleMap[settings.value.fontSize]))
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
        // 旧字段 maxCachedTerminals（终端缓存数量）迁移到 maxOpenTerminals
        const { maxCachedTerminals, ...rest } = parsed
        settings.value = { ...defaultMobileSettings, ...rest }
        if (rest.maxOpenTerminals == null && typeof maxCachedTerminals === 'number') {
          settings.value.maxOpenTerminals = maxCachedTerminals
        }
      } catch (e) {
        console.error('Failed to load settings:', e)
      }
    }

    // 尝试从后端加载移动端设置并同步
    try {
      const dbSettings = await invoke<Array<{ key: string; value: string }>>('get_all_db_settings')
      // DB 中同时存在新旧 key 时只用新 key，避免旧行覆盖用户已修改的新值
      const hasMaxOpenTerminals = dbSettings.some(s => s.key === 'mobile.maxOpenTerminals')
      for (const s of dbSettings) {
        if (s.key.startsWith('mobile.')) {
          let settingKey = s.key.replace('mobile.', '')
          // 旧字段兼容：终端缓存数量 → 最大可打开终端数量（已有新 key 时跳过旧行）
          if (settingKey === 'maxCachedTerminals') {
            if (hasMaxOpenTerminals) continue
            settingKey = 'maxOpenTerminals'
          }
          const value = s.value === 'true' ? true : s.value === 'false' ? false : isNaN(Number(s.value)) ? s.value : Number(s.value)
          ;(settings.value as any)[settingKey] = value
        }
      }
    } catch {
      // Backend may not be available
    }

    // 迁移旧版设置值（字体档位、终端数量范围）
    migrateLegacySettings()

    // 同步到 settingsStore（使设置生效）
    syncToSettingsStore()
  })()

  return loadPromise
}

/** 归一化旧版设置值：字体档位迁移到新三档，终端数量限制在 1-20 */
function migrateLegacySettings() {
  const fs = settings.value.fontSize as string
  if (!['normal', 'large', 'xlarge'].includes(fs)) {
    settings.value.fontSize = legacyFontSizeMap[fs] ?? 'normal'
  }
  settings.value.maxOpenTerminals = Math.min(20, Math.max(1, Math.round(settings.value.maxOpenTerminals || defaultMobileSettings.maxOpenTerminals)))
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

  /** 主色色板 - 与桌面端同名 palette 同源，两端各自独立选择 */
  const paletteMode = computed({
    get: () => settingsStore.settings.ui.palette || 'default',
    set: (value: string) => {
      settingsStore.saveSettings({
        ui: {
          ...settingsStore.settings.ui,
          palette: value
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
    paletteMode,
    currentLanguage,
    loadSettings,
    saveSettings,
    resetSettings,
  }
}
