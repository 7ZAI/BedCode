import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface Settings {
  network: {
    port: number
    service_name: string
    enable_discovery: boolean
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
  }
}

const defaultSettings: Settings = {
  network: {
    port: 8765,
    service_name: 'bedcode',
    enable_discovery: true,
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
    terminal_font_size: 14,
    terminal_font_family: 'Consolas',
    show_preview: true,
  },
}

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<Settings>(JSON.parse(JSON.stringify(defaultSettings)))

  async function loadSettings() {
    try {
      const loaded = await invoke<Settings>('get_app_settings')
      // Deep merge with defaults
      settings.value = {
        network: { ...defaultSettings.network, ...loaded.network },
        session: { ...defaultSettings.session, ...loaded.session },
        ui: { ...defaultSettings.ui, ...loaded.ui },
      }
    } catch (e) {
      console.error('Failed to load settings:', e)
    }
  }

  async function saveSettings(newSettings: Partial<Settings>) {
    try {
      const merged = { ...settings.value, ...newSettings }
      await invoke('save_app_settings', { settings: merged })
      settings.value = merged
    } catch (e) {
      console.error('Failed to save settings:', e)
    }
  }

  return {
    settings,
    loadSettings,
    saveSettings,
  }
})
