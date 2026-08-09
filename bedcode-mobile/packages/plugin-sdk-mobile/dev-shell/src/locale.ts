/**
 * 语言管理（与 theme.ts 同语义）
 *
 * - 支持 zh-CN / en，持久化到 localStorage
 * - main.ts 创建 i18n 时读取持久化值，组件切换时写回
 */
export type DevLocale = 'zh-CN' | 'en'

const LOCALE_KEY = 'bedcode-dev-shell:locale'

/** 读取持久化语言（非法值回退默认 zh-CN） */
export function readSavedLocale(): DevLocale {
  try {
    const saved = localStorage.getItem(LOCALE_KEY)
    if (saved === 'en' || saved === 'zh-CN') return saved
  } catch {
    // 隐私模式等场景使用默认 zh-CN
  }
  return 'zh-CN'
}

/** 保存语言选择（持久化失败静默忽略） */
export function saveLocale(next: DevLocale): void {
  try {
    localStorage.setItem(LOCALE_KEY, next)
  } catch {
    // 忽略持久化失败
  }
}
