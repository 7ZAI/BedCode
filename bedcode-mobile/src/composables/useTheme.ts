import { ref, watch, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '@/stores/settings'
import { logger } from '@/utils/frontendLogger'

const isSystemDark = ref(false)
let systemThemeQuery: MediaQueryList | null = null

/**
 * 同步系统状态栏/导航栏图标外观（仅 Android 生效，其他平台 invoke 失败被捕获）
 *
 * edge-to-edge 下系统栏透明、App 背景延伸到其后：App 深色配浅色图标、浅色配深色
 * 图标。否则设备浅色 + App 夜间模式时深色背景上仍是深色图标，状态栏时间/日期
 * 不可读。由 Kotlin StatusBarStylePlugin 实现（经 commands::android 命令）。
 */
async function syncSystemBarAppearance(isDark: boolean) {
  try {
    await invoke('set_status_bar_style', { dark: isDark })
  } catch (e) {
    // 桌面/浏览器等无该命令的平台静默降级为不处理系统栏
    logger.warn('[Theme] Failed to sync system bar appearance:', e)
  }
}

const systemThemeHandler = (e: MediaQueryListEvent) => {
  document.documentElement.classList.toggle('dark', e.matches)
  isSystemDark.value = e.matches
  void syncSystemBarAppearance(e.matches)
}

function applyTheme(theme: string) {
  const root = document.documentElement
  const isDark = theme === 'system' ? isSystemDark.value : theme === 'dark'

  if (isDark) {
    root.classList.add('dark')
  } else {
    root.classList.remove('dark')
  }

  // 系统栏图标外观跟随 App 主题（而非设备主题）
  void syncSystemBarAppearance(isDark)

  // 主色色板：与桌面端同名 palette 同源（背景/文字等其余 token 保持移动端自身风格）
  const palette = useSettingsStore().settings.ui.palette || 'default'
  root.setAttribute('data-palette', palette)
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

  // 监听主色色板变化（palette 与 theme 独立，无需重建 system 监听）
  watch(() => settingsStore.settings.ui.palette, () => {
    applyTheme(settingsStore.settings.ui.theme)
  })

  // 主题对应的容器类名
  const themeClasses = computed(() => {
    const theme = settingsStore.settings.ui.theme
    const isDark = theme === 'system' ? isSystemDark.value : theme === 'dark'

    return {
      container: isDark
        ? 'min-h-screen bg-slate-50 dark:bg-dark-900 text-slate-900 dark:text-dark-100'
        : 'min-h-screen bg-slate-50 text-slate-900'
    }
  })

  return {
    isSystemDark,
    themeClasses,
    setupTheme,
    cleanupTheme,
  }
}
