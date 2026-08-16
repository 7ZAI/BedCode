/**
 * 字节/速率格式化 (Mobile)
 *
 * 纯函数，翻译由调用方传入（composable 用 context.i18n.t，组件用注入的 t），
 * 避免模块级硬编码中文。
 */

/** 文件大小 → 人类可读字符串 */
export function formatBytes(bytes: number, t: (key: string, params?: Record<string, any>) => string): string {
  const b = Math.max(0, Number(bytes) || 0)
  if (b < 1024) return t('transfer.size.bytes', { value: b })
  const kb = b / 1024
  if (kb < 1024) return t('transfer.size.kb', { value: kb.toFixed(1) })
  const mb = kb / 1024
  if (mb < 1024) return t('transfer.size.mb', { value: mb.toFixed(1) })
  const gb = mb / 1024
  return t('transfer.size.gb', { value: gb.toFixed(2) })
}

/** 速率（字节/秒）→ 人类可读字符串 */
export function formatSpeed(bytesPerSec: number, t: (key: string, params?: Record<string, any>) => string): string {
  return `${formatBytes(bytesPerSec, t)}/s`
}

/** 进度百分比（0–100），total<=0 返回 null（未知大小） */
export function progressPercent(offset: number, total: number): number | null {
  if (total <= 0) return null
  return Math.min(100, Math.max(0, Math.round((offset / total) * 100)))
}
