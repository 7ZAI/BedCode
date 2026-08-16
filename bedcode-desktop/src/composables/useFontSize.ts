/**
 * 全局界面字体大小缩放
 *
 * 设置项 ui.font_size 以 px 存储（12 = "正常"档位），实际生效机制是
 * 在 :root 上设置 --ui-scale 缩放因子；所有界面文字尺寸均写成
 * calc(基准px * var(--ui-scale))，因此各元素保持原有大小比例等比缩放，
 * 不会出现统一大小的问题。终端字体由 terminal_font_size 独立控制，不在此列。
 */
import { watch } from 'vue'
import { useSettingsStore } from '@/stores/settings'

/** "正常"档位对应的基准字号（px），即当前代码中的默认界面文字大小 */
export const NORMAL_FONT_SIZE = 12
/** 滑杆档位范围：小(10) / 正常(12) / 大(14) / 超大(16) */
export const MIN_FONT_SIZE = 10
export const MAX_FONT_SIZE = 16

function applyFontSize(size: number) {
  const clamped = Math.min(MAX_FONT_SIZE, Math.max(MIN_FONT_SIZE, size))
  const scale = clamped / NORMAL_FONT_SIZE
  document.documentElement.style.setProperty('--ui-scale', String(scale), 'important')
}

export function useFontSize() {
  const settingsStore = useSettingsStore()

  function setupFontSize() {
    const fontSize = settingsStore.settings.ui.font_size || NORMAL_FONT_SIZE
    applyFontSize(fontSize)
  }

  // 监听全局字体大小设置变化（终端字体由终端设置独立控制，不在此列）
  watch(() => settingsStore.settings.ui.font_size, (newSize) => {
    if (newSize) {
      applyFontSize(newSize)
    }
  })

  return {
    setupFontSize,
  }
}
