/**
 * CodeViewer Store - 代码查看设置
 *
 * 管理移动端代码查看器的字体大小、主题、缩进和行号设置
 * 使用 localStorage 持久化，纯前端状态不经过 Rust 后端
 */
import { defineStore } from 'pinia'
import { ref } from 'vue'

/** 代码查看设置 */
export interface CodeViewerSettings {
  fontSize: number         // 10-24
  lineHeight: number      // 1.0-2.5，步进 0.1
  theme: string            // shiki theme ID 或 'system'
  tabSize: number          // 2 | 4 | 8
  showLineNumbers: boolean
}

const STORAGE_KEY = 'bedcode-code-viewer-settings'

const defaultSettings: CodeViewerSettings = {
  fontSize: 11,
  lineHeight: 1.5,
  theme: 'system',
  tabSize: 4,
  showLineNumbers: true,
}

/** 可选代码主题配置 */
export const CODE_THEMES: Record<string, { label: string; background: string; foreground: string }> = {
  'system':        { label: 'settings.appearance.followSystem', background: 'var(--mobile-bg-secondary)', foreground: 'var(--mobile-text-primary)' },
  // 深色主题
  'vitesse-dark':  { label: 'Vitesse Dark', background: '#121212', foreground: '#dbd7ca' },
  'one-dark-pro':  { label: 'One Dark Pro', background: '#282c34', foreground: '#abb2bf' },
  'nord':          { label: 'Nord',         background: '#2e3440', foreground: '#d8dee9' },
  'github-dark':   { label: 'GitHub Dark',  background: '#0d1117', foreground: '#e6edf3' },
  'monokai':       { label: 'Monokai',      background: '#272822', foreground: '#f8f8f2' },
  // 浅色主题
  'vitesse-light': { label: 'Vitesse Light', background: '#ffffff', foreground: '#393a34' },
  'one-light':     { label: 'One Light',     background: '#fafafa', foreground: '#383a42' },
  'github-light':  { label: 'GitHub Light',  background: '#ffffff', foreground: '#24292f' },
}

/** 深色 → 浅色主题映射，用于 system 模式自动切换 */
const DARK_TO_LIGHT: Record<string, string> = {
  'vitesse-dark': 'vitesse-light',
  'one-dark-pro': 'one-light',
  'github-dark':  'github-light',
}

/** 浅色 → 深色主题映射 */
const LIGHT_TO_DARK: Record<string, string> = {
  'vitesse-light': 'vitesse-dark',
  'one-light':     'one-dark-pro',
  'github-light':  'github-dark',
}

/** 根据 app 主题解析实际使用的 shiki 主题 ID */
export function resolveCodeTheme(settingsTheme: string, isDark: boolean): string {
  if (settingsTheme !== 'system') return settingsTheme
  return isDark ? 'vitesse-dark' : 'vitesse-light'
}

/** 切换明暗时映射主题：深色主题 → 对应浅色主题，反之亦然 */
export function mapThemeForMode(currentTheme: string, isDark: boolean): string {
  if (currentTheme === 'system') return 'system'
  if (isDark && LIGHT_TO_DARK[currentTheme]) return LIGHT_TO_DARK[currentTheme]
  if (!isDark && DARK_TO_LIGHT[currentTheme]) return DARK_TO_LIGHT[currentTheme]
  return currentTheme
}

function loadFromStorage(): CodeViewerSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) {
      const parsed = JSON.parse(raw)
      return { ...defaultSettings, ...parsed }
    }
  } catch (e) {
    console.warn('[CodeViewer] Failed to load settings from localStorage:', e)
  }
  return { ...defaultSettings }
}

export const useCodeViewerStore = defineStore('codeViewer', () => {
  const settings = ref<CodeViewerSettings>(loadFromStorage())

  function saveSettings(newSettings: CodeViewerSettings) {
    settings.value = { ...newSettings }
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(settings.value))
    } catch (e) {
      console.warn('[CodeViewer] Failed to save settings to localStorage:', e)
    }
  }

  function resetSettings() {
    saveSettings({ ...defaultSettings })
  }

  return {
    settings,
    saveSettings,
    resetSettings,
  }
})
