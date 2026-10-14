/**
 * 壳 UI 偏好（主屏壁纸 / 遮罩强度）
 * -----------------------------------------------------------------------------
 * 归属：这是**壳的视觉偏好**（纯机制词汇，不含任何业务语义），因此不写宿主
 * AppConfig——引擎面不加壳词汇（与桌面壳 `useShellSettings.ts` 同口径，
 * AGENTS §5.1 B1/B5）。持久化走 localStorage：偏好丢失只影响观感，不影响功能；
 * 隐私模式等写入失败按 warn 记录，不静默吞掉（壳 L4 纪律）。
 *
 * 壁纸 id 是**稳定标识**而非样式值：渐变真源在 `styles/shell.css` 的 --wall-*，
 * 这里只存 id。新增壁纸 = 加一条 id + 一条 token + 一条 i18n，三处同步。
 */

import { computed, ref, watch, type ComputedRef, type Ref } from 'vue'
import { logger } from '@/utils/frontendLogger'

/** 壁纸档位（与 shell.css 的 --wall-* 一一对应；none = 纯页面底色） */
export const SHELL_WALLPAPERS = ['none', 'aurora', 'dune', 'reef'] as const
export type ShellWallpaper = (typeof SHELL_WALLPAPERS)[number]

export const WALLPAPER_DEFAULT: ShellWallpaper = 'aurora'
/** 遮罩强度上限：超过 70% 壁纸几乎不可见，留着只是让用户白调 */
export const WALL_DIM_MAX = 70
export const WALL_DIM_DEFAULT = 30

interface ShellUiPrefs {
  wallpaper: ShellWallpaper
  /** 壁纸之上的页面色遮罩强度（%，护文字对比度） */
  wallDim: number
}

const STORAGE_KEY = 'bedcode.shell-ui-prefs.v1'

function isWallpaper(value: unknown): value is ShellWallpaper {
  return typeof value === 'string' && (SHELL_WALLPAPERS as readonly string[]).includes(value)
}

function load(): ShellUiPrefs {
  const fallback: ShellUiPrefs = { wallpaper: WALLPAPER_DEFAULT, wallDim: WALL_DIM_DEFAULT }
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return fallback
    const parsed = JSON.parse(raw) as Partial<ShellUiPrefs>
    // 只取已知键、逐项校验：损坏 / 旧版本 / 手改数据一律回落默认，不把脏值带进渲染
    return {
      wallpaper: isWallpaper(parsed.wallpaper) ? parsed.wallpaper : fallback.wallpaper,
      wallDim:
        typeof parsed.wallDim === 'number' && parsed.wallDim >= 0 && parsed.wallDim <= WALL_DIM_MAX
          ? Math.round(parsed.wallDim)
          : fallback.wallDim,
    }
  } catch (e) {
    logger.warn('[Shell] shell ui prefs unreadable; falling back to defaults', e)
    return fallback
  }
}

const prefs: Ref<ShellUiPrefs> = ref(load())

watch(
  prefs,
  (value) => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(value))
    } catch (e) {
      // localStorage 不可用（隐私模式 / 配额满）：偏好仅本次会话生效，不打断主流程
      logger.warn('[Shell] shell ui prefs not persisted', e)
    }
  },
  { deep: true },
)

/** 当前壁纸 id */
const wallpaper: ComputedRef<ShellWallpaper> = computed(() => prefs.value.wallpaper)
/** 当前遮罩强度（0–70） */
const wallDim: ComputedRef<number> = computed(() => prefs.value.wallDim)

function setWallpaper(next: ShellWallpaper): void {
  prefs.value = { ...prefs.value, wallpaper: next }
}

/** 设置遮罩强度；越界值钳制到 [0, WALL_DIM_MAX]，不抛错（滑杆可能给出小数） */
function setWallDim(next: number): void {
  const clamped = Math.min(WALL_DIM_MAX, Math.max(0, Math.round(next)))
  prefs.value = { ...prefs.value, wallDim: clamped }
}

/** 壁纸选择器直接消费的档位清单（id + i18n key 由调用方拼） */
export const SHELL_WALLPAPER_IDS: readonly ShellWallpaper[] = SHELL_WALLPAPERS

export interface ShellPreferences {
  wallpaper: ComputedRef<ShellWallpaper>
  wallDim: ComputedRef<number>
  setWallpaper: typeof setWallpaper
  setWallDim: typeof setWallDim
}

/** 获取壳 UI 偏好句柄（模块级单例，与壳内其它机制一致） */
export function useShellPreferences(): ShellPreferences {
  return { wallpaper, wallDim, setWallpaper, setWallDim }
}
