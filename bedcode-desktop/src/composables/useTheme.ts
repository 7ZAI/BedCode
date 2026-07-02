import { ref, watch, computed } from 'vue'
import { useSettingsStore } from '@/stores/settings'

const isSystemDark = ref(false)
let systemThemeQuery: MediaQueryList | null = null

const systemThemeHandler = (e: MediaQueryListEvent) => {
  document.documentElement.classList.toggle('dark', e.matches)
  isSystemDark.value = e.matches
}

function applyTheme(theme: string) {
  const root = document.documentElement
  const isDark = theme === 'system' ? isSystemDark.value : theme === 'dark'

  if (isDark) {
    root.classList.add('dark')
  } else {
    root.classList.remove('dark')
  }
}

function setupTheme() {
  const settingsStore = useSettingsStore()
  const theme = settingsStore.settings.ui.theme

  if (theme === 'system') {
    isSystemDark.value = window.matchMedia('(prefers-color-scheme: dark)').matches
    systemThemeQuery = window.matchMedia('(prefers-color-scheme: dark)')
    systemThemeQuery.addEventListener('change', systemThemeHandler)
  }

  applyTheme(theme)
}

function cleanupTheme() {
  if (systemThemeQuery) {
    systemThemeQuery.removeEventListener('change', systemThemeHandler)
    systemThemeQuery = null
  }
}

export function useTheme() {
  const settingsStore = useSettingsStore()

  // 监听主题设置变化
  watch(() => settingsStore.settings.ui.theme, (newTheme) => {
    cleanupTheme()
    applyTheme(newTheme)
    setupTheme()
  })

  // 主题切换通过 :root.dark CSS 变量自动生效，无需 dark: 前缀
  const themeClasses = computed(() => ({
    container: 'min-h-screen bg-page text-[var(--text-primary)]'
  }))

  return {
    isSystemDark,
    themeClasses,
    setupTheme,
    cleanupTheme,
  }
}
