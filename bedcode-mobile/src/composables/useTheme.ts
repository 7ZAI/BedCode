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

  // 主色色板**不**在这里挂：2026-10-16 起主色归各 wasm-app 自持，宿主壳固定用
  // mobile.css 的 `:root` 默认主色（象牙白），应用把 `data-palette` 挂在自己运行面的
  // 根节点上（见 wasm-apps/bedcode/src/app/AppRoot.vue），因此壳与应用互不串色。
  // 历史遗留的 settings.ui.palette 值会被清掉（见 stores/settings.ts 的 migrateUi），
  // 迁到应用的 accentPalette 键，避免旧值在无人写 UI 的情况下悄悄改壳的主色。
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

/**
 * 首次渲染前预应用主题（main.ts 在 app.mount 前调用）：把 .dark 类与
 * data-palette 提前挂到 <html>，并初始化 isSystemDark。
 *
 * 为什么：App.vue 的 setupTheme 在 onMounted 才执行，首帧渲染时 .dark 尚未
 * 上树——mobile.css 的 --mobile-* token 以 html:not(.dark) 为浅色、:root 为深色，
 * 深色用户首帧会以浅色 token 渲染一帧再翻转（启动闪变之二，与 index.html
 * 硬编码 bg-dark-900 的错色闪变并列）。与 App.vue 的 setupTheme 幂等可重复调用。
 */
export function presetThemeBeforeMount() {
  const settingsStore = useSettingsStore()
  const theme = settingsStore.settings.ui.theme
  if (theme === 'system') {
    isSystemDark.value = window.matchMedia('(prefers-color-scheme: dark)').matches
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

  // 主题对应的容器类名
  //
  // 用设计 token 而非 Tailwind slate 色（2026-10-14 修正）：容器是整页的底色，而页面自身用
  // --mobile-bg-primary（暖色墨纸体系）。两者不一致时，任何**没铺满视口高度**的页面（如
  // SettingsSubPage 的 h-full 在 min-h-screen 父级下退化成内容高）都会在下方露出一条
  // 不同色系的底（浅色主题是冷白 #F8FAFC 对比暖白 #F5F4F0，深色主题是冷 slate 对比暖黑）。
  // token 本身已按 .dark 切换，因此这里不再需要 dark: 变体。
  const themeClasses = computed(() => ({
    container: 'min-h-screen bg-[var(--mobile-bg-primary)] text-[var(--mobile-text-primary)]',
  }))

  return {
    isSystemDark,
    themeClasses,
    setupTheme,
    cleanupTheme,
  }
}

/**
 * 当前 App 是否深色（响应式：settings.ui.theme + 系统主题跟随）
 *
 * 供插件共享面（mobileApi.isDark，票 15）与终端主题解析使用；无副作用——
 * 不安装监视器，系统主题监听由 setupTheme 建立（isSystemDark 由其驱动）。
 */
export function useIsDark() {
  const settingsStore = useSettingsStore()
  return computed(() => {
    const theme = settingsStore.settings.ui.theme
    return theme === 'system' ? isSystemDark.value : theme === 'dark'
  })
}
