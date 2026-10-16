/**
 * Dev Shell 壳展示层小工具（纯函数，与宿主 shell/utils.ts 同构）
 *
 * 只放无状态纯函数，方便单测与复用。
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

/** 图标形态：emoji / SVG path d / 图片路径 / 无 */
export type IconKind = 'emoji' | 'path' | 'image' | 'none'

/**
 * 图片图标的扩展名判据（与宿主 shell/utils.ts 同源）
 *
 * 调试数据源读不到插件包内资源，会把图片路径换成通用 path 图形（见 adapters/devAppSource.ts
 * 的 resolveIcon），因此壳内当前不会从 `icon` 命中 image；保留判据是为了与宿主同构——
 * 数据源给出已解析 iconUrl 时，两种运行环境走同一条渲染分支。
 */
const IMAGE_ICON_RE = /\.(png|jpe?g|webp|gif|svg)$/i

/**
 * 判断图标形态
 *
 * 与宿主 utils.iconKindOf 同构，判据顺序一致：图片路径必须先于 path 判定——「icon.svg」
 * 这类值不含路径指令字符，漏掉这一步会被当 emoji 字面量渲染。
 */
export function iconKindOf(icon: string | undefined): IconKind {
  if (!icon) return 'none'
  if (IMAGE_ICON_RE.test(icon.trim())) return 'image'
  if (/^[MmLlHhVvCcSsQqTtAaZz0-9\s.,+-]+$/.test(icon)) return 'path'
  return 'emoji'
}