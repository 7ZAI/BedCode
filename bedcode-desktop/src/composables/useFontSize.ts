/**
 * 全局界面字体大小缩放
 *
 * 设置项 ui.font_size 以 px 存储（12 = "正常"档位），实际生效机制是
 * 在 :root 上设置 --ui-scale 缩放因子；所有界面文字尺寸均写成
 * calc(基准px * var(--ui-scale))，因此各元素保持原有大小比例等比缩放，
 * 不会出现统一大小的问题。终端字体由 terminal_font_size 独立控制，不在此列。
 *
 * Linux 平台额外叠加 PLATFORM_UI_SCALE 基线因子：替代原 html.platform-linux 的
 * CSS zoom（issue 06 —— zoom 破坏 xterm 鼠标坐标系，改为 font-size + --ui-scale
 * 体系），界面文字按设置档位 × 1.15 放大，观感与 zoom 前等价。
 */
import { watch } from 'vue'
import { useSettingsStore } from '@/stores/settings'
import { usePlatform } from '@/composables/usePlatform'

/** "正常"档位对应的基准字号（px），即当前代码中的默认界面文字大小 */
export const NORMAL_FONT_SIZE = 12
/** 滑杆档位范围：小(10) / 正常(12) / 大(14) / 超大(16) */
export const MIN_FONT_SIZE = 10
export const MAX_FONT_SIZE = 16
/** Linux 平台基线 UI 缩放因子（替代原 zoom:1.15，TerminalPreview 复用同一因子） */
export const PLATFORM_UI_SCALE = 1.15

function applyFontSize(size: number, isLinux: boolean) {
  const clamped = Math.min(MAX_FONT_SIZE, Math.max(MIN_FONT_SIZE, size))
  const scale = (clamped / NORMAL_FONT_SIZE) * (isLinux ? PLATFORM_UI_SCALE : 1)
  document.documentElement.style.setProperty('--ui-scale', String(scale), 'important')
}

export function useFontSize() {
  const settingsStore = useSettingsStore()
  const { platformInfo } = usePlatform()

  function currentFontSize(): number {
    return settingsStore.settings.ui.font_size || NORMAL_FONT_SIZE
  }

  // platformInfo 未初始化（platform === null）时不设置 --ui-scale，交给 CSS 默认
  // 值（splash 期）；platform 解析就绪后重算，消除与 main.ts initPlatform 的竞态。
  function recompute() {
    if (platformInfo.value.platform === null) return
    applyFontSize(currentFontSize(), platformInfo.value.isLinux)
  }

  function setupFontSize() {
    recompute()
  }

  // 监听全局字体大小设置变化（终端字体由终端设置独立控制，不在此列）
  watch(
    () => settingsStore.settings.ui.font_size,
    (newSize) => {
      if (newSize) {
        recompute()
      }
    },
  )

  // platform 解析完成后重算（含 Linux 基线因子）
  watch(
    () => platformInfo.value.platform,
    () => recompute(),
  )

  return {
    setupFontSize,
  }
}
