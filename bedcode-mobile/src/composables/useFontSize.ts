import { watch } from 'vue'
import { useSettingsStore } from '@/stores/settings'

function applyFontSize(size: number) {
  const root = document.documentElement
  root.style.setProperty('--font-size-base', `${size}px`, 'important')
  root.style.setProperty('--global-font-size', `${size}px`, 'important')

  const small = Math.max(10, Math.round(size * 0.85))
  const large = Math.round(size * 1.15)
  const xl = Math.round(size * 1.3)
  const xs = Math.max(9, Math.round(size * 0.75))

  root.style.setProperty('--font-size-xs', `${xs}px`, 'important')
  root.style.setProperty('--font-size-sm', `${small}px`, 'important')
  root.style.setProperty('--font-size-base', `${size}px`, 'important')
  root.style.setProperty('--font-size-lg', `${large}px`, 'important')
  root.style.setProperty('--font-size-xl', `${xl}px`, 'important')
}

export function useFontSize() {
  const settingsStore = useSettingsStore()

  function setupFontSize() {
    const fontSize = settingsStore.settings.ui.terminal_font_size || 14
    applyFontSize(fontSize)
  }

  // 监听字体大小设置变化
  watch(() => settingsStore.settings.ui.terminal_font_size, (newSize) => {
    if (newSize) {
      applyFontSize(newSize)
    }
  })

  return {
    setupFontSize,
  }
}
