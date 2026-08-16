import { defineStore } from 'pinia'
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
    // 主色色板（与桌面端同名色板同源；default = Dracula 象牙白）
    palette?: string
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
    palette: 'default',
  },
}

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<Settings>(JSON.parse(JSON.stringify(defaultSettings)))

  async function loadSettings() {
    try {
      const loaded = await invoke<Settings>('get_app_settings')
      settings.value = {
        network: { ...defaultSettings.network, ...loaded.network },
        session: { ...defaultSettings.session, ...loaded.session },
        ui: { ...defaultSettings.ui, ...loaded.ui },
      }
    } catch (e) {
      console.error('[Settings] Failed to load settings:', e)
    }
  }

  async function saveSettings(newSettings: Partial<Settings>) {
    try {
      const merged = { ...settings.value, ...newSettings }
      await invoke('save_app_settings', { settings: merged })
      settings.value = merged
    } catch (e) {
      console.error('[Settings] Failed to save settings:', e)
    }
  }

  // 获��终端缓存最大数量
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
