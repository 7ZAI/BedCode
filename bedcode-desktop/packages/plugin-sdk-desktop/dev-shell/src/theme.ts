/**
 * 宿主界面设置管理（与宿主 useTheme / useFontSize 同语义）
 *
 * - 主题模式：light / dark / system，切换 <html> 的 .dark 类（style.css 中 :root.dark 变量组生效）
 * - 主题色板：warm / cool / forest / ocean / sunset / violet，切换 <html> 的 data-palette 属性
 * - 字体大小：以 px 存储（12 = "正常"档位），实际生效为 :root 的 --ui-scale 缩放因子
 *   （所有界面文字尺寸均写成 calc(基准px * var(--ui-scale))，等比缩放）
 * - 全部持久化到 localStorage，启动时由 initHostUi() 恢复，避免刷新回退
 */
import { ref } from 'vue'

export type ThemeMode = 'light' | 'dark' | 'system'
export type PaletteId = 'warm' | 'cool' | 'forest' | 'ocean' | 'sunset' | 'violet'

/** "正常"档位对应的基准字号（px），即默认界面文字大小 */
export const NORMAL_FONT_SIZE = 12
/** 滑杆档位范围：小(10) / 正常(12) / 大(14) / 超大(16) */
export const MIN_FONT_SIZE = 10
export const MAX_FONT_SIZE = 16

const THEME_KEY = 'bedcode-dev-shell:theme'
const PALETTE_KEY = 'bedcode-dev-shell:palette'
const FONT_SIZE_KEY = 'bedcode-dev-shell:font-size'

const theme = ref<ThemeMode>('system')
const palette = ref<PaletteId>('warm')
const fontSize = ref<number>(NORMAL_FONT_SIZE)

let systemQuery: MediaQueryList | null = null

/** 按当前模式 + 系统偏好应用 .dark 类（与宿主 useTheme.applyTheme 同逻辑） */
function applyTheme(): void {
  const isDark =
    theme.value === 'system'
      ? (systemQuery?.matches ?? false)
      : theme.value === 'dark'
  document.documentElement.classList.toggle('dark', isDark)
}

/** 应用色板：data-palette 属性驱动 CSS 变量组切换 */
function applyPalette(): void {
  document.documentElement.dataset.palette = palette.value
}

/** 应用字体大小：字号 / 基准 得到 --ui-scale 缩放因子 */
function applyFontSize(): void {
  const clamped = Math.min(MAX_FONT_SIZE, Math.max(MIN_FONT_SIZE, fontSize.value))
  document.documentElement.style.setProperty('--ui-scale', String(clamped / NORMAL_FONT_SIZE))
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

/** 启动时调用（main.ts）：恢复持久化的主题模式 / 色板 / 字体大小并应用 */
export function initHostUi(): void {
  try {
    const savedTheme = localStorage.getItem(THEME_KEY)
    if (savedTheme === 'light' || savedTheme === 'dark' || savedTheme === 'system') {
      theme.value = savedTheme
    }
    const savedPalette = localStorage.getItem(PALETTE_KEY)
    if (
      savedPalette === 'warm' || savedPalette === 'cool' || savedPalette === 'forest' ||
      savedPalette === 'ocean' || savedPalette === 'sunset' || savedPalette === 'violet'
    ) {
      palette.value = savedPalette
    }
    const savedSize = Number(localStorage.getItem(FONT_SIZE_KEY))
    if (Number.isFinite(savedSize) && savedSize >= MIN_FONT_SIZE && savedSize <= MAX_FONT_SIZE) {
      fontSize.value = savedSize
    }
  } catch {
    // 隐私模式等场景使用默认值
  }
  if (theme.value === 'system') setupSystemListener()
  applyTheme()
  applyPalette()
  applyFontSize()
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

/** 切换主题色板（持久化，即时生效） */
export function setPalette(next: PaletteId): void {
  palette.value = next
  try {
    localStorage.setItem(PALETTE_KEY, next)
  } catch {
    // 忽略持久化失败
  }
  applyPalette()
}

/** 切换字体大小（px，持久化，即时生效） */
export function setFontSize(size: number): void {
  fontSize.value = size
  try {
    localStorage.setItem(FONT_SIZE_KEY, String(size))
  } catch {
    // 忽略持久化失败
  }
  applyFontSize()
}

/** 供设置页读取/切换（响应式） */
export function useHostUi() {
  return { theme, palette, fontSize, setTheme, setPalette, setFontSize }
}
