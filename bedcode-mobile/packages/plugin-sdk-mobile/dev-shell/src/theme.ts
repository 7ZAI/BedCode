/**
 * 主题管理（与宿主 useTheme 同语义）
 *
 * - 模式：dark（深色）/ light（浅色）/ system（跟随系统），持久化到 localStorage
 * - 应用方式与宿主一致：切换 <html> 的 .dark 类（mobile.css 中深色为 :root 默认，
 *   浅色作用于 html:not(.dark) .mobile-ui / .mobile-app）
 * - system 模式监听 prefers-color-scheme 变化实时切换
 */
import { ref } from 'vue'

export type ThemeMode = 'dark' | 'light' | 'system'

const THEME_KEY = 'bedcode-dev-shell:theme'

const theme = ref<ThemeMode>('system')
let systemQuery: MediaQueryList | null = null

/** 按当前模式 + 系统偏好应用 .dark 类 */
function applyTheme(): void {
  const isDark =
    theme.value === 'system'
      ? (systemQuery?.matches ?? true)
      : theme.value === 'dark'
  document.documentElement.classList.toggle('dark', isDark)
}

function setupSystemListener(): void {
  systemQuery?.removeEventListener('change', applyTheme)
  systemQuery = window.matchMedia('(prefers-color-scheme: dark)')
  systemQuery.addEventListener('change', applyTheme)
}

function teardownSystemListener(): void {
  systemQuery?.removeEventListener('change', applyTheme)
  systemQuery = null
}

/** 启动时调用（index.html 内联脚本已按持久化偏好消除首帧闪烁） */
export function initTheme(): void {
  try {
    const saved = localStorage.getItem(THEME_KEY)
    if (saved === 'dark' || saved === 'light' || saved === 'system') {
      theme.value = saved
    }
  } catch {
    // 隐私模式等场景使用默认 system
  }
  if (theme.value === 'system') setupSystemListener()
  applyTheme()
}

/** 切换主题模式（持久化；system 时建立系统偏好监听） */
export function setTheme(mode: ThemeMode): void {
  theme.value = mode
  try {
    localStorage.setItem(THEME_KEY, mode)
  } catch {
    // 忽略持久化失败
  }
  if (mode === 'system') {
    setupSystemListener()
  } else {
    teardownSystemListener()
  }
  applyTheme()
}

/** 供组件读取/切换（DevToolbar 分段按钮） */
export function useDevTheme() {
  return { theme, setTheme }
}
