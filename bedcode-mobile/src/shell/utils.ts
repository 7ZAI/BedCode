/**
 * 宿主壳展示层小工具
 * -----------------------------------------------------------------------------
 * 只放纯函数（无状态、无副作用），方便单测与复用。
 */

/** 字节数格式化（1024 进制，保留 1 位小数；<1KB 直接给 B） */
export function formatBytes(bytes: number | undefined): string {
  if (bytes === undefined || bytes === null || Number.isNaN(bytes)) return '—'
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let value = bytes / 1024
  let unitIndex = 0
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex += 1
  }
  return `${value.toFixed(1)} ${units[unitIndex]}`
}

/**
 * 首字母回退（无图标应用用）
 *
 * 取首个字符而非首字母：中文名取首字比取不到拉丁字母更可读。
 */
export function initialOf(name: string): string {
  const trimmed = (name ?? '').trim()
  return trimmed ? trimmed.slice(0, 1).toUpperCase() : '?'
}

/**
 * 判断图标形态：emoji / SVG path d / 图片路径 / 无
 *
 * 三类来源的优先级与插件清单契约一致（PluginInfo.icon）：
 *   · 图片路径（icon.svg / icon.png …）→ 由调用方经 asset protocol 取图，相对插件目录解析
 *   · SVG path d → 直接画 path（判据：只含路径指令字符）
 *   · 其余字符串（emoji / 短文本）→ 当文本渲染
 *
 * 关键：图片路径必须排在 path 之前判定。`icon.svg` 这类值不含路径指令字符，
 * 旧实现把它当 emoji 文本渲染，界面上出现字面量 “icon.svg”（实测三个内置应用全部命中）。
 */
export type IconKind = 'emoji' | 'path' | 'image' | 'none'

/** 图片图标的扩展名判据（与桌面端 PluginIcon 同源） */
const IMAGE_ICON_RE = /\.(png|jpe?g|webp|gif|svg)$/i

export function iconKindOf(icon: string | undefined): IconKind {
  if (!icon) return 'none'
  if (IMAGE_ICON_RE.test(icon.trim())) return 'image'
  // path d 只由指令字母、数字、空白与 . , - + e 组成；emoji 与文件路径都不会满足
  if (/^[MmLlHhVvCcSsQqTtAaZz0-9\s.,+-]+$/.test(icon)) return 'path'
  return 'emoji'
}
