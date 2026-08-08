/**
 * 图标工具：插件图标约定为 emoji 或 SVG path d 字符串（Heroicons outline 风格，viewBox=0 0 24 24）
 */
export function isSvgIcon(icon?: string): boolean {
  return typeof icon === 'string' && icon.startsWith('M')
}
