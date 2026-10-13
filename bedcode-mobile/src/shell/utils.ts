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
 * 判断图标形态：emoji / SVG path d / 无
 *
 * SVG path d 的判据是「只含路径指令字符」，避免把 emoji 或图片路径误判成 path。
 */
export type IconKind = 'emoji' | 'path' | 'none'

export function iconKindOf(icon: string | undefined): IconKind {
  if (!icon) return 'none'
  // path d 只由指令字母、数字、空白与 . , - + e 组成；emoji 与文件路径都不会满足
  if (/^[MmLlHhVvCcSsQqTtAaZz0-9\s.,+-]+$/.test(icon)) return 'path'
  return 'emoji'
}
