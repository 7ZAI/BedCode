import { defineStore } from 'pinia'
import { logger } from '@/utils/frontendLogger'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface Settings {
  network: {
    port: number
    // QR 码使用的 IP 地址（用于多网卡环境）
    qr_host?: string
  }
  session: {
    default_environment: string
    default_wsl_distro?: string
    default_working_dir?: string
    default_command?: string
    session_timeout: number
  }
  ui: {
    theme: string
    terminal_font_size: number
    terminal_font_family: string
    show_preview: boolean
    // 语言偏好
    language?: string
    // 移动端终端页面缓存最大数量
    max_cached_terminals?: number
    // 是否在后台时发送通知
    notify_in_background?: boolean
    // 主色色板已于 2026-10-16 移出宿主（归各 wasm-app 自持，见 settings.accentPalette）；
    // 类型里不再有 palette 字段，loadSettings 也会把它从历史存档里剔除。
  }
}

const defaultSettings: Settings = {
  network: {
    port: 8765,
  },
  session: {
    default_environment: 'windows',
    default_wsl_distro: undefined,
    default_working_dir: undefined,
    default_command: 'claude',
    session_timeout: 3600,
  },
  ui: {
    theme: 'system',
    terminal_font_size: 12,
    terminal_font_family: 'Consolas',
    show_preview: true,
    language: 'zh-CN',
    max_cached_terminals: 10,
    notify_in_background: true,
  },
}

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<Settings>(JSON.parse(JSON.stringify(defaultSettings)))

  async function loadSettings() {
    try {
      const loaded = await invoke<Settings>('get_app_settings')
      // ui 里显式剔除历史遗留的 palette：主色已下沉到 wasm-app 自持的 accentPalette
      // （mobile.accentPalette），宿主再留着这个键只会让旧值看起来「还在生效」。
      // 走 Record 断言是因为 Settings['ui'] 已不再声明 palette，但后端存档里可能还有。
      const { palette: _retiredPalette, ...ui } = {
        ...defaultSettings.ui,
        ...loaded.ui,
      } as Record<string, unknown> & Settings['ui']
      settings.value = {
        network: { ...defaultSettings.network, ...loaded.network },
        session: { ...defaultSettings.session, ...loaded.session },
        ui,
      }
    } catch (e) {
      logger.error('[Settings] Failed to load settings:', e)
    }
  }

  async function saveSettings(newSettings: Partial<Settings>) {
    try {
      const merged = { ...settings.value, ...newSettings }
      await invoke('save_app_settings', { settings: merged })
      settings.value = merged
    } catch (e) {
      logger.error('[Settings] Failed to save settings:', e)
    }
  }

  // 获终端缓存最大数量
  function getMaxCachedTerminals(): number {
    return settings.value.ui.max_cached_terminals || 10
  }

  return {
    settings,
    loadSettings,
    saveSettings,
    getMaxCachedTerminals,
  }
})
