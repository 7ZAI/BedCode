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
  theme: string            // shiki theme ID
  tabSize: number          // 2 | 4 | 8
  showLineNumbers: boolean
}

const STORAGE_KEY = 'bedcode-code-viewer-settings'

const defaultSettings: CodeViewerSettings = {
  fontSize: 13,
  theme: 'vitesse-dark',
  tabSize: 4,
  showLineNumbers: true,
}

/** 可选代码主题配置 */
export const CODE_THEMES: Record<string, { label: string; background: string; foreground: string }> = {
  'vitesse-dark': { label: 'Vitesse Dark', background: '#121212', foreground: '#dbd7ca' },
  'one-dark-pro': { label: 'One Dark Pro', background: '#282c34', foreground: '#abb2bf' },
  'nord':         { label: 'Nord',         background: '#2e3440', foreground: '#d8dee9' },
  'github-dark':  { label: 'GitHub Dark',  background: '#0d1117', foreground: '#e6edf3' },
  'monokai':      { label: 'Monokai',      background: '#272822', foreground: '#f8f8f2' },
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
