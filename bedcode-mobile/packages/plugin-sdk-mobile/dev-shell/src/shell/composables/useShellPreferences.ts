/**
 * Dev Shell 壳 UI 偏好（与宿主 shell/composables/useShellPreferences.ts 同构）
 * -----------------------------------------------------------------------------
 * 归属：这是**壳的视觉偏好**（纯机制词汇，不含任何业务语义），因此不写宿主
 * AppConfig。持久化走 localStorage，与宿主同键同构：丢失只影响观感，不影响功能。
 *
 * 壁纸 id 是**稳定标识**而非样式值：渐变真源在 `styles/wallpaper.css` 的 --wall-*，
 * 这里只存 id。新增内置壁纸 = 加一条 id + 一条 token + 一条 i18n，三处同步。
 *
 * 与宿主的差异：宿主支持「自定义壁纸」档（用户经宿主命令选图并落盘，壳不能自行
 * 读盘）；dev-shell 没有宿主文件通道，`custom` 档恒定不可达——这里保留档位与
 * 校验逻辑只为了与宿主契约同构，落点由「从未写入过 path」天然挡住。
 */

import { computed, ref, watch, type ComputedRef, type Ref } from 'vue'
import { logger } from '../logger'

/** 壁纸档位（前四档与 wallpaper.css 的 --wall-* 一一对应；custom 保留为宿主契约对齐） */
export const SHELL_WALLPAPERS = ['none', 'aurora', 'dune', 'reef', 'custom'] as const
export type ShellWallpaper = (typeof SHELL_WALLPAPERS)[number]

/** 自定义档位的内部标识（与 i18n / token 无关，只用于分支） */
export const WALLPAPER_CUSTOM: ShellWallpaper = 'custom'

export const WALLPAPER_DEFAULT: ShellWallpaper = 'aurora'
/** 遮罩强度上限：超过 70% 壁纸几乎不可见，留着只是让用户白调 */
export const WALL_DIM_MAX = 70
export const WALL_DIM_DEFAULT = 30

interface ShellUiPrefs {
  wallpaper: ShellWallpaper
  /** 自定义壁纸的本地绝对路径；dev-shell 无宿主文件通道，恒为 null */
  wallpaperImage: string | null
  /** 壁纸之上的页面色遮罩强度（%，护文字对比度） */
  wallDim: number
}

const STORAGE_KEY = 'bedcode.shell-ui-prefs.v1'

function isWallpaper(value: unknown): value is ShellWallpaper {
  return typeof value === 'string' && (SHELL_WALLPAPERS as readonly string[]).includes(value)
}

function load(): ShellUiPrefs {
  const fallback: ShellUiPrefs = {
    wallpaper: WALLPAPER_DEFAULT,
    wallpaperImage: null,
    wallDim: WALL_DIM_DEFAULT,
  }
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return fallback
    const parsed = JSON.parse(raw) as Partial<ShellUiPrefs>
    // 只取已知键、逐项校验：损坏 / 旧版本 / 手改数据一律回落默认，不把脏值带进渲染
    const wallpaper = isWallpaper(parsed.wallpaper) ? parsed.wallpaper : fallback.wallpaper
    const wallpaperImage =
      typeof parsed.wallpaperImage === 'string' && parsed.wallpaperImage.length > 0
        ? parsed.wallpaperImage
        : null
    return {
      // custom 档没有图就等于没选：回落到默认渐变，而不是渲染一层空背景
      wallpaper: wallpaper === WALLPAPER_CUSTOM && wallpaperImage === null ? fallback.wallpaper : wallpaper,
      wallpaperImage,
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
/** 当前自定义壁纸路径（dev-shell 恒为 null） */
const wallpaperImage: ComputedRef<string | null> = computed(() => prefs.value.wallpaperImage)
/** 当前遮罩强度（0–70） */
const wallDim: ComputedRef<number> = computed(() => prefs.value.wallDim)

function setWallpaper(next: ShellWallpaper): void {
  prefs.value = { ...prefs.value, wallpaper: next }
}

/**
 * 记下自定义壁纸路径并切到 custom 档
 *
 * 路径由宿主命令（选图 + 落盘）给出；dev-shell 没有该通道，此函数仅保持宿主
 * 契约形状，不被任何 dev 界面调用。
 */
function setWallpaperImage(next: string | null): void {
  const image = next && next.length > 0 ? next : null
  if (image !== null) {
    prefs.value = { ...prefs.value, wallpaperImage: image, wallpaper: WALLPAPER_CUSTOM }
    return
  }
  // 移除自定义图：正停在 custom 档时回落到默认内置壁纸，避免「选了自定义却没有图」的空档
  prefs.value = {
    ...prefs.value,
    wallpaperImage: null,
    wallpaper:
      prefs.value.wallpaper === WALLPAPER_CUSTOM ? WALLPAPER_DEFAULT : prefs.value.wallpaper,
  }
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
  wallpaperImage: ComputedRef<string | null>
  wallDim: ComputedRef<number>
  setWallpaper: typeof setWallpaper
  setWallpaperImage: typeof setWallpaperImage
  setWallDim: typeof setWallDim
}

/** 获取壳 UI 偏好句柄（模块级单例，与壳内其它机制一致） */
export function useShellPreferences(): ShellPreferences {
  return { wallpaper, wallpaperImage, wallDim, setWallpaper, setWallpaperImage, setWallDim }
}