import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface Settings {
  network: {
    port: number
    // QR 码使用的 IP 地址（用于多网卡环境）
    qr_host?: string
    // 服务器运行时阻止系统休眠
    prevent_sleep?: boolean
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
    // 色板（warm 暖调工作台，未来可扩展）
    theme_palette?: string
    // 全局界面字体大小（终端字体大小由 terminal_font_size 独立控制）
    font_size: number
    terminal_font_size: number
    terminal_font_family: string
    terminal_theme: string
    show_preview: boolean
    // 语言偏好
    language?: string
    // 移动端终端页面缓存最大数量
    max_cached_terminals?: number
    // 是否在后台时发送通知
    notify_in_background?: boolean
    // 终端背景图片文件名（位于应用数据目录，空/未设置表示不启用）
    terminal_bg_image?: string | null
    // 终端背景图片不透明度（0-100，越小图片越淡）
    terminal_bg_opacity?: number
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
    theme_palette: 'warm',
    font_size: 12,
    terminal_font_size: 12,
    terminal_font_family: 'Consolas',
    terminal_theme: 'dracula',
    show_preview: true,
    language: 'zh-CN',
    max_cached_terminals: 10,
    notify_in_background: true,
    terminal_bg_image: undefined,
    terminal_bg_opacity: 30,
  },
}

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<Settings>(JSON.parse(JSON.stringify(defaultSettings)))
  // 最近一次成功保存内容的 JSON 快照：deep watch 触发时对比内容判断是否已持久化。
  // 不能用对象引用比对——Pinia ref 赋值会包一层 reactive proxy，settings.value
  // 永远不等于原始对象；且用户变更发生在同一对象上，引用比对也无法区分新旧状态
  let lastSavedSnapshot: string | null = null
  // 保存序号：并发保存时仅最后一次的响应回写 store，先发慢回的旧响应不得覆盖新值
  let saveSeq = 0

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
      const mySeq = ++saveSeq
      await invoke('save_app_settings', { settings: merged })
      // 期间已有更新的保存请求：回写会覆盖更新值，丢弃本次回写
      if (mySeq !== saveSeq) return
      settings.value = merged
      lastSavedSnapshot = JSON.stringify(merged)
    } catch (e) {
      console.error('[Settings] Failed to save settings:', e)
    }
  }

  /** 当前 store 状态是否与最近一次持久化一致（避免保存回写触发重复保存） */
  function isPersisted(current: Settings): boolean {
    return lastSavedSnapshot !== null && JSON.stringify(current) === lastSavedSnapshot
  }

  // 获��终端缓存最大数量
  function getMaxCachedTerminals(): number {
    return settings.value.ui.max_cached_terminals || 10
  }

  return {
    settings,
    loadSettings,
    saveSettings,
    isPersisted,
    getMaxCachedTerminals,
  }
})
